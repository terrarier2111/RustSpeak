use std::alloc::{alloc, dealloc, Layout, LayoutError};
use std::arch::asm;
use std::mem::{align_of, ManuallyDrop, MaybeUninit, needs_drop, size_of, transmute};
use std::ops::Deref;
use std::{mem, ptr};
use std::ptr::{addr_of, addr_of_mut, NonNull, null_mut};
use std::sync::Arc;
use std::sync::atomic::{AtomicPtr, AtomicU8, fence, Ordering};
use aligned::{A4, Aligned};

// disable MIRI SB(stacked borrows) checks

// this is better for cases where we DON'T care much about removal of nodes during traversal:
// https://www.codeproject.com/Articles/723555/A-Lock-Free-Doubly-Linked-List
// this is better if we DO:
// https://scholar.google.com/citations?view_op=view_citation&hl=de&user=RJmBj1wAAAAJ&citation_for_view=RJmBj1wAAAAJ:UebtZRa9Y70C

// FIXME: NOTE THAT: &mut T can be converted into *mut T by using .into() on the mutable reference!

// FIXME: add guard to nodes in Unbound mode to remove them once all of the references to them get dropped
pub struct AtomicDoublyLinkedList<T, const NODE_KIND: NodeKind = { NodeKind::Bound }> {
    header_node: RawNode<T, NODE_KIND>,
    // in the header node itself, the left field points to the `header_node` field of the list itself, so we don't have to maintain a reference count
    tail_node: RawNode<T, NODE_KIND>,
    // in the tail node itself, the right field points to the `tail_node` field of the list itself, so we don't have to maintain a reference count
    // TODO: also add tail_node functions (where possible) (just to complement the header_node)
}

impl<T, const NODE_KIND: NodeKind> AtomicDoublyLinkedList<T, NODE_KIND> {

    const ENDS_ORDERING: Ordering = Ordering::SeqCst;

    pub fn new() -> Arc<Self> {
        if size_of::<Node<T, NODE_KIND, false>>() != size_of::<Node<T, NODE_KIND, true>>() {
            // check if the sizes for false and true differ
            unreachable!("Node size can't be evaluated statically!");
        }
        if align_of::<Node<T, NODE_KIND, false>>() != align_of::<Node<T, NODE_KIND, true>>() {
            // check if the alignments for false and true differ
            unreachable!("Node alignment can't be evaluated statically!");
        }
        if size_of::<RawNode<T, NODE_KIND, false>>() != size_of::<RawNode<T, NODE_KIND, true>>() {
            // check if the sizes for false and true differ
            unreachable!("Node size can't be evaluated statically!");
        }
        if align_of::<RawNode<T, NODE_KIND, false>>() != align_of::<RawNode<T, NODE_KIND, true>>() {
            // check if the alignments for false and true differ
            unreachable!("Node alignment can't be evaluated statically!");
        }
        if size_of::<Arc<Aligned<A4, AtomicDoublyLinkedListNode<T, NODE_KIND, false>>>>() != size_of::<Arc<Aligned<A4, AtomicDoublyLinkedListNode<T, NODE_KIND, true>>>>() {
            unreachable!("Arc's size can't be evaluated statically!");
        }
        if align_of::<Arc<Aligned<A4, AtomicDoublyLinkedListNode<T, NODE_KIND, false>>>>() != align_of::<Arc<Aligned<A4, AtomicDoublyLinkedListNode<T, NODE_KIND, true>>>>() {
            unreachable!("Arc's alignment can't be evaluated statically!");
        }
        /*
        if align_of::<Arc<Aligned<A4, AtomicDoublyLinkedListNode<T, NODE_KIND>>>>() < 4 {
            unreachable!("Arc's alignment isn't sufficient!");
        }*/
        let ret = Arc::new(Self {
            header_node: Aligned(AtomicDoublyLinkedListNode {
                val: MaybeUninit::uninit(),
                left: Link::invalid(),
                right: Link::invalid(),
            }),
            tail_node: Aligned(AtomicDoublyLinkedListNode {
                val: MaybeUninit::uninit(),
                left: Link::invalid(),
                right: Link::invalid(),
            }),
        });

        // SAFETY: we know that there are no other threads setting modifying
        // these nodes and thus they will automatically be correct
        unsafe { ret.header_node.right.set_unsafe::<false>(ret.tail_addr()); }
        unsafe { ret.tail_node.left.set_unsafe::<false>(ret.header_addr()); }

        ret
    }

