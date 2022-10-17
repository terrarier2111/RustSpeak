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

// FIXME: we have to ensure, that the first 2 bits aren't used - a possible solution would be to use the `aligned` crate: https://docs.rs/aligned/latest/aligned/
pub struct AtomicDoublyLinkedList<T/*, const NODE_KIND: NodeKind = { NodeKind::Bound }*/> {
    header_node: AtomicPtr<Arc<AtomicDoublyLinkedListNode<T>>>,
    // TODO: in the header node itself, the left field should point to the list itself, so we don't have to maintain a reference count

    // FIXME: also add a tail_node (just to complement the header_node)
}

impl<T> AtomicDoublyLinkedList<T> {

    pub fn push_head(&self, val: T) -> Arc<AtomicDoublyLinkedListNode<T>> {
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

    pub fn remove_head(&self) -> Option<Arc<AtomicDoublyLinkedListNode<T>>> {
        /*let old_header = self.header_node.fetch_update(Ordering::Release, Ordering::Acquire, move |curr_header| {
            Some(curr_header.right.load(Ordering::Acquire))
        }).unwrap();
        unsafe { old_header.as_ref() }.map(|x| x.clone())*/
        todo!()
    }

    pub fn is_empty(&self) -> bool {
        self.header_node.load(Ordering::SeqCst).is_null()
    }

    // FIXME: add len getter!

    // FIXME: add iter method!

}

impl<T> Drop for AtomicDoublyLinkedList<T> {
    fn drop(&mut self) {
        // FIXME: destroy entire list!
    }
}

// FIXME: add NodeKind as a const static parameter to the list itself (and not to the individual nodes)
#[derive(Copy, Clone, Default, Debug)]
enum NodeKind {
    /// nodes of this kind will remain inside the list, even if the
    /// reference returned by the add function gets dropped
    #[default]
    Bound,
    /// nodes of this kind will immediately be removed from the list
    /// once the reference returned by the add function gets dropped
    Unbound,
}

pub struct AtomicDoublyLinkedListNode<T> {
    val: T,
    left: Link<T>,
    right: Link<T>,
}

impl<T> AtomicDoublyLinkedListNode<T> {

    /// checks whether the node is detached from the list or not
    pub fn is_detached(&self) -> bool {
        self.right.is_invalid()
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

    pub fn remove(mut self: Arc<Self>) -> Option<Arc<Self>> {
        let this = addr_of_mut!(self);
        let prev;
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
            if self.right.try_set_addr_full::<true>(next.ptr, next_no_del_mark) {
                loop {
                    prev = self.left.get();
                    if prev.get_deletion_marker() || self.left.try_set_addr_full::<true>(prev.ptr, prev.get_ptr()) {
                        break;
                    }
                }
                prev = unsafe { prev.get_ptr().as_ref() }.unwrap().correct_prev(next.get_ptr()).get();
                if let Some(tail) = unsafe { next.get_ptr().cast::<AtomicPtr<Arc<AtomicDoublyLinkedListNode<T>>>>().as_ref() } {
                    tail.compare_exchange(this, );
                }
                unsafe { self.left.invalidate(); }
                unsafe { self.right.invalidate(); }
                unsafe { addr_of_mut!(self).drop_in_place() } // drop the arc
                return Some(self);
            }
        }
    }

