use std::mem::{ManuallyDrop, MaybeUninit, transmute};
use std::ops::Deref;
use std::ptr;
use std::ptr::{addr_of, addr_of_mut, null_mut};
use std::sync::Arc;
use std::sync::atomic::{AtomicPtr, AtomicU8, fence, Ordering};
use aligned::{A4, Aligned};
use parking_lot::Mutex;

// this is better for cases where we DON'T care much about removal of nodes during traversal:
// https://www.codeproject.com/Articles/723555/A-Lock-Free-Doubly-Linked-List
// this is better if we DO:
// https://scholar.google.com/citations?view_op=view_citation&hl=de&user=RJmBj1wAAAAJ&citation_for_view=RJmBj1wAAAAJ:UebtZRa9Y70C

pub struct AtomicDoublyLinkedList<T, const NODE_KIND: NodeKind = { NodeKind::Bound }> {
    header_node: AtomicPtr<Node<T, NODE_KIND>>,
    // in the header node itself, the left field points to the `header_node` field of the list itself, so we don't have to maintain a reference count
    tail_node: AtomicPtr<Node<T, NODE_KIND>>,
    // in the tail node itself, the right field points to the `tail_node` field of the list itself, so we don't have to maintain a reference count
    // TODO: also add tail_node functions (where possible) (just to complement the header_node)
}

impl<T, const NODE_KIND: NodeKind> AtomicDoublyLinkedList<T, NODE_KIND> {

    const ENDS_ORDERING: Ordering = Ordering::SeqCst;

    #[inline]
    fn header_addr(&self) -> *const AtomicPtr<Node<T, NODE_KIND>> { // FIXME: is this safe - because we are later casting between const and mut pointers?
        addr_of!(self.header_node)
    }

    #[inline]
    fn tail_addr(&self) -> *const AtomicPtr<Node<T, NODE_KIND>> { // FIXME: is this safe - because we are later casting between const and mut pointers?
        addr_of!(self.tail_node)
    }

    pub fn push_head(&self, val: T) -> Node<T, NODE_KIND> {
        let mut node = Aligned(Arc::new(AtomicDoublyLinkedListNode {
            val,
            left: Link { ptr: AtomicPtr::new(ptr::from_exposed_addr_mut(self.header_addr().expose_addr() | HEAD_OR_TAIL_MARKER)) },
            right: Link { ptr: AtomicPtr::new(ptr::from_exposed_addr_mut(self.tail_addr().expose_addr() | HEAD_OR_TAIL_MARKER)) },
        }));
        'main: loop {
            let head = self.header_node.load(Self::ENDS_ORDERING);
            if let Some(head) = unsafe { head.as_ref() } {
                head.clone().inner_add_before(node.clone());
            } else {
                let node_addr = addr_of_mut!(node);
                if self.header_node.compare_exchange(null_mut(), node_addr, Ordering::SeqCst, strongest_failure_ordering(Ordering::SeqCst)).is_ok() { // TODO: try loosening this ordering!
                    // we can just assume that the following check will succeed because of the properties
                    // the update order leads to (that the right node will always be updated first)
                    // which enables us to deduce that if the header_node could be updated successfully that the tail_node
                    // can automatically be updated as well
                    if self.tail_node.compare_exchange(null_mut(), node_addr, Ordering::SeqCst, strongest_failure_ordering(Ordering::SeqCst)).is_ok() { // TODO: try loosening this ordering!
                        break;
                    }
                }
            }
        }
        if NODE_KIND == NodeKind::Bound {
            // leak one reference to node to make it stay in the list even when the reference that gets returned is dropped
            let ret = node.clone();
            let _ = ManuallyDrop::new(ret); // leak a single reference
        }
        node
    }

    pub fn remove_head(&self) -> Option<Node<T, NODE_KIND, true>> {
        loop {
            if let Some(head) = unsafe { self.header_node.load(Self::ENDS_ORDERING).as_ref() } {
                if let Some(val) = head.clone().remove() {
                    return Some(Aligned(val));
                }
            } else {
                return None;
            }
        }
    }

    /*
    pub fn push_tail(&self, val: T) -> Arc<AtomicDoublyLinkedListNode<T>> {
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
        let new_node = AtomicDoublyLinkedListNode {
            val,
            left: Link::invalid(),
            right: Link::invalid(),
        };
        loop {
            let head = self.header_node.load(Self::ENDS_ORDERING);
            if let Some(head) = unsafe { head.as_ref() } {
                head.add_before(val);
            } else {
                let node = Arc::new(AtomicDoublyLinkedListNode {
                    val,
                    left: Link { ptr: AtomicPtr::new(self.header_addr().cast()) },
                    right: Link { ptr: AtomicPtr::new(self.tail_addr().cast()) },
                });

            }
        }
    }*/

    pub fn is_empty(&self) -> bool {
        self.header_node.load(Self::ENDS_ORDERING).is_null()
    }

    // FIXME: add len getter!
    /*pub fn len(&self) -> usize {
        let mut prev = self.header_node.load(Self::ENDS_ORDERING);
        let mut len = 0;
        loop {
            if prev.is_null() {
                return len;
            }
            len += 1;
            loop {
                // FIXME: how can we be sure that the node we are visiting isn't getting removed currently (this can probably be achieved by checking the REMOVE_MARKER)
                // FIXME: but how can we ensure that we are behaving correctly - and what does correct behavior in this context even mean?
            }
        }
    }*/

    // FIXME: add iter method!

}

