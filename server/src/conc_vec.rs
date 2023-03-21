use std::alloc::{alloc, dealloc, Layout};
use std::ops::Deref;
use std::process::abort;
use std::ptr;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use crossbeam_utils::Backoff;
use swap_arc::{SwapArc, SwapArcAnyMeta, SwapArcOption};

pub struct ConcurrentVec<T> {
    alloc: SwapArcOption<SizedAlloc<T>>,
    len: AtomicUsize,
    push_far: AtomicUsize,
    pop_far: AtomicUsize,
    guard: AtomicUsize,
}

const PUSH_OR_ITER_FLAG: usize = 1 << (usize::BITS as usize - 1);
const POP_FLAG: usize = 1 << (usize::BITS as usize - 2);
const LOCK_FLAG: usize = 1 << (usize::BITS as usize - 3);

const PUSH_OR_ITER_INC: usize = 1 << 0;
const POP_INC: usize = 1 << ((usize::BITS as usize - 3) / 2);
const PUSH_OR_ITER_INC_BITS: usize = (POP_INC - PUSH_OR_ITER_INC) >> 2; // leave the two msb free (always 0), so we have a buffer
const POP_INC_BITS: usize = ((1 << (usize::BITS as usize - 3)) - POP_INC) >> 2; // leave the two msb free (always 0), so we have a buffer

const SCALE_FACTOR: usize = 2;
const INITIAL_CAP: usize = 8;

impl<T> ConcurrentVec<T> {
    pub fn new() -> Self {
        Self {
            alloc: SwapArcAnyMeta::new(None),
            len: Default::default(),
            push_far: Default::default(),
            pop_far: Default::default(),
            guard: Default::default(),
        }
    }

    pub fn push(&self, val: T) {
        // inc push_or_iter counter
        let mut curr_guard = self.guard.fetch_add(PUSH_OR_ITER_INC, Ordering::Acquire);
        while curr_guard & PUSH_OR_ITER_FLAG == 0 {
            let mut backoff = Backoff::new();
            // wait until the POP_FLAG and LOCK_FLAG are unset
            while curr_guard & (POP_FLAG | LOCK_FLAG) != 0 {
                backoff.snooze();
                curr_guard = self.guard.load(Ordering::Acquire);
            }
            match self.guard.compare_exchange(curr_guard, (curr_guard & !(POP_FLAG | LOCK_FLAG)) | PUSH_OR_ITER_FLAG, Ordering::AcqRel, Ordering::Acquire) {
                Ok(_) => break,
                Err(val) => {
                    curr_guard = val;
                }
            }
        }
        let pop_far = self.pop_far.load(Ordering::Acquire);
        let push_far = self.push_far.fetch_add(1, Ordering::Acquire);
        let slot = push_far - pop_far;

        // check if we have hit the soft guard bit, if so, recover
        if push_far == (1 << (usize::BITS - 2)) {
            // recover by decreasing the current counter
            self.pop_far.store(0, Ordering::Release);
            self.push_far.fetch_sub(pop_far, Ordering::Release);
        }

        // check if we have hit the hard guard bit, if so, abort.
        if push_far >= (1 << (usize::BITS - 1)) {
            // we can't recover safely anymore because the vec grew too quickly.
            abort();
        }

        if let Some(cap) = self.alloc.load().as_ref() {
            let size = cap.size;
            if size == slot {
                let old_alloc = cap.deref().ptr;
                drop(cap);
                unsafe { resize(self, size, old_alloc, slot); }

                #[cold]
                unsafe fn resize<T>(slf: &ConcurrentVec<T>, size: usize, old_alloc: *mut T, slot: usize) {
                    // wait until all previous writes finished
                    let mut backoff = Backoff::new();
                    while slf.len.load(Ordering::Acquire) != slot {
                        backoff.snooze();
                    }
                    let alloc = unsafe { alloc(Layout::array::<T>(size * SCALE_FACTOR).unwrap()) }.cast();
                    unsafe { ptr::copy_nonoverlapping(old_alloc, alloc, size); }
                    slf.alloc.store(Some(Arc::new(SizedAlloc::new(alloc.cast(), size * SCALE_FACTOR))));

                    let mut backoff = Backoff::new();
                    // wait for the resize to be performed
                    while slf.alloc.load().as_ref().as_ref().unwrap().size <= slot {
                        backoff.snooze();
                    }
                }
            } else if cap.size < slot {
                drop(cap);
                let mut backoff = Backoff::new();
                // wait for the resize to be performed
                while self.alloc.load().as_ref().as_ref().unwrap().size <= slot {
                    backoff.snooze();
                }
            }
        } else {
            if slot == 0 {
                let alloc = unsafe { alloc(Layout::array::<T>(INITIAL_CAP).unwrap()) };
                self.alloc.store(Some(Arc::new(SizedAlloc::new(alloc.cast(), INITIAL_CAP))));
            }
            let mut backoff = Backoff::new();
            // wait for the resize to be performed
            while self.alloc.load().as_ref().is_none() {
                backoff.snooze();
            }
        }
        loop {
            let cap = self.alloc.load();
            let cap = cap.as_ref().as_ref().unwrap();
            if cap.size <= slot {
                continue;
            }
            unsafe { cap.deref().ptr.add(slot).write(val); }
            self.len.fetch_add(1, Ordering::Release);
            break;
        }

        let mut guard_end = self.guard.fetch_sub(PUSH_OR_ITER_INC, Ordering::Release) - PUSH_OR_ITER_INC;
        // check if somebody else will unset the flag
        while guard_end & PUSH_OR_ITER_INC_BITS == 0 && guard_end & PUSH_OR_ITER_FLAG != 0 {
            // get rid of the flag if there are no more on-going pushes or iterations
            match self.guard.compare_exchange(guard_end, guard_end & !PUSH_OR_ITER_FLAG, Ordering::Release, Ordering::Relaxed) {
                Ok(_) => break,
                Err(err) => {
                    guard_end = err;
                }
            }
        }
    }

