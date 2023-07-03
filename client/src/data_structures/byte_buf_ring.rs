use std::alloc::{alloc, Layout};
use std::marker::PhantomData;
use std::{ptr, slice};
use std::sync::atomic::{AtomicUsize, Ordering};

pub struct BBRing {
    buf: *mut u8,
    cap: usize,
    marker: AtomicUsize, // head(32 bits) + len(16 bits) + finished_len(16 bits)
}

impl BBRing {
    
    pub fn new(cap: usize) -> Self {
        let buf = unsafe { alloc(Layout::from_size_align_unchecked(cap, 1)) };
        if buf.is_null() {
            panic!("There was an error allocating the ring buf");
        }
        Self {
            buf,
            cap,
            marker: AtomicUsize::new(Marker::default().0),
        }
    }

    pub fn push(&self, data: &[u8]) {
        let marker = Marker::from_raw(self.marker.fetch_add(Marker::new((usize::BITS / 8) as usize + data.len(), 1, 0).into_raw(), Ordering::AcqRel));
        // FIXME: add OOB checks!
        // write length header
        unsafe { self.buf.add(marker.head()).cast::<usize>().write(data.len()) };
        // write payload
        unsafe { ptr::copy(data as *const u8, self.buf.add(marker.head() + usize::BITS / 8), data.len()) };
        // FIXME: increment finished_len if it is the same value as len in marker
    }

    pub fn pop(&self) -> BufGuard<'_> {

    }
    
}

pub struct BufGuard<'a> {
    ptr: *mut u8,
    len: usize,
    _phantom_data: PhantomData<&'a ()>,
}

impl AsRef<[u8]> for BufGuard<'_> {
    fn as_ref(&self) -> &[u8] {
        unsafe { slice::from_raw_parts(self.ptr.cast_const(), self.len) }
    }
}

impl AsMut<[u8]> for BufGuard<'_> {
    fn as_mut(&mut self) -> &mut [u8] {
        unsafe { slice::from_raw_parts_mut(self.ptr, self.len) }
    }
}

impl Drop for BufGuard<'_> {
    fn drop(&mut self) {
        // FIXME: adjust marker in parent accordingly!
    }
}

#[derive(Default)]
struct Marker(usize);

impl Marker {

    #[inline]
    fn new(head: usize, len: usize, finished_len: usize) -> Self {
        Self(head | (len << (usize::BITS / 2)) | (finished_len << (usize::BITS / 2 + usize::BITS / 2 / 2)))
    }

    #[inline]
    fn from_raw(raw: usize) -> Self {
        Self(raw)
    }

    #[inline]
    fn head(&self) -> usize {
        self.0 & (u32::MAX as usize)
    }

    #[inline]
    fn len(&self) -> usize {
        let offset = usize::BITS / 2;
        (self.0 & ((u16::MAX as usize) << offset)) >> offset
    }

    #[inline]
    fn finished_len(&self) -> usize {
        let offset = usize::BITS / 2 + usize::BITS / 2 / 2;
        (self.0 & ((u16::MAX as usize) << offset)) >> offset
    }

    #[inline]
    fn into_raw(self) -> usize {
        self.0
    }

}