impl<T, const NODE_KIND: NodeKind> Drop for AtomicDoublyLinkedList<T, NODE_KIND> {
    fn drop(&mut self) {
        // remove all nodes in the list, when the list gets dropped,
        // this makes sure that all the nodes' state is consistent
        // and correct
        while !self.is_empty() {
            self.remove_head();
        }
    }
}

#[derive(Copy, Clone, Default, Debug, Eq, PartialEq)]
pub enum NodeKind {
    /// nodes of this kind will remain inside the list, even if the
    /// reference returned by the add function gets dropped
    #[default]
    Bound,
    /// nodes of this kind will immediately be removed from the list
    /// once the reference returned by the add function gets dropped
    Unbound,
}

// we have to ensure, that the first 2 bits of this tye's pointer aren't used,
// in order to accomplish this we are always using this type in combination with Aligned<4, Self>
pub struct AtomicDoublyLinkedListNode<T, const NODE_KIND: NodeKind = { NodeKind::Bound }, const DELETED: bool = false> { // TODO: change the bool here into an enum!
    val: T,
    left: Link<T, NODE_KIND>,
    right: Link<T, NODE_KIND>,
}

impl<T, const NODE_KIND: NodeKind> AtomicDoublyLinkedListNode<T, NODE_KIND, false> {

    pub fn remove(mut self: Aligned<A4, Arc<Self>>) -> Option<Arc<AtomicDoublyLinkedListNode<T, NODE_KIND, true>>> {
        let this = addr_of_mut!(self);
        let mut prev;
        let mut next;
        loop {
            next = self.right.get();
            if next.get_deletion_marker() {
                // FIXME: do we need to drop the arc here as well?
                return None;
            }
            let next_no_del_mark = if next.get_head_or_tail_marker() {
                ptr::from_exposed_addr_mut(next.get_ptr().expose_addr() | HEAD_OR_TAIL_MARKER)
            } else {
                next.get_ptr()
            };
            if self.right.try_set_addr_full::<true>(next.ptr, next_no_del_mark) { // TODO: we could probably simply insert the raw next ptr instead of doing weird things with it.
                loop {
                    prev = self.left.get();
                    if prev.get_deletion_marker() || self.left.try_set_addr_full::<true>(prev.ptr, prev.get_ptr()) {
                        break;
                    }
                }
                prev = LinkContent {
                    ptr: unsafe { prev.get_ptr().as_ref() }.unwrap().clone().correct_prev(next.get_ptr()),
                };
                if prev.get_head_or_tail_marker() {
                    let head = unsafe { prev.get_ptr().cast::<AtomicPtr<Node<T, NODE_KIND>>>().as_ref() }.unwrap();
                    let new_head = if next.get_head_or_tail_marker() {
                        null_mut()
                    } else {
                        next.get_ptr()
                    };
                    head.compare_exchange(this, new_head, Ordering::SeqCst, strongest_failure_ordering(Ordering::SeqCst)); // TODO: try loosening this ordering!
                    // FIXME: do we have to handle the failure of the above operation specially somehow?
                }
                if next.get_head_or_tail_marker() {
                    let tail = unsafe { next.get_ptr().cast::<AtomicPtr<Node<T, NODE_KIND>>>().as_ref() }.unwrap();
                    let new_tail = if prev.get_head_or_tail_marker() {
                        null_mut()
                    } else {
                        prev.get_ptr()
                    };
                    tail.compare_exchange(this, new_tail, Ordering::SeqCst, strongest_failure_ordering(Ordering::SeqCst)); // TODO: try loosening this ordering!
                    // FIXME: do we have to handle the failure of the above operation specially somehow?
                }
                println!("left del: {}", self.left.get().get_deletion_marker());
                println!("right del: {}", self.left.get().get_deletion_marker());
                unsafe { self.left.invalidate(); }
                // unsafe { self.right.invalidate(); } // FIXME: do we have to invalidate the right? if not we can maybe use it to traverse even through deleted nodes when iterating
                if NODE_KIND == NodeKind::Bound {
                    // SAFETY: This is safe because we know that we leaked a reference to the arc earlier,
                    // so we can just reduce the reference count again such that the `virtual reference`
                    // we held is gone.
                    unsafe { addr_of_mut!(self).drop_in_place() } // drop the arc
                }
                // SAFETY: This is safe because the only thing we change about the type
                // is slightly tightening its usage restrictions and the in-memory
                // representation of these two types is exactly the same as the
                // only thing that changes is a const attribute
                return Some(unsafe { transmute(self) });
            }
        }
    }