    pub fn pop(&self) -> Option<T> {
        // dec pop counter
        let mut curr_guard = self.guard.fetch_add(POP_INC, Ordering::Acquire);
        while curr_guard & POP_FLAG == 0 {
            let mut backoff = Backoff::new();
            // wait until the PUSH_OR_ITER_FLAG and LOCK_FLAG are unset
            while curr_guard & (PUSH_OR_ITER_FLAG | LOCK_FLAG) != 0 {
                backoff.snooze();
                curr_guard = self.guard.load(Ordering::Acquire);
            }
            match self.guard.compare_exchange(curr_guard, curr_guard | POP_FLAG, Ordering::Acquire, Ordering::Acquire) {
                Ok(_) => break,
                Err(val) => {
                    curr_guard = val;
                }
            }
        }
        let push_far = self.push_far.load(Ordering::Acquire);
        let pop_far = self.pop_far.fetch_add(1, Ordering::AcqRel);
        // the idx of the last inhabited element (take the pushed counter and subtract the popped counter from it,
        // and subtract 1 from it in order to adjust for converting from count to index)
        let slot = push_far - pop_far - 1;

        // check if we have hit the hard guard bit, if so, abort.
        if push_far >= (1 << (usize::BITS - 1)) {
            // we can't recover safely anymore because the vec grew too quickly.
            abort();
        }

        let ret = if let Some(cap) = self.alloc.load().as_ref() {
            let val = unsafe { cap.deref().ptr.add(slot).read() };
            self.len.fetch_sub(1, Ordering::Release);
            Some(val)
        } else {
            None
        };
        let mut guard_end = self.guard.fetch_sub(POP_INC, Ordering::Release) - POP_INC;
        // check if somebody else will unset the flag
        while guard_end & POP_INC_BITS == 0 && guard_end & POP_FLAG != 0 {
            // get rid of the flag if there are no more on-going pushes or iterations
            match self.guard.compare_exchange(guard_end, guard_end & !POP_FLAG, Ordering::Release, Ordering::Relaxed) {
                Ok(_) => break,
                Err(err) => {
                    guard_end = err;
                }
            }
        }

        ret
    }

    pub fn remove(&self, idx: usize) -> Option<T> {
        let mut backoff = Backoff::new();
        while self.guard.compare_exchange_weak(0, LOCK_FLAG, Ordering::Acquire, Ordering::Relaxed).is_err() {
            backoff.snooze();
        }

        let ret = if let Some(alloc) = self.alloc.load().deref() {
            let val = unsafe { alloc.deref().ptr.add(idx).read() };
            let trailing = alloc.size - idx - 1;
            if trailing != 0 {
                unsafe { ptr::copy(alloc.deref().ptr.add(idx + 1), alloc.deref().ptr.add(idx), trailing) };
            }
            self.len.store(self.len() - 1, Ordering::Release);
            Some(val)
        } else {
            None
        };

        self.guard.fetch_and(!LOCK_FLAG, Ordering::Release);

        ret
    }