    /*
    pub fn new<const NODES: NodeKind>() -> AtomicDoublyLinkedList<T, NODES> {
        AtomicDoublyLinkedList {
            header_node: Default::default(),
            tail_node: Default::default(),
        }
    }*/

    #[inline]
    fn header_addr(&self) -> NodePtr<T, NODE_KIND> {
        addr_of!(self.header_node)
    }

    #[inline]
    fn tail_addr(&self) -> NodePtr<T, NODE_KIND> {
        addr_of!(self.tail_node)
    }

    pub fn push_head(&self, val: T) -> Node<T, NODE_KIND> {
        self.header_node.add_after(val)
    }

    /*
    pub fn push_tail(&self, val: T) -> Arc<AtomicDoublyLinkedListNode<T>> {
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
        self.header_node.right.get().get_ptr() == self.tail_addr()
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

impl<T, const NODE_KIND: NodeKind> AtomicDoublyLinkedList<T, NODE_KIND> {

    pub fn remove_head(&self) -> Option<Node<T, { NodeKind::Bound }, true>> {
        if NODE_KIND == NodeKind::Bound {
            loop {
                if self.is_empty() {
                    return None;
                }
                let head = self.header_node.right.get().get_ptr().cast::<RawNode<T, { NodeKind::Bound }>>();
                let head = ManuallyDrop::new(unsafe { Arc::from_raw(head) });
                if let Some(val) = head.remove() {
                    println!("removed head!");
                    println!("rem end head: {:?}", self.header_node.right.get().ptr);
                    println!("rem end tail: {:?}", self.tail_node.left.get().ptr);
                    return Some(val);
                }
            }
        } else {
            loop {
                if self.is_empty() {
                    return None;
                }
                let head = self.header_node.right.get().get_ptr().cast::<RawNode<T, { NodeKind::Unbound }>>();
                // increase the ref count because we want to return a reference to the node and thus we have to create a reference out of thin air
                mem::forget(unsafe { Arc::from_raw(head) });
                let head = unsafe { Arc::from_raw(head) };
                if let Some(val) = head.remove() {
                    println!("removed head!");
                    println!("rem end head: {:?}", self.header_node.right.get().ptr);
                    println!("rem end tail: {:?}", self.tail_node.left.get().ptr);
                    return Some(val);
                }
            }
        }
    }

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
#[derive(Debug)]
pub struct AtomicDoublyLinkedListNode<T, const NODE_KIND: NodeKind = { NodeKind::Bound }, const DELETED: bool = false> { // TODO: change the bool here into an enum!
val: MaybeUninit<T>, // FIXME: drop this when the node gets dropped and right and left are both non-null
left: Link<T, NODE_KIND>,
    right: Link<T, NODE_KIND>,
}

impl<T> AtomicDoublyLinkedListNode<T, { NodeKind::Bound }, false> {

    pub fn remove(self: &Arc<Aligned<A4, Self>>) -> Option<Node<T, { NodeKind::Bound }, true>> {
        // let this = Arc::as_ptr(self) as *const RawNode<T, { NodeKind::Bound }>;
        if self.right.get().ptr.is_null() || self.left.get().ptr.is_null() {
            // we can't remove the header or tail nodes
            return None;
        }
        let mut prev;
        loop {
            let next = self.right.get();
            if next.get_deletion_marker() {
                // FIXME: do we need to drop the arc here as well? - we probably don't because the deletion marker on next (probably) means that this(`self`) node is already being deleted
                return None;
            }
            println!("pre set right!");
            if self.right.try_set_addr_full::<true>(next.ptr, next.ptr) {
                println!("did set right!");
                loop {
                    prev = self.left.get();
                    if prev.get_deletion_marker() || self.left.try_set_addr_full::<true>(prev.ptr, prev.ptr) {
                        break;
                    }
                }
                println!("pre head stuff!");
                prev = LinkContent {
                    ptr: unsafe { prev.get_ptr().as_ref() }.unwrap().clone().correct_prev(next.get_ptr()), // FIXME: we get an unwrapped none panic in this line (in Bound mode) - that's probably because here we have a header node which we try to deref!
                };
                println!("post head stuff!");
                println!("left del: {}", self.left.get().get_deletion_marker());
                println!("right del: {}", self.left.get().get_deletion_marker());

                // SAFETY: This is safe because we know that we leaked a reference to the arc earlier,
                // so we can just reduce the reference count again such that the `virtual reference`
                // we held is gone.
                let ret = unsafe { Arc::from_raw(Arc::as_ptr(self)) };

                // SAFETY: This is safe because the only thing we change about the type
                // is slightly tightening its usage restrictions and the in-memory
                // representation of these two types is exactly the same as the
                // only thing that changes is a const attribute
                return Some(unsafe { transmute(ret) });
            }
        }
    }

}

impl<T> AtomicDoublyLinkedListNode<T, { NodeKind::Unbound }, false> {

    pub fn remove(self: Arc<Aligned<A4, Self>>) -> Option<Node<T, { NodeKind::Bound }, true>> {
        // let this = Arc::as_ptr(self) as *const RawNode<T, { NodeKind::Unbound }>;
        if self.right.get().ptr.is_null() || self.left.get().ptr.is_null() {
            // we can't remove the header or tail nodes
            return None;
        }
        let mut prev;
        loop {
            let next = self.right.get();
            if next.get_deletion_marker() {
                // FIXME: do we need to drop the arc here as well? - we probably don't because the deletion marker on next (probably) means that this(`self`) node is already being deleted
                return None;
            }
            println!("pre set right!");
            if self.right.try_set_addr_full::<true>(next.ptr, next.ptr) {
                println!("did set right!");
                loop {
                    prev = self.left.get();
                    if prev.get_deletion_marker() || self.left.try_set_addr_full::<true>(prev.ptr, prev.ptr) {
                        break;
                    }
                }
                println!("pre head stuff!");
                prev = LinkContent {
                    ptr: unsafe { prev.get_ptr().as_ref() }.unwrap().clone().correct_prev(next.get_ptr()), // FIXME: we get an unwrapped none panic in this line (in Bound mode) - that's probably because here we have a header node which we try to deref!
                };
                println!("post head stuff!");
                println!("left del: {}", self.left.get().get_deletion_marker());
                println!("right del: {}", self.left.get().get_deletion_marker());

                // SAFETY: This is safe because the only thing we change about the type
                // is slightly tightening its usage restrictions and the in-memory
                // representation of these two types is exactly the same as the
                // only thing that changes is a const attribute
                return Some(unsafe { transmute(self) });
            }
        }
    }

}

impl<T, const NODE_KIND: NodeKind> AtomicDoublyLinkedListNode<T, NODE_KIND, false> {

    pub fn add_after(self: &Aligned<A4, Self>, val: T) -> Node<T, NODE_KIND> {
        let node = Arc::new(Aligned(AtomicDoublyLinkedListNode {
            val: MaybeUninit::new(val),
            left: Link::invalid(),
            right: Link::invalid(),
        }));

        self.inner_add_after(&node);

        if NODE_KIND == NodeKind::Bound {
            let ret = node.clone();
            let _ = ManuallyDrop::new(ret); // leak a single reference
        }
        node
    }

    fn inner_add_after(self: &Aligned<A4, Self>, node: &RawNode<T, NODE_KIND>) {
        if self.right.get().ptr.is_null() {
            // if we are the tail, add before ourselves, not after
            return self.inner_add_before(node);
        }
        let this = self as *const RawNode<T, NODE_KIND>;
        let node_ptr = node as *const RawNode<T, NODE_KIND>;
        loop {
            let next = self.right.get();
            unsafe { node.left.set_unsafe::<false>(this); }
            unsafe { node.right.set_unsafe::<false>(next.get_ptr()); }
            if self.right.try_set_addr_full::<false>(next.get_ptr(), node_ptr) {
                break;
            }
            if self.right.get().get_deletion_marker() {
                return self.inner_add_before(node);
            }
        }
        println!("added after!");
        let _ = self.correct_prev(node_ptr);
    }

    pub fn add_before(self: &Aligned<A4, Self>, val: T) -> Node<T, NODE_KIND> {
        let node = Arc::new(Aligned(AtomicDoublyLinkedListNode {
            val: MaybeUninit::new(val),
            left: Link::invalid(),
            right: Link::invalid(),
        }));

        self.inner_add_before(&node);

        if NODE_KIND == NodeKind::Bound {
            let ret = node.clone();
            let _ = ManuallyDrop::new(ret); // leak a single reference
        }
        node
    }

    fn inner_add_before(self: &Aligned<A4, Self>, node: &RawNode<T, NODE_KIND>) {
        if self.left.get().ptr.is_null() {
            // if we are the header, add after ourselves, not before
            return self.inner_add_before(node);
        }
        let this = self as *const RawNode<T, NODE_KIND>;
        let node_ptr = node as *const RawNode<T, NODE_KIND>;
        let mut i = 0;
        loop {
            i += 1;
            println!("in add before loop: {}", i);
            let left = self.left.get();
            let mut prev = left.get_ptr();
            let cursor = this;
            let /*mut */next = cursor;
            loop {
                while self.right.get().get_deletion_marker() {
                    unsafe { cursor.as_ref() }.unwrap().clone().next();
                    prev = unsafe { prev.as_ref() }.unwrap().clone().correct_prev(cursor.cast_mut());
                }
                // next = cursor;
                unsafe { node.left.set_unsafe::<false>(prev); }
                unsafe { node.right.set_unsafe::<false>(cursor.cast_mut()); }
                if unsafe { prev.as_ref().unwrap() }.right.try_set_addr_full::<false>(cursor.cast_mut(), node_ptr.cast_mut()) {
                    break;
                }
                prev = unsafe { prev.as_ref() }.unwrap().clone().correct_prev(cursor.cast_mut());
            }
            let _ = unsafe { prev.as_ref() }.unwrap().clone().correct_prev(next.cast_mut());
            break;
        }
    }

    /// Tries to set the current node's right link
    /// to its following node.
    /// In pseudocode: self.right = self.right.right;
    /// Note, that there are additional deletion checks
    /// being performed before setting the next node.
    /// This method returns true as long as the tail node isn't reached.
    fn next(self: &Aligned<A4, Self>) -> bool { // prepped for header/tail update
        let mut cursor = self as *const RawNode<T, NODE_KIND>;
        loop {
            let next = unsafe { cursor.as_ref() }.unwrap().right.get();
            if next.ptr.is_null() {
                // check if the cursor is the tail, if so - return false
                return false;
            }
            let marker = unsafe { next.get_ptr().as_ref() }.unwrap().right.get().get_deletion_marker();
            if marker && unsafe { cursor.as_ref() }.unwrap().right.get().ptr == next.get_ptr() {
                unsafe { next.get_ptr().as_ref() }.unwrap().left.set_deletion_mark();
                let new_right = unsafe { next.get_ptr().as_ref() }.unwrap().right.get();
                unsafe { cursor.as_ref() }.unwrap().right.try_set_addr_full::<false>(next.ptr, new_right.get_ptr());
                continue;
            }
            cursor = next.get_ptr();
            if !marker && !unsafe { next.get_ptr().as_ref() }.unwrap().right.get().ptr.is_null() {
                return true;
            }
        }
    }

    // header -> node -> this
    // header <- node | header <- this
    // node.correct_prev(this)


    // header -> node | this -> node
    // header <- this <- node
    /// tries to update the prev pointer of a node and then return a reference to a possibly
    /// logically previous node
    fn correct_prev(self: &Aligned<A4, Self>, node: NodePtr<T, NODE_KIND>) -> NodePtr<T, NODE_KIND> {
        let mut prev = self as *const RawNode<T, NODE_KIND>; // = node
        let mut last_link: Option<NodePtr<T, NODE_KIND>> = None;
        let mut i = 0;
        loop {
            i += 1;
            println!("in correct prev loop {}", i);
            let link_1 = unsafe { node.as_ref().unwrap() }.left.get();
            if link_1.get_deletion_marker() {
                break;
            }
            println!("prev: {:?}", prev);
            let mut prev_2 = unsafe { prev.as_ref() }.unwrap().right.get(); // = this | FIXME: this sometimes panics on unwrap because of None values (but only in the second loop iter)
            if prev_2.get_deletion_marker() {
                if let Some(last_link) = last_link.take() {
                    unsafe { prev.as_ref() }.unwrap().left.set_deletion_mark();
                    unsafe { last_link.as_ref().unwrap() }.right.try_set_addr_full::<false>(prev, prev_2.get_ptr());
                    prev = last_link;
                    continue;
                }
                prev_2 = unsafe { prev.as_ref() }.unwrap().left.get();
                prev = prev_2.get_ptr();
                println!("continuing after del marker check {:?}", prev_2.ptr);
                continue;
            }
            if prev_2.ptr != node {
                println!("prev_2: {:?}", prev_2.ptr);
                println!("node: {:?}", node);
                last_link = Some(prev);
                prev = prev_2.get_ptr();
                println!("continuing after prev_2 = node check");
                continue;
            }

            if unsafe { node.as_ref().unwrap() }.left.try_set_addr_full::<false>(link_1.ptr, prev) {
                if unsafe { prev.as_ref() }.unwrap().left.get().get_deletion_marker() {
                    println!("continuing in the end!");
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
    /*pub fn is_detached(&self) -> bool {
        // we can assume that the node is detached if DELETED is true,
        // but in all cases other than `DELETED = true` we have to check for left's validity to ensure that we can return false
        DELETED || self.left.is_invalid()
    }*/

    #[inline]
    pub fn get(&self) -> Option<&T> {
        let right = self.right.get();
        if right.get_deletion_marker() || right.ptr.is_null() || self.left.get().ptr.is_null() {
            return None;
        }
        // SAFETY: we have just checked that we are not the header or tail nodes and thus our
        // value has to be init!
        Some(unsafe { self.val.assume_init_ref() })
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

#[derive(Debug)]
struct Link<T, const NODE_KIND: NodeKind = { NodeKind::Bound }> {
    ptr: Aligned<A4, AtomicPtr<RawNode<T, NODE_KIND>>>,
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
            if node.get_deletion_marker() || self.ptr.compare_exchange(node.get_ptr().cast_mut(), ptr::from_exposed_addr_mut(node.ptr.expose_addr() | DELETION_MARKER), Self::CAS_ORDERING, strongest_failure_ordering(Self::CAS_ORDERING)).is_ok() {
                break;
            }
        }
    }

    fn try_set_addr_full<const SET_DELETION_MARKER: bool>(&self, old: NodePtr<T, NODE_KIND>, new: NodePtr<T, NODE_KIND>) -> bool {
        self.ptr.compare_exchange(old.cast_mut(), if SET_DELETION_MARKER { ptr::from_exposed_addr_mut(new.expose_addr() | DELETION_MARKER) } else { new.cast_mut() }, Self::CAS_ORDERING, strongest_failure_ordering(Self::CAS_ORDERING)).is_ok()
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
            ptr: Aligned(AtomicPtr::new(null_mut())),
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

#[derive(Copy, Clone, Debug)]
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

type NodePtr<T, const NODE_KIND: NodeKind, const REMOVED: bool = false> = *const RawNode<T, NODE_KIND, REMOVED>;
type RawNode<T, const NODE_KIND: NodeKind, const REMOVED: bool = false> = Aligned<A4, AtomicDoublyLinkedListNode<T, NODE_KIND, REMOVED>>;
pub type Node<T, const NODE_KIND: NodeKind, const REMOVED: bool = false> = Arc<Aligned<A4, AtomicDoublyLinkedListNode<T, NODE_KIND, REMOVED>>>;

struct SizedBox<T> { // FIXME: NOTE: this thing could basically be replaced by Box::leak
alloc_ptr: NonNull<T>,
}

impl<T> SizedBox<T> {

    const LAYOUT: Layout = Layout::from_size_align(size_of::<T>(), align_of::<T>()).ok().unwrap(); // FIXME: can we somehow retain the error message?

    fn new(val: T) -> Self {
        // SAFETY: The layout we provided was checked at compiletime, so it has to be initialized correctly
        let alloc = unsafe { alloc(Self::LAYOUT) }.cast::<T>();
        // FIXME: add safety comment
        unsafe { alloc.write(val); }
        Self {
            alloc_ptr: NonNull::new(alloc).unwrap(), // FIXME: can we make this unchecked?
        }
    }

    fn as_ref(&self) -> &T {
        // SAFETY: This is safe because we know that alloc_ptr can't be zero
        // and because we know that alloc_ptr has to point to a valid
        // instance of T in memory
        unsafe { self.alloc_ptr.as_ptr().as_ref().unwrap_unchecked() }
    }

    fn as_mut(&mut self) -> &mut T {
        // SAFETY: This is safe because we know that alloc_ptr can't be zero
        // and because we know that alloc_ptr has to point to a valid
        // instance of T in memory
        unsafe { self.alloc_ptr.as_ptr().as_mut().unwrap_unchecked() }
    }

    fn into_ptr(self) -> NonNull<T> {
        let ret = self.alloc_ptr;
        mem::forget(self);
        ret
    }

}

impl<T> Drop for SizedBox<T> {
    fn drop(&mut self) {
        // SAFETY: This is safe to call because SizedBox can only be dropped once
        unsafe { ptr::drop_in_place(self.alloc_ptr.as_ptr()); }
        // FIXME: add safety comment
        unsafe { dealloc(self.alloc_ptr.as_ptr().cast::<u8>(), SizedBox::<T>::LAYOUT); }
    }
}
