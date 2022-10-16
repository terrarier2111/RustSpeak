use std::mem::{ManuallyDrop, MaybeUninit, transmute};
use std::ops::Deref;
use std::ptr;
use std::ptr::{addr_of_mut, null_mut};
use std::sync::Arc;
use std::sync::atomic::{AtomicPtr, AtomicU8, fence, Ordering};
use parking_lot::Mutex;

// this is better for cases where we DON'T care much about removal of nodes during traversal:
// https://www.codeproject.com/Articles/723555/A-Lock-Free-Doubly-Linked-List
// this is better if we DO:
// https://scholar.google.com/citations?view_op=view_citation&hl=de&user=RJmBj1wAAAAJ&citation_for_view=RJmBj1wAAAAJ:UebtZRa9Y70C

pub struct AtomicBoxedDoublyLinkedList<T> {
    header_node: AtomicPtr<Arc<AtomicBoxedDoublyLinkedListNode<T>>>,
}

impl<T> AtomicBoxedDoublyLinkedList<T> {

    pub fn push_head(&self, val: T) -> Arc<AtomicBoxedDoublyLinkedListNode<T>> {
        /*let mut node = Arc::new(AtomicBoxedDoublyLinkedListNode {
            val,
            left: Default::default(),
            right: Default::default(),
        });
        let node_ptr = addr_of_mut!(node);
        let _ = self.header_node.fetch_update(Ordering::Release, Ordering::Acquire, move |curr_head| {
            if let Some(curr_head) = unsafe { curr_head.as_ref() } {
                curr_head.left.store(node_ptr, Ordering::Release);
            }
            node.right.store(curr_head, Ordering::Release); // FIXME: is it okay to do this inside the fetch_update?
            Some(node_ptr)
        }).unwrap();
        let ret = node.clone();
        let _ = ManuallyDrop::new(node); // leak node to increase strong reference count
        ret*/
        todo!()
    }

    pub fn remove_head(&self) -> Option<Arc<AtomicBoxedDoublyLinkedListNode<T>>> {
        /*let old_header = self.header_node.fetch_update(Ordering::Release, Ordering::Acquire, move |curr_header| {
            Some(curr_header.right.load(Ordering::Acquire))
        }).unwrap();
        unsafe { old_header.as_ref() }.map(|x| x.clone())*/
        todo!()
    }

}

impl<T> Drop for AtomicBoxedDoublyLinkedList<T> {
    fn drop(&mut self) {
        // FIXME: destroy entire list!
    }
}

pub struct AtomicBoxedDoublyLinkedListNode<T> {
    val: T,
    left: Link<T>,
    right: Link<T>,
}

impl<T> AtomicBoxedDoublyLinkedListNode<T> {

    #[inline]
    pub fn get(&self) -> Option<&T> {
        if self.right.get().get_marker() {
            return None;
        }
        Some(&self.val)
    }

    /*
    // FIXME: should we maybe consume Arc<Self> here?
    pub fn next(&self) -> Option<Arc<AtomicBoxedDoublyLinkedListNode<T>>> {
        unsafe { self.right.load(Ordering::Acquire).as_ref() }.map(|x| x.clone())
    }

    // FIXME: should we maybe consume Arc<Self> here?
    pub fn prev(&self) -> Option<Arc<AtomicBoxedDoublyLinkedListNode<T>>> {
        unsafe { self.left.load(Ordering::Acquire).as_ref() }.map(|x| x.clone())
    }*/

    pub fn remove(mut self: Arc<Self>) -> Option<> {
        /*let mut state = NodeState(self.state.load(Ordering::AcqRel));

        let right = self.right.load(Ordering::Acquire);
        let left = self.left.load(Ordering::Acquire);
        if let Some(left) = unsafe { left.as_ref() } {
            left.right.store(right, Ordering::Release);
        }
        if let Some(right) = unsafe { right.as_ref() } {
            right.left.store(left, Ordering::Release);
        }*/

        /*
        loop {
            let left = self.left.load(Ordering::Relaxed);
            if left.left.load(Ordering::Relaxed).
        }
        */
        let prev;
        let mut next;
        loop {
            next = self.right.get();
            if next.get_marker() {
                return None;
            }
            if self.right.try_set_addr_full(next.ptr, next.get_ptr(), true) {
                loop {
                    prev = self.left.get();
                    if prev.get_marker() || self.left.try_set_addr_full(prev.ptr, prev.get_ptr(), true) {
                        break;
                    }
                }
                prev = pr
            }
        }



        unsafe { addr_of_mut!(self).drop_in_place() } // drop the arc
        todo!()
    }

