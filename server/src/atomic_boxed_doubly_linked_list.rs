use std::mem::{ManuallyDrop, transmute};
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
        let mut node = Arc::new(AtomicBoxedDoublyLinkedListNode {
            val,
            left: Default::default(),
            right: Default::default(),
            state: Default::default()
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
        ret
    }

    pub fn remove_head(&self) -> Option<Arc<AtomicBoxedDoublyLinkedListNode<T>>> {
        let old_header = self.header_node.fetch_update(Ordering::Release, Ordering::Acquire, move |curr_header| {
            Some(curr_header.right.load(Ordering::Acquire))
        }).unwrap();
        unsafe { old_header.as_ref() }.map(|x| x.clone())
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
        if self.right.get().1 {
            return None;
        }
        Some(&self.val)
    }

    // FIXME: should we maybe consume Arc<Self> here?
    pub fn next(&self) -> Option<Arc<AtomicBoxedDoublyLinkedListNode<T>>> {
        unsafe { self.right.load(Ordering::Acquire).as_ref() }.map(|x| x.clone())
    }

    // FIXME: should we maybe consume Arc<Self> here?
    pub fn prev(&self) -> Option<Arc<AtomicBoxedDoublyLinkedListNode<T>>> {
        unsafe { self.left.load(Ordering::Acquire).as_ref() }.map(|x| x.clone())
    }

    pub fn remove(mut self: Arc<Self>) {
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



        unsafe { addr_of_mut!(self).drop_in_place() } // drop the arc
    }

    pub fn add_after(mut self: Arc<Self>, val: T) -> Arc<AtomicBoxedDoublyLinkedListNode<T>> {
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

    fn inner_add_after(mut self: Arc<Self>, mut node: Arc<AtomicBoxedDoublyLinkedListNode<T>>) {
        let this = addr_of_mut!(self);
        let node_ptr = addr_of_mut!(node);
        loop {
            let next = this.right.get();
            unsafe { node.left.set_unsafe(this, false); }
            unsafe { node.right.set_unsafe(next.0, false); }
            if self.right.try_set_addr_full(next.0, node_ptr) {
                break;
            }
            if self.right.get().1 {
                return self.insert_before(val);
            }
        }
        // let _prev = self.correct_prev(next);
    }

    pub fn add_before(mut self: Arc<Self>, val: T) -> Arc<AtomicBoxedDoublyLinkedListNode<T>> {
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

    fn inner_add_before(mut self: Arc<Self>, mut node: Arc<AtomicBoxedDoublyLinkedListNode<T>>) {
        let this = addr_of_mut!(self);
        let node_ptr = addr_of_mut!(node);
        let mut prev = self.left.get().0;
        let mut cursor = this;
        loop {
            while self.right.get().1 {
                // cursor.next();
                // prev = prev.correct_prev(cursor);
            }
            unsafe { self.left.set_unsafe(prev, false); }
            unsafe { self.right.set_unsafe(cursor, false); }
            if unsafe { prev.as_ref().unwrap() }.right.try_set_addr_full(cursor, node_ptr) {
                break;
            }
            // prev = prev.correct_prev(cursor);
        }
        // let _ = prev.correct_prev(next);
    }

}

#[derive(Copy, Clone)]
#[repr(transparent)]
struct NodeState(u8);

impl NodeState {

    const DESTROYING: u8 = 1 << 0;
    const ADDING: u8 = 1 << 1;

    fn from_raw(raw: u8) -> Self {
        NodeState(raw)
    }

    fn to_raw(self) -> u8 {
        self.0
    }

    fn is_destroying(&self) -> self {
        (self.0 & Self::DESTROYING) != 0
    }

    fn is_adding(&self) -> self {
        (self.0 & Self::ADDING) != 0
    }

    fn set_bit(&mut self, flag: u8, val: bool) {
        if val {
            self.0 &= u8::MAX & (!flag);
        } else {
            self.0 |= flag;
        }
    }

    fn set_destroying(&mut self, val: bool) {
        self.set_bit(Self::DESTROYING, val);
    }

    fn set_adding(&mut self, val: bool) {
        self.set_bit(Self::ADDING, val);
    }

}

struct Link<T> {
    ptr: AtomicPtr<Arc<AtomicBoxedDoublyLinkedListNode<T>>>,
}

impl<T> Link<T> {
    const MARKER: usize = 1 << 63;
    const CAS_ORDERING: Ordering = Ordering::SeqCst;

    fn get(&self) -> (*mut Arc<AtomicBoxedDoublyLinkedListNode<T>>, bool) {
        let raw_ptr = self.ptr.load(Ordering::Relaxed);
        (ptr::from_exposed_addr_mut(raw_ptr.expose_addr() & (!Self::MARKER)), (raw_ptr.expose_addr() & Self::MARKER) != 0)
    }

    fn get_raw(&self) -> *mut Arc<AtomicBoxedDoublyLinkedListNode<T>> {
        self.ptr.load(Ordering::Relaxed)
    }

    fn set_mark(&self) {
        loop {
            let (node, marker) = self.get();
            if marker || self.ptr.compare_exchange(node, ptr::from_exposed_addr_mut(node.expose_addr() | Self::MARKER), Self::CAS_ORDERING, strongest_failure_ordering(Self::CAS_ORDERING)).is_ok(){
                break;
            }
        }
    }

    fn set_addr(&self, new: *mut Arc<AtomicBoxedDoublyLinkedListNode<T>>) {
        loop {
            let (node, marker) = self.get();
            // FIXME: do we need to be able to retain the old MARKER?
            if marker || self.try_set_addr_full(node, new) {
                break;
            }
        }
    }

    fn try_set_addr_full(&self, old: *mut Arc<AtomicBoxedDoublyLinkedListNode<T>>, new: *mut Arc<AtomicBoxedDoublyLinkedListNode<T>>) -> bool {
        self.ptr.compare_exchange(old, new, Self::CAS_ORDERING, strongest_failure_ordering(Self::CAS_ORDERING)).is_ok()
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
    const fn invalid<T>() -> Self {
        Self {
            ptr: Default::default(),
        }
    }

    fn correct_prev(mut prev: &Link<T>, node: *mut Arc<AtomicBoxedDoublyLinkedListNode<T>>) -> *mut Arc<AtomicBoxedDoublyLinkedListNode<T>> {
        let mut last_link: Option<Arc<AtomicBoxedDoublyLinkedListNode<T>>> = None;
        loop {
            let link_1 = unsafe { node.as_ref().unwrap() }.left.get();
            if link_1.1 {
                break;
            }
            let mut prev_2 = prev.get().0.right.get();
            if prev_2.1 {
                if let Some(last_link) = last_link.take() {
                    prev.get().0.left.set_mark();
                    last_link.right.try_set_addr_full(prev.get_raw(), prev_2.0); // FIXME: does `this` have to contain the MARKER as well?
                    // prev = last_link;
                    continue;
                }
                prev_2 = prev.get().0.left.get();
                continue;
            }
            /*if prev_2 != node {
                last_link = Some(prev);
                prev = prev_2;
                continue;
            }

            if node.left.try_set_addr_full(node.left.get_raw(), link_1, prev) {

            }*/

        }
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
    }
}
