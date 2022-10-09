use std::mem::{ManuallyDrop, transmute};
use std::ptr::{addr_of_mut, null_mut};
use std::sync::Arc;
use std::sync::atomic::{AtomicPtr, AtomicU8, fence, Ordering};
use parking_lot::Mutex;

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
    left: AtomicPtr<Arc<AtomicBoxedDoublyLinkedListNode<T>>>,
    right: AtomicPtr<Arc<AtomicBoxedDoublyLinkedListNode<T>>>,
    state: AtomicU8,
}

impl<T> AtomicBoxedDoublyLinkedListNode<T> {

    #[inline]
    pub fn get(&self) -> &T {
        &self.val
    }

    // FIXME: should we maybe consume Arc<Self> here?
    /// Returns the value that's currently the currently valid next value
    /// it is not ensured that no write operation is being currently performed on this node
    pub fn next(&self) -> Option<Arc<AtomicBoxedDoublyLinkedListNode<T>>> {
        unsafe { self.right.load(Ordering::Acquire).as_ref() }.map(|x| x.clone())
    }

    // FIXME: should we maybe consume Arc<Self> here?
    /// Returns the value that's currently the currently valid next value
    /// it is not ensured that no write operation is being currently performed on this node
    pub fn prev(&self) -> Option<Arc<AtomicBoxedDoublyLinkedListNode<T>>> {
        unsafe { self.left.load(Ordering::Acquire).as_ref() }.map(|x| x.clone())
    }

    pub fn remove(mut self: Arc<Self>) {
        let mut state = NodeState(self.state.load(Ordering::AcqRel));

        let right = self.right.load(Ordering::Acquire);
        let left = self.left.load(Ordering::Acquire);
        if let Some(left) = unsafe { left.as_ref() } {
            left.right.store(right, Ordering::Release);
        }
        if let Some(right) = unsafe { right.as_ref() } {
            right.left.store(left, Ordering::Release);
        }

        unsafe { addr_of_mut!(self).drop_in_place() } // drop the arc
    }

    pub fn add(mut self: Arc<Self>, val: T) -> Arc<AtomicBoxedDoublyLinkedListNode<T>> {
        let this = addr_of_mut!(self);
        let right = self.right.load(Ordering::Acquire);
        let mut node = Arc::new(AtomicBoxedDoublyLinkedListNode {
            val,
            left: AtomicPtr::new(this),
            right: AtomicPtr::new(right),
            state: Default::default(),
        });
        let node_ptr = addr_of_mut!(node);
        if let Some(right) = unsafe { right.as_ref() } {
            right.left.store(node_ptr, Ordering::Release);
        } else {
            self.right.store(node_ptr, Ordering::Release);
        }
        let ret = node.clone();
        let _ = ManuallyDrop::new(node); // leak a single reference
        ret
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