    /// tries to update the
    /// prev pointer of a node and then return a reference to a possibly
    /// logically previous node
    fn correct_prev(mut self: Arc<Self>, node: *mut Arc<AtomicBoxedDoublyLinkedListNode<T>>) -> Option<Link<T>> {
        let this = addr_of_mut!(self);
        let mut prev = this;
        let mut last_link: Option<*mut Arc<AtomicBoxedDoublyLinkedListNode<T>>> = None;
        loop {
            let link_1 = unsafe { node.as_ref().unwrap() }.left.get();
            if link_1.get_marker() {
                break;
            }
            let mut prev_2 = unsafe { prev.as_ref() }.unwrap().right.get();
            if prev_2.get_marker() {
                if let Some(last_link) = last_link.take() {
                    unsafe { prev.as_ref() }.unwrap().left.set_mark();
                    unsafe { last_link.get_ref().get().get_ptr().as_ref().unwrap() }.right.try_set_addr_full(prev, prev_2.get_ptr(), false);
                    prev = last_link;
                    continue;
                }
                prev_2 = unsafe { prev.get().get_ptr().as_ref() }.unwrap().left.get();
                continue;
            }
            if prev_2.ptr != node {
                last_link = Some(prev);
                prev = RefSource::Instance(Link {
                    ptr: AtomicPtr::new(prev_2.ptr),
                });
                continue;
            }

            if unsafe { node.as_ref().unwrap() }.left.try_set_addr_full(link_1.ptr, prev.get().get_ptr(), false) {
                if unsafe { prev.get().get_ptr().as_ref() }.unwrap().left.get().get_marker() {
                    continue;
                }
                break;
            }

        }
        match prev {
            RefSource::Ref(_) => None,
            RefSource::Instance(inst) => Some(inst),
        }
    }

}

struct Link<T> {
    ptr: AtomicPtr<Arc<AtomicBoxedDoublyLinkedListNode<T>>>,
}

impl<T> Link<T> {
    const MARKER: usize = 1 << 63;
    const CAS_ORDERING: Ordering = Ordering::SeqCst;

    /*fn get(&self) -> (*mut Arc<AtomicBoxedDoublyLinkedListNode<T>>, bool) {
        let raw_ptr = self.ptr.load(Ordering::Relaxed);
        (ptr::from_exposed_addr_mut(raw_ptr.expose_addr() & (!Self::MARKER)), (raw_ptr.expose_addr() & Self::MARKER) != 0)
    }

    fn get_raw(&self) -> *mut Arc<AtomicBoxedDoublyLinkedListNode<T>> {
        self.ptr.load(Ordering::Relaxed)
    }*/
    
    fn get(&self) -> LinkContent<T> {
        LinkContent {
            ptr: self.ptr.load(Ordering::Relaxed),
        }
    }

    fn set_mark(&self) {
        loop {
            let node = self.get();
            let node_ptr = node.get_ptr();
            if node.get_marker() || self.ptr.compare_exchange(node_ptr, ptr::from_exposed_addr_mut(node_ptr.expose_addr() | Self::MARKER), Self::CAS_ORDERING, strongest_failure_ordering(Self::CAS_ORDERING)).is_ok(){
                break;
            }
        }
    }

    fn set_addr(&self, new: *mut Arc<AtomicBoxedDoublyLinkedListNode<T>>) {
        loop {
            let node = self.get();
            // FIXME: do we need to be able to retain the old MARKER?
            if node.get_marker() || self.try_set_addr_full(node.get_ptr(), new, false) {
                break;
            }
        }
    }

    fn try_set_addr_full(&self, old: *mut Arc<AtomicBoxedDoublyLinkedListNode<T>>, new: *mut Arc<AtomicBoxedDoublyLinkedListNode<T>>, set_marker: bool) -> bool {
        self.ptr.compare_exchange(old, if set_marker { ptr::from_exposed_addr_mut(new.expose_addr() | Self::MARKER) } else { new }, Self::CAS_ORDERING, strongest_failure_ordering(Self::CAS_ORDERING)).is_ok()
    }

    unsafe fn set_unsafe(&self, new: *mut Arc<AtomicBoxedDoublyLinkedListNode<T>>, marker: bool) {
        let marker = if marker {
            Self::MARKER
        } else {
            0
        };
        self.ptr.store(ptr::from_exposed_addr_mut(new.expose_addr() | marker), Self::CAS_ORDERING);
    }

    #[inline]
    const fn invalid() -> Self {
        Self {
            ptr: AtomicPtr::new(null_mut()),
        }
    }