    pub fn add_after(mut self: Aligned<A4, Arc<Self>>, val: T) -> Node<T, NODE_KIND> {
        /*if self.right.get().get_head_or_tail_marker() {
            return self.add_before(val);
        }*/
        let mut node = Aligned(Arc::new(AtomicDoublyLinkedListNode {
            val,
            left: Link::invalid(),
            right: Link::invalid(),
        }));

        self.inner_add_after(node.clone());

        if NODE_KIND == NodeKind::Bound {
            let ret = node.clone();
            let _ = ManuallyDrop::new(ret); // leak a single reference
        }
        node
    }

    fn inner_add_after(mut self: Aligned<A4, Arc<Self>>, mut node: Node<T, NODE_KIND>) {
        let this = addr_of_mut!(self);
        let node_ptr = addr_of_mut!(node);
        loop {
            let next = self.right.get();
            unsafe { node.left.set_unsafe::<false>(this); }
            let node_right = if next.get_head_or_tail_marker() {
                // if `self` refers to the tail node, set the new tail and the tail marker as the new node's
                // next(right) node
                ptr::from_exposed_addr_mut(next.get_ptr().expose_addr() | HEAD_OR_TAIL_MARKER)
            } else {
                next.get_ptr()
            };
            unsafe { node.right.set_unsafe::<false>(node_right); }
            if next.get_head_or_tail_marker() {
                let tail = unsafe { next.get_ptr().cast::<AtomicPtr<Node<T, NODE_KIND>>>().as_ref() }.unwrap();
                if tail.compare_exchange(this, node_ptr, Ordering::SeqCst, strongest_failure_ordering(Ordering::SeqCst)).is_ok() { // TODO: can we relax this ordering a bit?
                    break;
                }
            } else if self.right.try_set_addr_full::<false>(next.get_ptr(), node_ptr) {
                break;
            }
            if self.right.get().get_deletion_marker() {
                return self.inner_add_before(node);
            }
        }
        let _ = self.correct_prev(node_ptr);
    }

    pub fn add_before(mut self: Aligned<A4, Arc<Self>>, val: T) -> Node<T, NODE_KIND> {
        let mut node = Aligned(Arc::new(AtomicDoublyLinkedListNode {
            val,
            left: Link::invalid(),
            right: Link::invalid(),
        }));

        self.inner_add_before(node.clone());

        if NODE_KIND == NodeKind::Bound {
            let ret = node.clone();
            let _ = ManuallyDrop::new(ret); // leak a single reference
        }
        node
    }

