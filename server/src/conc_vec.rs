use std::alloc::{dealloc, Layout};
use std::process::abort;
use std::sync::atomic::{AtomicUsize, Ordering};
use swap_arc::{SwapArc, SwapArcOption};

pub struct ConcurrentVec<T> {
    alloc: SwapArcOption<SizedAlloc<T>>,
    len: AtomicUsize,
    push_far: AtomicUsize,
    pop_far: AtomicUsize,
    guard: AtomicUsize,
}

const PUSH_OR_ITER_FLAG: usize = 1 << (usize::BITS - 1);
const POP_FLAG: usize = 1 << (usize::BITS - 2);

const PUSH_OR_ITER_INC: usize = 1 << 0;
const POP_INC: usize = 1 << ((usize::BITS - 2) / 2);

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
        let mut curr_guard = self.guard.fetch_add(PUSH_OR_ITER_INC, Ordering::AcqRel);
        while curr_guard & PUSH_OR_ITER_FLAG == 0 {
            match self.guard.compare_exchange(curr_guard & !POP_FLAG, (curr_guard & !POP_FLAG) | PUSH_OR_ITER_FLAG, Ordering::AcqRel, Ordering::Acquire) {
                Ok(_) => break,
                Err(val) => {
                    curr_guard = val;
                }
            }
        }
        let slot = self.push_far.fetch_add(1, Ordering::AcqRel);

    }

    pub fn pop(&self) -> Option<T> {

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
    len: usize,
}

impl<T> SizedAlloc<T> {

    fn new(ptr: *mut T, size: usize) -> Self {
        Self {
            size,
            ptr,
            len: 0,
        }
    }

    fn set_partially_init(&mut self, len: usize) {
        if len > size {
            // this is not allowed
            abort();
        }
        self.len = len;
    }

}

impl<T> Drop for SizedAlloc<T> {
    fn drop(&mut self) {
        for x in 0..self.len {
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