    pub fn add_after(mut self: Arc<Self>, val: T) -> Arc<AtomicDoublyLinkedListNode<T>> {
        let mut node = Arc::new(AtomicDoublyLinkedListNode {
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

    fn inner_add_after(mut self: Arc<Self>, mut node: Arc<AtomicDoublyLinkedListNode<T>>) {
        let this = addr_of_mut!(self);
        let node_ptr = addr_of_mut!(node);
        loop {
            let next = unsafe { this.as_ref() }.unwrap().right.get();
            unsafe { node.left.set_unsafe::<false>(this); }
            let node_right = if next.get_head_or_tail_marker() {
                // if `self` refers to the tail node, set the new tail and the tail marker as the new node's
                // next(right) node
                ptr::from_exposed_addr_mut(next.get_ptr() | HEAD_OR_TAIL_MARKER)
            } else {
                next.get_ptr()
            };
            unsafe { node.right.set_unsafe::<false>(node_right); }
            if self.right.try_set_addr_full::<false>(next.get_ptr(), node_ptr) {
                if next.get_head_or_tail_marker() {
                    let tail = unsafe { next.get_ptr().cast::<AtomicPtr<Arc<AtomicDoublyLinkedListNode<T>>>>().as_ref() }.unwrap();
                    tail.store(node_ptr, Ordering::SeqCst); // FIXME: can we relax this ordering a bit?
                }
                break;
            }
            if self.right.get().get_deletion_marker() {
                return self.inner_add_before(node);
            }
        }
        let _ = self.correct_prev(node_ptr);
    }

    pub fn add_before(mut self: Arc<Self>, val: T) -> Arc<AtomicDoublyLinkedListNode<T>> {
        let mut node = Arc::new(AtomicDoublyLinkedListNode {
            val,
            left: Link::invalid(),
            right: Link::invalid(),
        });

        self.inner_add_before(node.clone());

        let ret = node.clone();
        let _ = ManuallyDrop::new(node); // leak a single reference
        ret
    }

    fn inner_add_before(mut self: Arc<Self>, mut node: Arc<AtomicDoublyLinkedListNode<T>>) {
        let this = addr_of_mut!(self);
        let node_ptr = addr_of_mut!(node);
        let mut prev = RefSource::Ref(&self.left);
        let mut cursor = this;
        let mut next = cursor;
        loop {
            while self.right.get().get_deletion_marker() {
                cursor.next();
                prev = RefSource::Instance(prev.correct_prev(cursor));
            }
            next = cursor;
            let prev_val = prev.get_ref().get();
            let node_left = if next.get_head_or_tail_marker() {
                // if `self` refers to the head node, set the head and the head marker as the new node's
                // prev(left) node
                ptr::from_exposed_addr_mut(prev_val.get_ptr() | HEAD_OR_TAIL_MARKER)
            } else {
                next.get_ptr()
            };
            unsafe { node.left.set_unsafe::<false>(node_left); }
            unsafe { node.right.set_unsafe::<false>(cursor); }
            if unsafe { prev_val.get_ptr().as_ref().unwrap() }.right.try_set_addr_full::<false>(cursor, node_ptr) {
                if prev_val.get_head_or_tail_marker() {
                    let header = unsafe { prev_val.get_ptr().cast::<AtomicPtr<Arc<AtomicDoublyLinkedListNode<T>>>>().as_ref() }.unwrap();
                    header.store(node_ptr, Ordering::SeqCst); // FIXME: can we relax this ordering a bit?
                    // FIXME: can we use compare_exchange here?
                }
                break;
            }
            if let Some(updated) = prev.correct_prev(cursor) {
                prev = RefSource::Instance(updated);
            }
        }
        let _ = prev.correct_prev(next);
    }

    fn next(mut self: Arc<Self>) -> bool {
        let mut cursor = addr_of_mut!(self);
        loop {
            let next = unsafe { cursor.as_ref() }.unwrap().right.get();
            let marker = next.get_ptr().right.get().get_deletion_marker();
            if marker && unsafe { cursor.as_ref() }.unwrap().right.get().ptr == next.get_ptr() {
                next.get_ptr().left.set_deletion_mark();
                unsafe { cursor.as_ref() }.unwrap().right.try_set_addr_full::<false>(next.ptr, next.get_ptr().right.get().get_ptr());
                continue;
            }
            cursor = next.get_ptr();
            if !marker && !next.get_head_or_tail_marker() {
                return true;
            }
        }
    }

    /// tries to update the prev pointer of a node and then return a reference to a possibly
    /// logically previous node
    fn correct_prev(mut self: Arc<Self>, node: *mut Arc<AtomicDoublyLinkedListNode<T>>) -> Link<T> {
        let this = addr_of_mut!(self);
        // SAFETY:
        // it is safe to artificially construct a Link here because the first time
        // the marker of prev becomes relevant, it is impossible for the original prev
        // to still be present because it has to have been replaced by last_link at that
        // point in time
        let mut prev = Link {
            ptr: AtomicPtr::new(this),
        };
        let node = LinkContent {
            ptr: node,
        };
        let mut last_link: Option<Link<T>> = None;
        loop {
            let link_1 = unsafe { node.get_ptr().as_ref().unwrap() }.left.get();
            if link_1.get_deletion_marker() {
                break;
            }
            let mut prev_2 = unsafe { prev.get().get_ptr().as_ref() }.unwrap().right.get();
            if prev_2.get_marker() {
                if let Some(last_link) = last_link.take() {
                    unsafe { prev.get().get_ptr().as_ref() }.unwrap().left.set_deletion_mark();
                    unsafe { last_link.get().get_ptr().as_ref().unwrap() }.right.try_set_addr_full::<false>(prev.get().ptr, prev_2.get_ptr());
                    prev = last_link;
                    continue;
                }
                prev_2 = unsafe { prev.get().get_ptr().as_ref() }.unwrap().left.get();
                continue;
            }
            if prev_2 != node {
                last_link = Some(prev);
                // FIXME: is it safe to construct a link here?
                prev = Link {
                    ptr: AtomicPtr::new(prev_2.ptr),
                };
                continue;
            }

            if unsafe { node.get_ptr().as_ref().unwrap() }.left.try_set_addr_full::<false>(link_1.ptr, prev.get().get_ptr()) {
                if unsafe { prev.get().get_ptr().as_ref() }.unwrap().left.get().get_deletion_marker() {
                    continue;
                }
                break;
            }

        }
        prev
    }

}

const DELETION_MARKER: usize = 1 << 63;
const HEAD_OR_TAIL_MARKER: usize = 1 << 62; // FIXME: implement and use this!

struct Link<T> {
    ptr: AtomicPtr<Arc<AtomicDoublyLinkedListNode<T>>>,
}

impl<T> Link<T> {
    const CAS_ORDERING: Ordering = Ordering::SeqCst;
    
    fn get(&self) -> LinkContent<T> {
        LinkContent {
            ptr: self.ptr.load(Ordering::Relaxed),
        }
    }

    fn set_deletion_mark(&self) {
        loop {
            let node = self.get();
            if node.get_deletion_marker() || self.ptr.compare_exchange(node_ptr, ptr::from_exposed_addr_mut(node.ptr.expose_addr() | DELETION_MARKER), Self::CAS_ORDERING, strongest_failure_ordering(Self::CAS_ORDERING)).is_ok(){
                break;
            }
        }
    }

    /*
    fn set_addr(&self, new: *mut Arc<AtomicDoublyLinkedListNode<T>>) {
        loop {
            let node = self.get();
            // FIXME: do we need to be able to retain the old MARKER?
            if node.get_deletion_marker() || self.try_set_addr_full::<false>(node.get_ptr(), new) {
                break;
            }
        }
    }*/

    fn try_set_addr_full<const SET_DELETION_MARKER: bool>(&self, old: *mut Arc<AtomicDoublyLinkedListNode<T>>, new: *mut Arc<AtomicDoublyLinkedListNode<T>>) -> bool {
        self.ptr.compare_exchange(old, if SET_DELETION_MARKER { ptr::from_exposed_addr_mut(new.expose_addr() | DELETION_MARKER) } else { new }, Self::CAS_ORDERING, strongest_failure_ordering(Self::CAS_ORDERING)).is_ok()
    }

    unsafe fn set_unsafe<const SET_DELETION_MARKER: bool>(&self, new: *mut Arc<AtomicDoublyLinkedListNode<T>>) {
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
struct LinkContent<T> {
    ptr: *mut Arc<AtomicDoublyLinkedListNode<T>>,
}

impl<T> LinkContent<T> {
    fn get_deletion_marker(&self) -> bool {
        (self.ptr.expose_addr() & DELETION_MARKER) != 0
    }

    fn get_head_or_tail_marker(&self) -> bool {
        (self.ptr.expose_addr() & HEAD_OR_TAIL_MARKER) != 0
    }

    fn get_ptr(&self) -> *mut Arc<AtomicDoublyLinkedListNode<T>> {
        ptr::from_exposed_addr_mut(self.ptr.expose_addr() & (!(DELETION_MARKER | HEAD_OR_TAIL_MARKER)))
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