    pub fn add_after(&self, val: T) -> Arc<AtomicBoxedDoublyLinkedListNode<T>> {
        let mut node = Arc::new(AtomicBoxedDoublyLinkedListNode {
            val,
            left: Link::invalid(),
            right: Link::invalid(),
        });

        self.inner_add_after(node.clone());

        // let _prev = self.correct_prev(next);
        /*
        if let Some(right) = unsafe { right.as_ref() } {
            right.left.store(node_ptr, Ordering::Release);
        } else {
            self.right.store(node_ptr, Ordering::Release);
        }
        let ret = node.clone();
        let _ = ManuallyDrop::new(node); // leak a single reference
        ret*/
        let ret = node.clone();
        let _ = ManuallyDrop::new(node); // leak a single reference
        ret
    }

    fn inner_add_after(&self, mut node: Arc<AtomicBoxedDoublyLinkedListNode<T>>) {
        let this = self.get().get_ptr();
        let slf = unsafe { this.as_ref() }.unwrap();
        let node_ptr = addr_of_mut!(node);
        loop {
            let next = unsafe { this.as_ref() }.unwrap().right.get();
            unsafe { node.left.set_unsafe(this, false); }
            unsafe { node.right.set_unsafe(next.get_ptr(), false); }
            if slf.right.try_set_addr_full(next.get_ptr(), node_ptr, false) {
                break;
            }
            if slf.right.get().get_marker() {
                return self.inner_add_before(node);
            }
        }
        let _ = self.correct_prev(node_ptr);
    }

    pub fn add_before(&self, val: T) -> Arc<AtomicBoxedDoublyLinkedListNode<T>> {
        let mut node = Arc::new(AtomicBoxedDoublyLinkedListNode {
            val,
            left: Link::invalid(),
            right: Link::invalid(),
        });

        self.inner_add_before(node.clone());

        let ret = node.clone();
        let _ = ManuallyDrop::new(node); // leak a single reference
        ret
    }

    fn inner_add_before(&self, mut node: Arc<AtomicBoxedDoublyLinkedListNode<T>>) {
        let this = self.get().get_ptr();
        let slf = unsafe { this.as_ref() }.unwrap();
        let node_ptr = addr_of_mut!(node);
        let mut prev = RefSource::Ref(&slf.left);
        let mut cursor = this;
        let mut next = cursor;
        loop {
            while self.right.get().get_marker() {
                // cursor.next();
                if let Some(updated) = prev.correct_prev(cursor) {
                    prev = RefSource::Instance(updated);
                }
            }
            next = cursor;
            let prev_val = prev.get_ref().get();
            unsafe { node.left.set_unsafe(prev_val.get_ptr(), false); }
            unsafe { node.right.set_unsafe(cursor, false); }
            if unsafe { prev_val.get_ptr().as_ref().unwrap() }.right.try_set_addr_full(cursor, node_ptr, false) {
                break;
            }
            if let Some(updated) = prev.correct_prev(cursor) {
                prev = RefSource::Instance(updated);
            }
        }
        let _ = prev.correct_prev(next);
    }

}

#[inline]
#[cfg(target_has_atomic = "8")]
fn strongest_failure_ordering(order: Ordering) -> Ordering {
    match order {
        Ordering::Release => Ordering::Relaxed,
        Ordering::Relaxed => Ordering::Relaxed,
        Ordering::SeqCst => Ordering::SeqCst,
        Ordering::Acquire => Ordering::Acquire,
        Ordering::AcqRel => Ordering::Acquire,
        _ => unreachable!(),
    }
}

#[derive(Copy, Clone)]
struct LinkContent<T> {
    ptr: *mut Arc<AtomicBoxedDoublyLinkedListNode<T>>,
}

impl<T> LinkContent<T> {
    const MARKER: usize = 1 << 63;

    fn get_marker(&self) -> bool {
        (self.ptr.expose_addr() & Self::MARKER) != 0
    }

    fn get_ptr(&self) -> *mut Arc<AtomicBoxedDoublyLinkedListNode<T>> {
        ptr::from_exposed_addr_mut(self.ptr.expose_addr() & (!Self::MARKER))
    }
}

impl<T> PartialEq for LinkContent<T> {
    fn eq(&self, other: &Self) -> bool {
        self.ptr.expose_addr() == other.ptr.expose_addr()
    }
}

enum RefSource<'a, T> {
    Ref(&'a T),
    Instance(T),
}

impl<'a, T> RefSource<'a, T> {

    fn get_ref(&self) -> &T {
        match self {
            RefSource::Ref(rf) => rf,
            RefSource::Instance(inst) => inst,
        }
    }

}

impl<'a, T> Deref for RefSource<'a, T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        match self {
            RefSource::Ref(rf) => rf,
            RefSource::Instance(inst) => inst,
        }
    }
}

impl<'a, T> AsRef<T> for RefSource<'a, T> {
    fn as_ref(&self) -> &T {
        match self {
            RefSource::Ref(rf) => rf,
            RefSource::Instance(inst) => inst,
        }
    }
}