    pub fn insert(&self, idx: usize, val: T) {
        let mut backoff = Backoff::new();
        while self.guard.compare_exchange_weak(0, LOCK_FLAG, Ordering::Acquire, Ordering::Relaxed).is_err() {
            backoff.snooze();
        }

        let len = self.len.load(Ordering::Acquire);
        if len < idx {
            panic!("Index out of bounds ({}) while len is {}.", idx, len);
        }

        let req = len + 1;
        if let Some(cap) = self.alloc.load().as_ref() {
            let size = cap.size;
            if size < req {
                let old_alloc = cap.deref().ptr;
                drop(cap);
                unsafe { resize(self, size, old_alloc); }

                #[cold]
                unsafe fn resize<T>(slf: &ConcurrentVec<T>, size: usize, old_alloc: *mut T) {
                    let alloc = unsafe { alloc(Layout::array::<T>(size * SCALE_FACTOR).unwrap()) }.cast();
                    unsafe { ptr::copy_nonoverlapping(old_alloc, alloc, size); }
                    slf.alloc.store(Some(Arc::new(SizedAlloc::new(alloc.cast(), size * SCALE_FACTOR))));
                }
            }
        } else {
            let alloc = unsafe { alloc(Layout::array::<T>(INITIAL_CAP).unwrap()) };
            self.alloc.store(Some(Arc::new(SizedAlloc::new(alloc.cast(), INITIAL_CAP))));

            let mut backoff = Backoff::new();
            // wait for the resize to be performed
            while self.alloc.load().as_ref().is_none() {
                backoff.snooze();
            }
        }

        let alloc = self.alloc.load();
        let alloc = alloc.as_ref().as_ref().unwrap();
        let alloc = alloc.deref();
        let ptr = alloc.ptr;
        let len = self.len();
        unsafe { ptr::copy(ptr.add(idx), ptr.add(idx).add(1), len - idx); }
        unsafe { ptr.add(idx).write(val); }
        self.len.store(len + 1, Ordering::Release);

        self.guard.fetch_and(!LOCK_FLAG, Ordering::Release);
    }

    pub fn iter(&self) -> Iter<'_, T> {
        // inc push_or_iter counter
        let mut curr_guard = self.guard.fetch_add(PUSH_OR_ITER_INC, Ordering::Acquire);
        while curr_guard & PUSH_OR_ITER_FLAG == 0 {
            let mut backoff = Backoff::new();
            // wait until the POP_FLAG and REM_FLAG are unset
            while curr_guard & (POP_FLAG | LOCK_FLAG) != 0 {
                backoff.snooze();
                curr_guard = self.guard.load(Ordering::Acquire);
            }
            match self.guard.compare_exchange(curr_guard, (curr_guard & !(POP_FLAG | LOCK_FLAG)) | PUSH_OR_ITER_FLAG, Ordering::AcqRel, Ordering::Acquire) {
                Ok(_) => break,
                Err(val) => {
                    curr_guard = val;
                }
            }
        }
        Iter {
            parent: self,
            idx: 0,
        }
    }

    pub fn len(&self) -> usize {
        self.len.load(Ordering::Acquire)
    }

    #[inline]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

impl<T> Drop for ConcurrentVec<T> {
    fn drop(&mut self) {
        if let Some(alloc) = self.alloc.load().as_ref() {
            alloc.set_partially_init(*self.len.get_mut());
        }
    }
}

struct SizedAlloc<T> {
    size: usize,
    ptr: *mut T,
    len: AtomicUsize,
}

impl<T> SizedAlloc<T> {

    #[inline]
    const fn new(ptr: *mut T, size: usize) -> Self {
        Self {
            size,
            ptr,
            len: AtomicUsize::new(0),
        }
    }

    fn set_partially_init(&self, len: usize) {
        if len > self.size {
            // this is not allowed
            abort();
        }
        self.len.store(len, Ordering::Release);
    }

}

impl<T> Drop for SizedAlloc<T> {
    fn drop(&mut self) {
        for x in 0..*self.len.get_mut() {
            unsafe { self.ptr.offset(x as isize).drop_in_place(); }
        }
        unsafe { dealloc(self.ptr.cast(), Layout::array::<T>(self.size).unwrap()) };
    }
}

unsafe impl<T> Send for SizedAlloc<T> {}
unsafe impl<T> Sync for SizedAlloc<T> {}

pub struct Iter<'a, T> {
    parent: &'a ConcurrentVec<T>,
    idx: usize,
}

impl<'a, T> Iterator for Iter<'a, T> {
    type Item = &'a T;

    fn next(&mut self) -> Option<Self::Item> {
        let len = self.parent.len.load(Ordering::Acquire);
        if len <= self.idx {
            return None;
        }

        let ret = unsafe { self.parent.alloc.load().as_ref().as_ref().unwrap().deref().ptr.add(self.idx) };

        self.idx += 1;

        Some(unsafe { ret.as_ref().unwrap() })
    }
}

impl<'a, T> Drop for Iter<'a, T> {
    fn drop(&mut self) {
        // decrement the `push_or_iter` counter
        self.parent.guard.fetch_sub(PUSH_OR_ITER_INC, Ordering::Release);
    }
}