    fn inner_add_before(mut self: Aligned<A4, Arc<Self>>, mut node: Node<T, NODE_KIND>) {
        let this = addr_of_mut!(self);
        let node_ptr = addr_of_mut!(node);
        loop {
            let left = self.left.get();
            if left.get_head_or_tail_marker() {
                // we detected that we are the header node, so we try to set the header's right node to this one
                // set the head and the head marker as the new node's prev(left) node
                let node_left = ptr::from_exposed_addr_mut(left.get_ptr().expose_addr() | HEAD_OR_TAIL_MARKER);
                unsafe { node.left.set_unsafe::<false>(node_left); }
                unsafe { node.right.set_unsafe::<false>(this); }
                let header = unsafe { left.get_ptr().cast::<AtomicPtr<Node<T, NODE_KIND>>>().as_ref() }.unwrap();
                if header.compare_exchange(this, node_ptr, Ordering::SeqCst, strongest_failure_ordering(Ordering::SeqCst)).is_ok() { // TODO: can we relax this ordering a bit?
                    // let _ = prev.get_ref().correct_prev(this);
                    // FIXME: we have to do some sort of correction for this node's left link
                    return;
                }
                // return self.inner_add_after(node);
            }
            let mut prev = self.left.get().get_ptr();
            let cursor = this;
            let /*mut */next = cursor;
            loop {
                while self.right.get().get_deletion_marker() {
                    unsafe { cursor.as_ref() }.unwrap().clone().next();
                    prev = unsafe { prev.as_ref() }.unwrap().clone().correct_prev(cursor);
                }
                // next = cursor;
                unsafe { node.left.set_unsafe::<false>(prev); }
                unsafe { node.right.set_unsafe::<false>(cursor); }
                if unsafe { prev.as_ref().unwrap() }.right.try_set_addr_full::<false>(cursor, node_ptr) {
                    break;
                }
                prev = unsafe { prev.as_ref() }.unwrap().clone().correct_prev(cursor);
            }
            let _ = unsafe { prev.as_ref() }.unwrap().clone().correct_prev(next);
        }
    }

    /// Tries to set the current node's right link
    /// to its following node.
    /// In pseudocode: self.right = self.right.right;
    /// Note, that there are additional deletion checks
    /// being performed before setting the next node.
    /// This method returns true as long as the tail node isn't reached.
    fn next(mut self: Aligned<A4, Arc<Self>>) -> bool {
        let mut cursor = addr_of_mut!(self);
        loop {
            let next = unsafe { cursor.as_ref() }.unwrap().right.get();
            if next.get_head_or_tail_marker() {
                // check if the cursor is the tail, if so - return false
                return false;
            }
            let marker = unsafe { next.get_ptr().as_ref() }.unwrap().right.get().get_deletion_marker();
            if marker && unsafe { cursor.as_ref() }.unwrap().right.get().ptr == next.get_ptr() {
                unsafe { next.get_ptr().as_ref() }.unwrap().left.set_deletion_mark();
                unsafe { cursor.as_ref() }.unwrap().right.try_set_addr_full::<false>(next.ptr, unsafe { next.get_ptr().as_ref() }.unwrap().right.get().get_ptr());
                continue;
            }
            cursor = next.get_ptr();
            if !marker && !unsafe { next.get_ptr().as_ref() }.unwrap().right.get().get_head_or_tail_marker() {
                return true;
            }
        }
    }

    /// tries to update the prev pointer of a node and then return a reference to a possibly
    /// logically previous node
    fn correct_prev(mut self: Aligned<A4, Arc<Self>>, node: NodePtr<T, NODE_KIND>) -> NodePtr<T, NODE_KIND> {
        let mut prev = addr_of_mut!(self);
        let mut last_link: Option<NodePtr<T, NODE_KIND>> = None;
        loop {
            let link_1 = unsafe { node.as_ref().unwrap() }.left.get();
            if link_1.get_deletion_marker() {
                break;
            }
            let mut prev_2 = unsafe { prev.as_ref() }.unwrap().right.get();
            if prev_2.get_deletion_marker() {
                if let Some(last_link) = last_link.take() {
                    unsafe { prev.as_ref() }.unwrap().left.set_deletion_mark();
                    unsafe { last_link.as_ref().unwrap() }.right.try_set_addr_full::<false>(prev, prev_2.get_ptr());
                    prev = last_link;
                    continue;
                }
                prev_2 = unsafe { prev.as_ref() }.unwrap().left.get();
                prev = prev_2.get_ptr();
                continue;
            }
            if prev_2.ptr != node {
                last_link = Some(prev);
                prev = prev_2.get_ptr();
                continue;
            }

            if unsafe { node.as_ref().unwrap() }.left.try_set_addr_full::<false>(link_1.ptr, prev) {
                if unsafe { prev.as_ref() }.unwrap().left.get().get_deletion_marker() {
                    continue;
                }
                break;
            }

        }
        prev
    }

}

