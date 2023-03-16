use std::alloc::{alloc, dealloc, Layout};
use std::ops::Deref;
use std::process::abort;
use std::ptr;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use crossbeam_utils::Backoff;
use swap_arc::{SwapArc, SwapArcOption};

pub struct ConcurrentVec<T> {
    alloc: SwapArcOption<SizedAlloc<T>>,
    len: AtomicUsize,
    push_far: AtomicUsize,
    pop_far: AtomicUsize,
    guard: AtomicUsize,
}

const PUSH_OR_ITER_FLAG: usize = 1 << (usize::BITS as usize - 1);
const POP_FLAG: usize = 1 << (usize::BITS as usize - 2);
const REM_FLAG: usize = 1 << (usize::BITS as usize - 3);

const PUSH_INC: usize = 1 << 0;
const ITER_INC: usize = 1 << ((usize::BITS as usize - 3) / 3 * 1);
const POP_INC: usize = 1 << ((usize::BITS as usize - 3) / 3 * 2);
const PUSH_INC_BITS: usize = (ITER_INC - PUSH_INC) >> 1; // leave the msb free (always 0), so we have a buffer
const ITER_INC_BITS: usize = (POP_INC - ITER_INC) >> 1; // leave the msb free (always 0), so we have a buffer
const POP_INC_BITS: usize = ((usize::BITS as usize - 3) - POP_INC) >> 1; // leave the msb free (always 0), so we have a buffer

const SCALE_FACTOR: usize = 2;
const INITIAL_CAP: usize = 8;

impl<T> ConcurrentVec<T> {
    pub fn new() -> Self {
        Self {
            alloc: SwapArcOption::empty(),
            len: Default::default(),
            push_far: Default::default(),
            pop_far: Default::default(),
            guard: Default::default(),
        }
    }

    pub fn push(&self, val: T) {
        // inc push_or_iter counter
        let mut curr_guard = self.guard.fetch_add(PUSH_INC, Ordering::Acquire);
        while curr_guard & PUSH_OR_ITER_FLAG == 0 {
            let mut backoff = Backoff::new();
            // wait until the POP_FLAG is unset
            while curr_guard & POP_FLAG != 0 {
                backoff.snooze();
                curr_guard = self.guard.load(Ordering::Acquire);
            }
            match self.guard.compare_exchange(curr_guard, (curr_guard & !POP_FLAG) | PUSH_OR_ITER_FLAG, Ordering::AcqRel, Ordering::Acquire) {
                Ok(_) => break,
                Err(val) => {
                    curr_guard = val;
                }
            }
        }
        let slot = self.push_far.fetch_add(1, Ordering::AcqRel);
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
                    while slf.len.load(Ordering::Acquire) != slot - 1 {
                        backoff.snooze();
                    }
                    let alloc = unsafe { alloc(Layout::array::<T>(size * SCALE_FACTOR).unwrap()) };
                    unsafe { ptr::copy_nonoverlapping(old_alloc, alloc, size); }
                    slf.alloc.store(Some(Arc::new(SizedAlloc::new(alloc.cast(), size * SCALE_FACTOR))));
                }
            } else if cap.size < slot {
                drop(cap);
                let mut backoff = Backoff::new();
                // wait for the resize to be performed
                while self.alloc.load().as_ref().unwrap().size < slot {
                    backoff.snooze();
                }
            }
            let cap = self.alloc.load().as_ref().unwrap();
            unsafe { cap.deref().ptr.add(slot).write(val); }
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
        let mut guard_end = self.guard.fetch_sub(PUSH_INC, Ordering::Release);
        while (guard_end & !PUSH_OR_ITER_FLAG) == 0 {
            // get rid of the flag if there are no more on-going pushes or iterations
            match self.guard.compare_exchange(PUSH_OR_ITER_FLAG, 0, Ordering::Release, Ordering::Relaxed) {
                Ok(_) => break,
                Err(err) => {
                    if (err & (PUSH_INC_BITS | ITER_INC_BITS)) != 0 {
                        // somebody else will unset the flag
                        break;
                    }
                    guard_end = err;
                }
            }
        }
    }

    pub fn pop(&self) -> Option<T> {

    }

    pub fn remove(&self, idx: usize) -> Option<T> {
        let mut backoff = Backoff::new();
        while self.guard.compare_exchange_weak(0, REM_FLAG, Ordering::Acquire, Ordering::Relaxed).is_err() {
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

        self.guard.fetch_and(!REM_FLAG, Ordering::Release);

        ret
    }

    pub fn iter(&self) -> Iter<'_, T> {

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
        if let Some(alloc) = self.alloc.get_mut() {
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
        if len > size {
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
        unsafe { dealloc(self.ptr, Layout::array::<T>(self.size).unwrap()) };
    }
}

pub struct Iter<'a, T> {
    parent: &'a ConcurrentVec<T>,
    idx: usize,
}

impl<'a, T> Iterator for Iter<'a, T> {
    type Item = &'a T;

    fn next(&mut self) -> Option<Self::Item> {
        todo!()
    }
}
