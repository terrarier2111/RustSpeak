use std::mem;
use std::ptr::null_mut;
use std::sync::atomic::{AtomicPtr, Ordering};

pub struct ConcurrentOnceCell<T> {
    ptr: AtomicPtr<T>,
}

impl<T> ConcurrentOnceCell<T> {

    pub fn new() -> Self {
        Self {
            ptr: AtomicPtr::new(null_mut()),
        }
    }

    pub fn is_init(&self) -> bool {
        !self.ptr.load(Ordering::Acquire).is_null()
    }

    pub fn try_init(&self, val: T) -> Result<(), T> {
        let mut sized = crate::sized_box::SizedBox::new(val);
        let ptr = sized.as_mut() as *mut T;
        match self.ptr.compare_exchange(null_mut(), ptr, Ordering::Release, Ordering::Relaxed) {
            Ok(_) => {
                mem::forget(sized);
                Ok(())
            }
            Err(_) => {
                Err(sized.into_inner())
            }
        }
    }

    pub fn get(&self) -> Option<&T> {
        unsafe { self.ptr.load(Ordering::Acquire).as_ref() }
    }

}

impl<T> Drop for ConcurrentOnceCell<T> {
    fn drop(&mut self) {
        let ptr = *self.ptr.get_mut();
        if !ptr.is_null() {
            unsafe { ptr.drop_in_place(); }
        }
    }
}