impl<T, const NODE_KIND: NodeKind, const DELETED: bool> AtomicDoublyLinkedListNode<T, NODE_KIND, DELETED> {

    /// checks whether the node is detached from the list or not
    pub fn is_detached(&self) -> bool {
        // FIXME: can we assume that the node is detached if DELETED is true? - we probably can, but in all cases we have to check for left's validity to ensure that we can return false
        self.left.is_invalid()
    }

    #[inline]
    pub fn get(&self) -> Option<&T> {
        if self.right.get().get_deletion_marker() {
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

}

const DELETION_MARKER: usize = 1 << 1/*63*/;
const HEAD_OR_TAIL_MARKER: usize = 1 << 0/*62*/;

struct Link<T, const NODE_KIND: NodeKind = { NodeKind::Bound }> {
    ptr: AtomicPtr<Node<T, NODE_KIND>>,
}

impl<T, const NODE_KIND: NodeKind> Link<T, NODE_KIND> {
    const CAS_ORDERING: Ordering = Ordering::SeqCst;
    
    fn get(&self) -> LinkContent<T, NODE_KIND> {
        LinkContent {
            ptr: self.ptr.load(Ordering::Relaxed),
        }
    }

    fn set_deletion_mark(&self) {
        loop {
            let node = self.get();
            if node.get_deletion_marker() || self.ptr.compare_exchange(node.get_ptr(), ptr::from_exposed_addr_mut(node.ptr.expose_addr() | DELETION_MARKER), Self::CAS_ORDERING, strongest_failure_ordering(Self::CAS_ORDERING)).is_ok() {
                break;
            }
        }
    }

    fn try_set_addr_full<const SET_DELETION_MARKER: bool>(&self, old: NodePtr<T, NODE_KIND>, new: NodePtr<T, NODE_KIND>) -> bool {
        self.ptr.compare_exchange(old, if SET_DELETION_MARKER { ptr::from_exposed_addr_mut(new.expose_addr() | DELETION_MARKER) } else { new }, Self::CAS_ORDERING, strongest_failure_ordering(Self::CAS_ORDERING)).is_ok()
    }

    unsafe fn set_unsafe<const SET_DELETION_MARKER: bool>(&self, new: NodePtr<T, NODE_KIND>) {
        let marker = if SET_DELETION_MARKER {
            DELETION_MARKER
        } else {
            0
        };
        self.ptr.store(ptr::from_exposed_addr_mut(new.expose_addr() | marker), Self::CAS_ORDERING);
    }

    unsafe fn invalidate(&self) {
        self.ptr.store(null_mut(), Self::CAS_ORDERING);
    }

    fn is_invalid(&self) -> bool {
        self.ptr.load(Self::CAS_ORDERING).is_null()
    }

    #[inline]
    const fn invalid() -> Self {
        Self {
            ptr: AtomicPtr::new(null_mut()),
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
        _ => unreachable!(),
    }
}

#[derive(Copy, Clone)]
struct LinkContent<T, const NODE_KIND: NodeKind = { NodeKind::Bound }> {
    ptr: NodePtr<T, NODE_KIND>,
}

impl<T, const NODE_KIND: NodeKind> LinkContent<T, NODE_KIND> {
    fn get_deletion_marker(&self) -> bool {
        (self.ptr.expose_addr() & DELETION_MARKER) != 0
    }

    fn get_head_or_tail_marker(&self) -> bool {
        (self.ptr.expose_addr() & HEAD_OR_TAIL_MARKER) != 0
    }

    fn get_ptr(&self) -> NodePtr<T, NODE_KIND> {
        ptr::from_exposed_addr_mut(self.ptr.expose_addr() & (!(DELETION_MARKER | HEAD_OR_TAIL_MARKER)))
    }
}

impl<T, const NODE_KIND: NodeKind> PartialEq for LinkContent<T, NODE_KIND> {
    fn eq(&self, other: &Self) -> bool {
        self.ptr.expose_addr() == other.ptr.expose_addr()
    }
}

type NodePtr<T, const NODE_KIND: NodeKind, const REMOVED: bool = false> = *mut Node<T, NODE_KIND, REMOVED>;
type Node<T, const NODE_KIND: NodeKind, const REMOVED: bool = false> = Aligned<A4, Arc<AtomicDoublyLinkedListNode<T, NODE_KIND, REMOVED>>>;
