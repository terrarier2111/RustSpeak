use aligned::{Aligned, A4};
use std::alloc::{alloc, dealloc, Layout, LayoutError};
use std::arch::asm;
use std::mem::{align_of, needs_drop, size_of, transmute, ManuallyDrop, MaybeUninit};
use std::ops::{Deref, DerefMut};
use std::ptr::{addr_of, addr_of_mut, null_mut, NonNull};
use std::sync::atomic::{fence, AtomicPtr, AtomicU8, Ordering, AtomicUsize};
use std::sync::Arc;
use std::time::Duration;
use std::{mem, ptr, thread};
use std::borrow::Borrow;
use std::marker::PhantomData;
use std::pin::Pin;

// disable MIRI SB(stacked borrows) checks

// this is better for cases where we DON'T care much about removal of nodes during traversal:
// https://www.codeproject.com/Articles/723555/A-Lock-Free-Doubly-Linked-List
// this is better if we DO:
// https://scholar.google.com/citations?view_op=view_citation&hl=de&user=RJmBj1wAAAAJ&citation_for_view=RJmBj1wAAAAJ:UebtZRa9Y70C

// FIXME: NOTE THAT: &mut T can be converted into *mut T by using .into() on the mutable reference!

// FIXME: add guard to nodes in Unbound mode to remove them once all of the references to them get dropped
// FIXME: employ reference counting on the nodes in order for them to be dropped correctly
#[derive(Clone)]
pub struct AtomicDoublyLinkedList<T, const NODE_KIND: NodeKind = { NodeKind::Bound }> {
    header_node: Arc<RawNode<T, NODE_KIND>>,
    // in the header node itself, the left field points to the `header_node` field of the list itself, so we don't have to maintain a reference count
    tail_node: Arc<RawNode<T, NODE_KIND>>,
    // in the tail node itself, the right field points to the `tail_node` field of the list itself, so we don't have to maintain a reference count
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
        if size_of::<Arc<Aligned<A4, AtomicDoublyLinkedListNode<T, NODE_KIND, false>>>>()
            != size_of::<Arc<Aligned<A4, AtomicDoublyLinkedListNode<T, NODE_KIND, true>>>>()
        {
            unreachable!("Arc's size can't be evaluated statically!");
        }
        if align_of::<Arc<Aligned<A4, AtomicDoublyLinkedListNode<T, NODE_KIND, false>>>>()
            != align_of::<Arc<Aligned<A4, AtomicDoublyLinkedListNode<T, NODE_KIND, true>>>>()
        {
            unreachable!("Arc's alignment can't be evaluated statically!");
        }
        /*
        if align_of::<Arc<Aligned<A4, AtomicDoublyLinkedListNode<T, NODE_KIND>>>>() < 4 {
            unreachable!("Arc's alignment isn't sufficient!");
        }*/
        let ret = Arc::new(Self {
            header_node: Arc::new(Aligned(AtomicDoublyLinkedListNode {
                val: MaybeUninit::uninit(),
                left: Link::invalid(),
                right: Link::invalid(),
            })),
            tail_node: Arc::new(Aligned(AtomicDoublyLinkedListNode {
                val: MaybeUninit::uninit(),
                left: Link::invalid(),
                right: Link::invalid(),
            })),
        });

        // SAFETY: we know that there are no other threads setting modifying
        // these nodes and thus they will automatically be correct
        unsafe {
            ret.header_node.right.set_unsafe/*::<false>*/(ret.tail_addr());
            create_ref(ret.tail_addr());
        }
        unsafe {
            ret.tail_node.left.set_unsafe/*::<false>*/(ret.header_addr());
            create_ref(ret.header_addr());
        }

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
        Arc::as_ptr(&self.header_node)
    }

    #[inline]
    fn tail_addr(&self) -> NodePtr<T, NODE_KIND> {
        Arc::as_ptr(&self.tail_node)
    }

    pub fn push_head(&self, val: T) -> Node<T, NODE_KIND> {
        self.header_node.add_after(val)
    }

    pub fn push_tail(&self, val: T) -> Node<T, NODE_KIND> {
        self.tail_node.add_before(val)
    }

    pub fn remove_head(&self) -> Option<Node<T, { NodeKind::Bound }, true>> {
        loop {
            if self.is_empty() {
                return None;
            }
            if NODE_KIND == NodeKind::Bound {
                let head = self
                    .header_node
                    .right
                    .get()
                    .get_ptr()
                    .cast::<RawNode<T, { NodeKind::Bound }>>();
                let head = ManuallyDrop::new(unsafe { Arc::from_raw(head) });
                if let Some(val) = head.remove() {
                    println!("removed head!");
                    println!("rem end head: {:?}", self.header_node.right.get().ptr);
                    println!("rem end tail: {:?}", self.tail_node.left.get().ptr);
                    return Some(val);
                }
            } else {
                let head = self
                    .header_node
                    .right
                    .get()
                    .get_ptr()
                    .cast::<RawNode<T, { NodeKind::Unbound }>>();
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

    pub fn remove_tail(&self) -> Option<Node<T, { NodeKind::Bound }, true>> {
        loop {
            if self.is_empty() {
                return None;
            }
            if NODE_KIND == NodeKind::Bound {
                let tail = self
                    .tail_node
                    .left
                    .get()
                    .get_ptr()
                    .cast::<RawNode<T, { NodeKind::Bound }>>();
                let tail = ManuallyDrop::new(unsafe { Arc::from_raw(tail) });
                if let Some(val) = tail.remove() {
                    println!("removed tail!");
                    println!("rem end head: {:?}", self.header_node.right.get().ptr);
                    println!("rem end tail: {:?}", self.tail_node.left.get().ptr);
                    return Some(val);
                }
            } else {
                let tail = self
                    .tail_node
                    .left
                    .get()
                    .get_ptr()
                    .cast::<RawNode<T, { NodeKind::Unbound }>>();
                // increase the ref count because we want to return a reference to the node and thus we have to create a reference out of thin air
                mem::forget(unsafe { Arc::from_raw(tail) });
                let tail = unsafe { Arc::from_raw(tail) };
                if let Some(val) = tail.remove() {
                    println!("removed tail!");
                    println!("rem end head: {:?}", self.header_node.right.get().ptr);
                    println!("rem end tail: {:?}", self.tail_node.left.get().ptr);
                    return Some(val);
                }
            }
        }
    }

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
pub struct AtomicDoublyLinkedListNode<
    T,
    const NODE_KIND: NodeKind = { NodeKind::Bound },
    const DELETED: bool = false,
> {
    // TODO: change the bool here into an enum!
    val: MaybeUninit<T>,
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
                /*if !next.get_ptr().is_null() &&
                    self.right.ptr.compare_exchange(next.ptr.cast_mut(), ptr::invalid_mut(DELETION_MARKER), Ordering::SeqCst, strongest_failure_ordering(Ordering::SeqCst)).is_ok() {
                    unsafe { release_ref(next.get_ptr()); }
                }*/
                return None;
            }
            println!("pre set right!");
            if self.right.try_set_deletion_marker(next.ptr) {
                println!("did set right!");
                loop {
                    prev = self.left.get();
                    if prev.get_deletion_marker()
                        || self.left.try_set_deletion_marker(prev.ptr)
                    {
                        break;
                    }
                }
                println!("pre head stuff!");
                let prev_tmp = unsafe { Arc::from_raw(prev.get_ptr()) };
                prev = LinkContent {
                    ptr: prev_tmp
                        .correct_prev::<true>(next.get_ptr()), // FIXME: we get an unwrapped none panic in this line (in Bound mode) - that's probably because here we have a header node which we try to deref!
                };
                mem::forget(prev_tmp);
                println!("post head stuff!");
                println!("left del: {}", self.left.get().get_deletion_marker());
                println!("right del: {}", self.left.get().get_deletion_marker());
                /*unsafe {
                    release_ref(prev.get_ptr());
                    release_ref(next.get_ptr());
                    // release_ref(self); // we probably don't need this as we return a reference from this function
                }*/

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
            if self.right.try_set_deletion_marker(next.ptr) {
                println!("did set right!");
                loop {
                    prev = self.left.get();
                    if prev.get_deletion_marker()
                        || self.left.try_set_deletion_marker(prev.ptr)
                    {
                        break;
                    }
                }
                println!("pre head stuff!");
                let prev_tmp = unsafe { Arc::from_raw(prev.get_ptr()) };
                prev = LinkContent {
                    ptr: prev_tmp
                        .correct_prev::<true>(next.get_ptr()), // FIXME: we get an unwrapped none panic in this line (in Bound mode) - that's probably because here we have a header node which we try to deref!
                };
                mem::forget(prev_tmp);
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
    pub fn add_after(self: &Arc<Aligned<A4, Self>>, val: T) -> Node<T, NODE_KIND> {
        let node = Arc::new/*pin*/(Aligned(AtomicDoublyLinkedListNode {
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

    // ptr->ptr->Node
    fn inner_add_after(self: &Arc<Aligned<A4, Self>>, node: &Arc<RawNode<T, NODE_KIND>>) {
        if self.right.get().ptr.is_null() {
            // if we are the tail, add before ourselves, not after
            return self.inner_add_before(node);
        }
        let mut back_off_weight = 1;
        let this = Arc::as_ptr(self);
        let node_ptr = Arc::as_ptr(node);
        let mut next;
        loop {
            next = self.right.get(); // = tail
            unsafe {
                // create_ref(this); // store_ref(node.left, <prev, false>) | in iter 1: head refs += 1
                node.left.set_unsafe/*::<false>*/(this);
            }
            unsafe {
                // create_ref(next.get_ptr()); // store_ref(node.right, <next, false>) | in iter 1: tail refs += 1
                node.right.set_unsafe/*::<false>*/(next.get_ptr());
            }
            if self
                .right
                .try_set_addr_full::<false>(next.get_ptr(), node_ptr)
            {
                break;
            }
            // unsafe { release_ref(next.get_ptr()); } // in iter 1: tail refs -= 1
            if self.right.get().get_deletion_marker() {
                // release_ref(node); // this is probably unnecessary, as we don't delete the node, but reuse it
                // delete_node(node);
                return self.inner_add_before(node);
            }
            // back-off
            thread::sleep(Duration::from_micros(10 * back_off_weight));
            back_off_weight += 1;
        }
        println!("added after!");
        // *cursor = node;
        let prev = self.correct_prev::<false>(next.get_ptr());
        // unsafe { release_ref_2(prev, next.get_ptr()); }
    }

    pub fn add_before(self: &Arc<Aligned<A4, Self>>, val: T) -> Node<T, NODE_KIND> {
        let node = Arc::new/*pin*/(Aligned(AtomicDoublyLinkedListNode {
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

    // FIXME: the original code uses `pointer to pointer to Node` instead of `pointer to Node` as we do, is this the main bug? - it probably is
    // ptr->ptr->Node
    fn inner_add_before(self: &Arc<Aligned<A4, Self>>, node: &Arc<RawNode<T, NODE_KIND>>) {
        if self.left.get().ptr.is_null() {
            // if we are the header, add after ourselves, not before
            return self.inner_add_after(node);
        }
        let mut back_off_weight = 1;
        let this = Arc::as_ptr(self);
        let node_ptr = Arc::as_ptr(node);
        let mut i = 0;
        let left = self.left.get();
        let mut prev = left.get_ptr();
        let mut cursor = this;
        let mut next = cursor;
        loop {
            i += 1;
            println!("in add before loop: {}", i);
            while self.right.get().get_deletion_marker() {
                /*if let Some(node) = unsafe { cursor.as_ref() }.unwrap().clone().next() {
                    cursor = node;
                }*/
                unsafe { cursor.as_ref() }.unwrap().clone().next();
                let prev_tmp = unsafe { Arc::from_raw(prev) };
                prev = prev_tmp
                    .correct_prev::<false>(cursor.cast_mut());
                mem::forget(prev_tmp);
                println!("found del marker!");
            }
            next = cursor;
            unsafe {
                // create_ref(prev); // store_ref(node.left, <prev, false>)
                node.left.set_unsafe/*::<false>*/(prev);
            }
            unsafe {
                // create_ref(next); // store_ref(node.right, <next, false>)
                node.right.set_unsafe/*::<false>*/(cursor.cast_mut());
            }
            if unsafe { prev.as_ref().unwrap() }
                .right
                .try_set_addr_full::<false>(cursor.cast_mut(), node_ptr.cast_mut())
            {
                break;
            }
            let prev_tmp = unsafe { Arc::from_raw(prev) };
            prev = prev_tmp
                .correct_prev::<false>(cursor.cast_mut());
            mem::forget(prev_tmp);
            // back-off
            thread::sleep(Duration::from_micros(10 * back_off_weight));
            back_off_weight += 1;
        }
        // *cursor = node;
        let prev_tmp = unsafe { Arc::from_raw(prev) };
        let prev = prev_tmp
            .correct_prev::<false>(next.cast_mut());
        mem::forget(prev_tmp);
        // unsafe { release_ref_2(prev, next); }
    }

    /// Tries to set the current node's right link
    /// to its following node.
    /// In pseudocode: self.right = self.right.right;
    /// Note, that there are additional deletion checks
    /// being performed before setting the next node.
    /// This method returns true as long as the tail node isn't reached.
    // pointer to pointer to node
    fn next(self: &Aligned<A4, Self>) -> Option<*const RawNode<T, NODE_KIND>> {
        // prepped for header/tail update
        let mut cursor = self as *const RawNode<T, NODE_KIND>;
        loop {
            let next = unsafe { cursor.as_ref() }.unwrap().right.get();
            if next.ptr.is_null() {
                // check if the cursor is the tail, if so - return false
                return None;
            }
            let marker = unsafe { next.get_ptr().as_ref() }
                .unwrap()
                .right
                .get()
                .get_deletion_marker();
            if marker
                && unsafe { cursor.as_ref() }.unwrap().right.get().ptr
                != ptr::from_exposed_addr(next.ptr.expose_addr() | DELETION_MARKER)
            {
                unsafe { next.get_ptr().as_ref() }
                    .unwrap()
                    .left
                    .set_deletion_mark();
                let new_right = unsafe { next.get_ptr().as_ref() }.unwrap().right.get();
                unsafe { cursor.as_ref() }
                    .unwrap()
                    .right
                    .try_set_addr_full::<false>(next.ptr, new_right.get_ptr());
                // unsafe { release_ref(next.get_ptr()); }
                continue;
            }
            // unsafe { release_ref(cursor); }
            cursor = next.get_ptr();
            if !marker
                && !unsafe { next.get_ptr().as_ref() }
                .unwrap()
                .right
                .get()
                .ptr
                .is_null()
            {
                return Some(cursor);
            }
        }
    }

    // FIXME: add prev()

    // header -> node -> this
    // header <- node | header <- this
    // node.correct_prev(this)

    // header -> node | this -> node
    // header <- this <- node


    // head.correct_prev(next) where next = tail, so: head.correct_prev(tail)
    /// tries to update the prev pointer of a node and then return a reference to a possibly
    /// logically previous node
    // ? - type is not annotated
    fn correct_prev<const FROM_DELETION: bool>(
        self: &Arc<Aligned<A4, Self>>,
        node: NodePtr<T, NODE_KIND>,
    ) -> NodePtr<T, NODE_KIND> {
        // let id = rand::random::<usize>();
        // FIXME: currently there is an issue where we go further and further to the right without finding from `self` without finding the `node` we are looking for
        // let initial = Arc::as_ptr(self); // = node
        let mut back_off_weight = 1;
        let mut prev = Arc::as_ptr(self); // = node
        let mut last_link: Option<NodePtr<T, NODE_KIND>> = None;
        let mut i = 0;
        loop {
            /*if i >= 10 {
                loop {}
            }*/
            i += 1;
            // println!("in correct prev loop {} del: {}", i, FROM_DELETION);
            let link_1 = unsafe { node.as_ref().unwrap() }.left.get();
            if link_1.get_deletion_marker() {
                break;
            }
            // println!("prev: {:?} | init: {:?}", prev, initial);
            let mut prev_2 = unsafe { prev.as_ref() }.unwrap().right.get(); // = this | FIXME: this sometimes has a dangling pointer (as reported by MIRI)
            // println!("in correct prev loop {} del: {}\nprev: {:?} | init: {:?} | node: {:?} | prev_2: {:?} | id: {id}", i, FROM_DELETION, prev, initial, node, prev_2.get_ptr());
            if prev_2.get_deletion_marker() {
                println!("del marker!");
                // loop {}
                if let Some(last_link) = last_link.take() {
                    unsafe { prev.as_ref() }.unwrap().left.set_deletion_mark();
                    unsafe { last_link.as_ref().unwrap() }
                        .right
                        .try_set_addr_full::<false>(prev, prev_2.get_ptr());
                    // unsafe { release_ref_2(prev_2.get_ptr(), prev); }
                    prev = last_link;
                    continue;
                }
                // unsafe { release_ref(prev_2.get_ptr()); }
                prev_2 = unsafe { prev.as_ref() }.unwrap().left.get();
                // unsafe { release_ref(prev); }
                prev = prev_2.get_ptr();
                println!("continuing after del marker check {:?}", prev_2.ptr);
                continue;
            }
            if /*prev_2.get_ptr()*/prev_2.ptr != node {
                println!("prev_2: {:?}", prev_2.ptr);
                println!("node: {:?}", node);
                if let Some(last_link) = last_link.replace(prev) {
                    // unsafe { release_ref(last_link); }
                }
                prev = prev_2.get_ptr();
                println!("continuing after prev_2 = node check");
                continue;
            }
            // unsafe { release_ref(prev_2.get_ptr()); } // tail refs -= 1

            if unsafe { node.as_ref().unwrap() }
                .left
                .try_set_addr_full::<false>(link_1.ptr, prev)
            {
                if unsafe { prev.as_ref() }
                    .unwrap()
                    .left
                    .get()
                    .get_deletion_marker()
                {
                    println!("continuing in the end!");
                    continue;
                }
                break;
            }
            // back-off
            thread::sleep(Duration::from_micros(10 * back_off_weight));
            back_off_weight += 1;
        }
        println!("finished correct prev!");
        /*if let Some(last_link) = last_link.take() {
            unsafe { release_ref(last_link); } // head refs -= 1
        }*/
        prev
    }
}

impl<T, const NODE_KIND: NodeKind, const DELETED: bool>
AtomicDoublyLinkedListNode<T, NODE_KIND, DELETED>
{
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

impl<T, const NODE_KIND: NodeKind, const DELETED: bool> Drop
for AtomicDoublyLinkedListNode<T, NODE_KIND, DELETED>
{
    fn drop(&mut self) {
        if self.right.get().ptr.is_null() || self.left.get().ptr.is_null() {
            // don't do anything when the header or tail nodes get dropped
            return;
        }
        /*
        if NODE_KIND == NodeKind::Unbound && !DELETED { // FIXME: add an detached marker and check it here!
            // FIXME: remove this node from the list! - put this code inside a wrapper
        }*/
        unsafe {
            release_ref(self.right.get().get_ptr());
            release_ref(self.left.get().get_ptr());
        }
        // SAFETY: this is safe because this is the only time
        // the node and thus `val` can be dropped
        unsafe {
            self.val.assume_init_drop();
        }
    }
}

const DELETION_MARKER: usize = 1 << 1/*63*/;
const DETACHED_MARKER: usize = 1 << 0/*62*/; // FIXME: can we replace this marker with nulling pointers?

#[derive(Debug)]
struct Link<T, const NODE_KIND: NodeKind = { NodeKind::Bound }> {
    ptr: Aligned<A4, AtomicPtr<RawNode<T, NODE_KIND>>>,
}

impl<T, const NODE_KIND: NodeKind> Link<T, NODE_KIND> {
    const CAS_ORDERING: Ordering = Ordering::SeqCst;

    fn get(&self) -> LinkContent<T, NODE_KIND> {
        LinkContent {
            ptr: self.ptr.load(Ordering::SeqCst/*Ordering::Relaxed*/),
        }
    }

    fn set_deletion_mark(&self) {
        let mut node = self.get();
        'retry: loop {
            if node.get_deletion_marker() {
                break;
            }
            let cmp = self
                .ptr
                .compare_exchange_weak(
                    node.ptr.cast_mut(),
                    ptr::from_exposed_addr_mut(node.ptr.expose_addr() | DELETION_MARKER),
                    Self::CAS_ORDERING,
                    strongest_failure_ordering(Self::CAS_ORDERING),
                );
            match cmp {
                Ok(_) => {
                    break 'retry;
                }
                Err(curr_val) => {
                    // retry if exchange failed
                    node = LinkContent {
                        ptr: curr_val.cast_const(),
                    };
                }
            }
        }
    }

    fn try_set_addr_full<const SET_DELETION_MARKER: bool>(
        &self,
        old: NodePtr<T, NODE_KIND>,
        new: NodePtr<T, NODE_KIND>,
    ) -> bool {
        unsafe { create_ref(new); } // FIXME: don't create references prematurely before even knowing whether the update will succeed or not

        let res = self.ptr
            .compare_exchange(
                old.cast_mut(),
                if SET_DELETION_MARKER {
                    ptr::from_exposed_addr_mut(new.expose_addr() | DELETION_MARKER)
                } else {
                    new.cast_mut()
                },
                Self::CAS_ORDERING,
                strongest_failure_ordering(Self::CAS_ORDERING),
            )
            .is_ok();

        if res {
            unsafe { release_ref(old); } // decrease the old reference count
        } else {
            unsafe { release_ref(new); } // decrease the new reference count if the update failed
        }

        res
    }

    fn try_set_deletion_marker/*<const SET_DELETION_MARKER: bool>*/(&self, old: NodePtr<T, NODE_KIND>) -> bool {
        self.try_set_addr_full::<true/*SET_DELETION_MARKER*/>(old, old)
    }

    // SAFETY: This may only be used on new nodes that are in no way
    // linked to and that link to nowhere.
    unsafe fn set_unsafe/*<const SET_DELETION_MARKER: bool>*/(&self, new: NodePtr<T, NODE_KIND>) {
        /*let marker = if SET_DELETION_MARKER {
            DELETION_MARKER
        } else {
            0
        };*/
        create_ref(new);
        self.ptr.store(
            new.cast_mut()/*ptr::from_exposed_addr_mut(new.expose_addr() | marker)*/,
            /*Ordering::Relaxed*/Self::CAS_ORDERING,
        );
        /*
        let prev = self.ptr.swap(
            new.cast_mut()/*ptr::from_exposed_addr_mut(new.expose_addr() | marker)*/,
            /*Ordering::Relaxed*/Self::CAS_ORDERING,
        );
        let prev = LinkContent {
            ptr: prev,
        };
        if !prev.get_ptr().is_null() {

        }
        */
    }

    fn cleanup(&self) {
        // FIXME: once the DETACHED flag is properly supported everywhere, set the ptr to DETACHED (and MAYBE add the DELETION flag)
    }

    /*
    unsafe fn invalidate(&self) {
        self.ptr.store(null_mut(), Self::CAS_ORDERING);
    }

    fn is_invalid(&self) -> bool {
        self.ptr.load(Self::CAS_ORDERING).is_null()
    }*/

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

    fn get_detached_marker(&self) -> bool {
        (self.ptr.expose_addr() & DETACHED_MARKER) != 0
    }

    fn get_ptr(&self) -> NodePtr<T, NODE_KIND> {
        ptr::from_exposed_addr_mut(
            self.ptr.expose_addr() & (!(DELETION_MARKER | DETACHED_MARKER)),
        )
    }
}

impl<T, const NODE_KIND: NodeKind> PartialEq for LinkContent<T, NODE_KIND> {
    fn eq(&self, other: &Self) -> bool {
        self.ptr.expose_addr() == other.ptr.expose_addr()
    }
}

unsafe fn release_ref<T, const NODE_KIND: NodeKind>(node_ptr: NodePtr<T, NODE_KIND>) {
    // decrement the reference count of the arc
    Arc::from_raw(node_ptr);
}

unsafe fn release_ref_2<T, const NODE_KIND: NodeKind>(node_ptr: NodePtr<T, NODE_KIND>, node_ptr_2: NodePtr<T, NODE_KIND>) {
    // FIXME: is this the correct handling of release_ref with 2 params?
    // release_ref(node_ptr);
    // release_ref(node_ptr_2);
}

unsafe fn create_ref<T, const NODE_KIND: NodeKind>(node_ptr: NodePtr<T, NODE_KIND>) {
    let owned = Arc::from_raw(node_ptr);
    mem::forget(owned.clone());
    mem::forget(owned);
}

type NodePtr<T, const NODE_KIND: NodeKind, const REMOVED: bool = false> =
*const RawNode<T, NODE_KIND, REMOVED>;
type RawNode<T, const NODE_KIND: NodeKind, const REMOVED: bool = false> =
Aligned<A4, AtomicDoublyLinkedListNode<T, NODE_KIND, REMOVED>>;
pub type Node<T, const NODE_KIND: NodeKind, const REMOVED: bool = false> =
/*Pin<*/Arc<Aligned<A4, AtomicDoublyLinkedListNode<T, NODE_KIND, REMOVED>>>/*>*/;

struct SizedBox<T> {
    // FIXME: NOTE: this thing could basically be replaced by Box::leak
    alloc_ptr: NonNull<T>,
}

impl<T> SizedBox<T> {
    const LAYOUT: Layout = Layout::from_size_align(size_of::<T>(), align_of::<T>())
        .ok()
        .unwrap(); // FIXME: can we somehow retain the error message?

    fn new(val: T) -> Self {
        // SAFETY: The layout we provided was checked at compiletime, so it has to be initialized correctly
        let alloc = unsafe { alloc(Self::LAYOUT) }.cast::<T>();
        // FIXME: add safety comment
        unsafe {
            alloc.write(val);
        }
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
        unsafe {
            ptr::drop_in_place(self.alloc_ptr.as_ptr());
        }
        // FIXME: add safety comment
        unsafe {
            dealloc(self.alloc_ptr.as_ptr().cast::<u8>(), SizedBox::<T>::LAYOUT);
        }
    }
}

/*
pub struct DropOwned {

}

pub enum DropStrategy {
    MemCpy, // this uses MaybeUninit and mem::replace
    Deref,  // this uses Option::take
}*/

pub trait OwnedDrop: Sized {

    fn drop_owned(self);

}

#[repr(transparent)]
pub struct DropOwnedMemCpy<T: OwnedDrop> {
    inner: MaybeUninit<T>,
}

impl<T: OwnedDrop> DropOwnedMemCpy<T> {

    pub fn new(val: T) -> Self {
        Self {
            inner: MaybeUninit::new(val),
        }
    }

}

impl<T: OwnedDrop> Drop for DropOwnedMemCpy<T> {
    fn drop(&mut self) {
        let owned = mem::replace(&mut self.inner, MaybeUninit::uninit());
        // SAFETY: This is safe because the previous inner value has to be
        // initialized because `DropOwnedMemCpy` can only be created with
        // an initialized value.
        unsafe { owned.assume_init() }.drop_owned();
    }
}

impl<T: OwnedDrop> Deref for DropOwnedMemCpy<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        unsafe { self.inner.assume_init_ref() }
    }
}

impl<T: OwnedDrop> DerefMut for DropOwnedMemCpy<T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        unsafe { self.inner.assume_init_mut() }
    }
}

impl<T: OwnedDrop> From<T> for DropOwnedMemCpy<T> {
    fn from(val: T) -> Self {
        DropOwnedMemCpy::new(val)
    }
}

/*
pub struct DropOwnedDeref<T: OwnedDrop> {
    inner: Option<T>,
}

impl<T: OwnedDrop> DropOwnedDeref<T> {

    pub fn new(val: T) -> Self {
        Self {
            inner: Some(val),
        }
    }

}

impl<T: OwnedDrop> Drop for DropOwnedDeref<T> {
    fn drop(&mut self) {
        let owned = self.inner.take();
        // SAFETY: This is safe because the previous inner value has to be
        // initialized because `DropOwnedMemCpy` can only be created with
        // an initialized value.
        unsafe { owned.unwrap_unchecked() }.drop_owned();
    }
}

impl<T: OwnedDrop> Deref for DropOwnedDeref<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        unsafe { self.inner.as_ref().unwrap_unchecked() }
    }
}

impl<T: OwnedDrop> DerefMut for DropOwnedDeref<T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        unsafe { self.inner.as_mut().unwrap_unchecked() }
    }
}

impl<T: OwnedDrop> From<T> for DropOwnedDeref<T> {
    fn from(val: T) -> Self {
        DropOwnedDeref::new(val)
    }
}*/

/// A `SwapArc` is a data structure that allows for an `Arc`
/// to be passed around and swapped out with other `Arc`s.
/// In order to achieve this, an internal reference count
/// scheme is used which allows for very quick, low overhead
/// reads in the common case (no update) and will sill be
/// decently fast when an update is performed, as updates
/// only consist of 3 atomic instructions. When a new
/// `Arc` is to be stored in the `SwapArc`, it first tries to
/// immediately update the current pointer (the one all readers will see)
/// (this is possible, if no other update is being performed and if there are no readers left)
/// if this fails, it will `push` the update so that it will
/// be performed by the last reader to finish reading.
/// A read consists of loading the current pointer and
/// performing a clone operation on the `Arc`, thus
/// readers are very short-lived and shouldn't block
/// updates for very long, although writer starvation
/// is possible in theory, it probably won't every be
/// observed in practice because of the short-lived
/// nature of readers.
pub struct SwapArc<T> {
    ref_cnt: AtomicUsize, // the last bit is the `update` bit
    ptr: AtomicPtr<T>,
    updated: AtomicPtr<T>,
}

impl<T> SwapArc<T> {

    const UPDATE: usize = 1 << (usize::BITS - 1);
    const FORCE_UPDATE: usize = 1 << (usize::BITS - 1); // FIXME: do we actually need a separate flag? - we probably do
    // FIXME: implement force updating!
    pub fn load_full(&self) -> Arc<T> {
        let mut ref_cnt = self.ref_cnt.fetch_add(1, Ordering::SeqCst);
        // wait until the update finished
        while ref_cnt & Self::UPDATE != 0 {
            ref_cnt = self.ref_cnt.load(Ordering::SeqCst);
        }
        let tmp = unsafe { Arc::from_raw(self.ptr.load(Ordering::Acquire)) };
        let ret = tmp.clone();
        // create a `virtual reference` to the Arc to ensure it doesn't get dropped until we can allow it to be so
        mem::forget(tmp);
        let curr = self.ref_cnt.fetch_sub(1, Ordering::SeqCst);
        // fast-rejection path to ensure we are only trying to update if it's worth it
        if (curr == 0/* || curr == Self::UPDATE*/) && !self.updated.load(Ordering::SeqCst).is_null() {
            self.try_update();
        }
        ret
    }

    fn try_update(&self) {
        match self.ref_cnt.compare_exchange(0, Self::UPDATE, Ordering::SeqCst, Ordering::SeqCst) {
            Ok(_)/* | Err(Self::UPDATE)*/ => {
                // take the update
                let update = self.updated.swap(null_mut(), Ordering::SeqCst);
                // check if we even have an update
                if !update.is_null() {
                    // update the pointer
                    let prev = self.ptr.swap(update, Ordering::Release);
                    // unset the update flag
                    self.ref_cnt.fetch_and(!Self::UPDATE, Ordering::SeqCst);
                    // drop the `virtual reference` we hold to the Arc
                    unsafe { Arc::from_raw(prev); }
                } else {
                    // unset the update flag
                    self.ref_cnt.fetch_and(!Self::UPDATE, Ordering::SeqCst);
                }
            }
            _ => {}
        }
    }

    pub fn update(&self, updated: Arc<T>) {
        loop {
            match self.ref_cnt.compare_exchange(0, Self::UPDATE, Ordering::SeqCst, Ordering::SeqCst) {
                Ok(_) => {
                    // clear out old updates to make sure our update won't be overwritten by them in the future
                    self.updated.store(null_mut(), Ordering::SeqCst);
                    let prev = self.ptr.swap(Arc::into_raw(updated).cast_mut(), Ordering::Release);
                    // unset the update flag
                    self.ref_cnt.fetch_and(!Self::UPDATE, Ordering::SeqCst);
                    // drop the `virtual reference` we hold to the Arc
                    unsafe { Arc::from_raw(prev); }
                    break;
                }
                Err(old) => {
                    if old & Self::UPDATE != 0 {
                        // somebody else already updates the current ptr, so we wait until they finish their update
                        continue;
                    }
                    // push our update up, so it will be applied in the future
                    self.updated.store(Arc::into_raw(updated).cast_mut(), Ordering::SeqCst);
                    break;
                }
            }
        }
    }

    /// This will force an update, this means that new
    /// readers will have to wait for all old readers to
    /// finish and to update the ptr, even when no update
    /// is queued this will block new readers for a short
    /// amount of time, until failure got detected
    fn dummy() {}
    /*fn force_update(&self) -> UpdateResult {
        let curr = self.ref_cnt.fetch_or(Self::FORCE_UPDATE, Ordering::SeqCst);
        if curr & Self::UPDATE != 0 {
            return UpdateResult::AlreadyUpdating;
        }
        if self.updated.load(Ordering::SeqCst).is_null() {
            // unset the flag, as there are no upcoming updates
            self.ref_cnt.fetch_and(!Self::UPDATE, Ordering::SeqCst);
            return UpdateResult::NoUpdate;
        }
        UpdateResult::Ok
    }*/

}

#[derive(Copy, Clone, Debug)]
pub enum UpdateResult {
    Ok,
    AlreadyUpdating,
    NoUpdate,
}


/// A `SwapArc` is a data structure that allows for an `Arc`
/// to be passed around and swapped out with other `Arc`s.
/// In order to achieve this, an internal reference count
/// scheme is used which allows for very quick, low overhead
/// reads in the common case (no update) and will sill be
/// decently fast when an update is performed, as updates
/// only consist of 3 atomic instructions. When a new
/// `Arc` is to be stored in the `SwapArc`, it first tries to
/// immediately update the current pointer (the one all readers will see)
/// (this is possible, if no other update is being performed and if there are no readers left)
/// if this fails, it will `push` the update so that it will
/// be performed by the last reader to finish reading.
/// A read consists of loading the current pointer and
/// performing a clone operation on the `Arc`, thus
/// readers are very short-lived and shouldn't block
/// updates for very long, although writer starvation
/// is possible in theory, it probably won't every be
/// observed in practice because of the short-lived
/// nature of readers.

/// This variant of `SwapArc` has wait-free reads (although
/// this is at the cost of additional atomic instructions
/// (at most 2 additional updates).
pub struct SwapArcIntermediate<T, D: DataPtrConvert<T> = Arc<T>> {
    curr_ref_cnt: AtomicUsize, // the last bit is the `update` bit
    ptr: AtomicPtr<T>,
    intermediate_ref_cnt: AtomicUsize, // the last bit is the `update` bit
    intermediate_ptr: AtomicPtr<T>,
    updated: AtomicPtr<T>,
    _phantom_data: PhantomData<D>,
}

impl<T, D: DataPtrConvert<T>> SwapArcIntermediate<T, D> {

    const UPDATE: usize = 1 << (usize::BITS - 1);
    // const FORCE_UPDATE: usize = 1 << (usize::BITS - 2); // FIXME: do we actually need a separate flag? - we probably do
    // FIXME: implement force updating!
    const OTHER_UPDATE: usize = 1 << (usize::BITS - 2);

    pub fn new(val: Arc<T>) -> Arc<Self> {
        let virtual_ref = Arc::into_raw(val);
        Arc::new(Self {
            curr_ref_cnt: Default::default(),
            ptr: AtomicPtr::new(virtual_ref.cast_mut()),
            intermediate_ref_cnt: Default::default(),
            intermediate_ptr: AtomicPtr::new(null_mut()),
            updated: AtomicPtr::new(null_mut()),
            _phantom_data: Default::default(),
        })
    }

    pub fn load<'a>(self: &'a Arc<Self>) -> SwapArcIntermediateGuard<'a, T, D> {
        let ref_cnt = self.curr_ref_cnt.fetch_add(1, Ordering::SeqCst);
        let (ptr, src) = if ref_cnt & Self::UPDATE != 0 {
            let intermediate_ref_cnt = self.intermediate_ref_cnt.fetch_add(1, Ordering::SeqCst);
            if intermediate_ref_cnt & Self::UPDATE != 0 {
                let ret = self.ptr.load(Ordering::Acquire);
                // release the redundant reference
                self.intermediate_ref_cnt.fetch_sub(1, Ordering::SeqCst);
                (ret, RefSource::Curr)
            } else {
                let ret = self.intermediate_ptr.load(Ordering::Acquire);
                // release the redundant reference
                self.curr_ref_cnt.fetch_sub(1, Ordering::SeqCst);
                (ret, RefSource::Intermediate)
            }
        } else {
            (self.ptr.load(Ordering::Acquire), RefSource::Curr)
        };
        // create a `virtual reference` to the Arc to ensure it doesn't get dropped until we can allow it to be so
        let fake_ref = ManuallyDrop::new(D::from(ptr));
        SwapArcIntermediateGuard::new(fake_ref, self, src)
    }

    pub fn load_full(self: &Arc<Self>) -> D {
        self.load().as_ref().clone()
    }

    fn try_update_curr(&self) -> bool {
        match self.curr_ref_cnt.compare_exchange(0, Self::UPDATE, Ordering::SeqCst, Ordering::SeqCst) {
            Ok(_) => {
                // FIXME: can we somehow bypass intermediate if we have a new update upcoming - we probably can't because this would probably cause memory leaks and other funny things that we don't like
                let intermediate = self.intermediate_ptr.load(Ordering::SeqCst);
                // update the pointer
                let prev = self.ptr.swap(intermediate, Ordering::Release);
                // unset the update flag
                self.curr_ref_cnt.fetch_and(!Self::UPDATE, Ordering::SeqCst);
                // unset the `weak` update flag from the intermediate ptr
                self.intermediate_ptr.fetch_and(!Self::OTHER_UPDATE, Ordering::SeqCst);
                // drop the `virtual reference` we hold to the Arc
                D::from(prev);
                true
            }
            _ => false,
        }
    }

    fn try_update_intermediate(&self) {
        match self.intermediate_ref_cnt.compare_exchange(0, Self::UPDATE | Self::OTHER_UPDATE, Ordering::SeqCst, Ordering::SeqCst) {
            Ok(_) => {
                // take the update
                let update = self.updated.swap(null_mut(), Ordering::SeqCst);
                // check if we even have an update
                if !update.is_null() {
                    let _prev = self.intermediate_ptr.swap(update, Ordering::Release);
                    // unset the update flag
                    self.intermediate_ref_cnt.fetch_and(!Self::UPDATE, Ordering::SeqCst);
                    // try finishing the update up!
                    match self.curr_ref_cnt.compare_exchange(0, Self::UPDATE, Ordering::SeqCst, Ordering::SeqCst) {
                        Ok(_) => {
                            let prev = self.ptr.swap(update, Ordering::Release);
                            // unset the update flag
                            self.curr_ref_cnt.fetch_and(!Self::UPDATE, Ordering::SeqCst);
                            // unset the `weak` update flag from the intermediate ptr
                            self.intermediate_ptr.fetch_and(!Self::OTHER_UPDATE, Ordering::SeqCst);
                            // drop the `virtual reference` we hold to the Arc
                            D::from(prev);
                        }
                        Err(_) => {}
                    }
                } else {
                    // unset the update flag
                    self.intermediate_ref_cnt.fetch_and(!Self::UPDATE, Ordering::SeqCst);
                }
            }
            Err(_) => {}
        }
    }

    pub fn update(&self, updated: D) {
        loop {
            match self.intermediate_ref_cnt.compare_exchange(0, Self::UPDATE | Self::OTHER_UPDATE, Ordering::SeqCst, Ordering::SeqCst) {
                Ok(_) => {
                    let new = updated.into().cast_mut();
                    // clear out old updates to make sure our update won't be overwritten by them in the future
                    let old = self.updated.swap(null_mut(), Ordering::SeqCst);
                    let _prev = self.intermediate_ptr.swap(new, Ordering::Release);
                    // unset the update flag
                    self.intermediate_ref_cnt.fetch_and(!Self::UPDATE, Ordering::SeqCst);
                    if !old.is_null() {
                        // drop the `virtual reference` we hold to the Arc
                        D::from(old);
                    }
                    // try finishing the update up!
                    match self.curr_ref_cnt.compare_exchange(0, Self::UPDATE, Ordering::SeqCst, Ordering::SeqCst) {
                        Ok(_) => {
                            let prev = self.ptr.swap(new, Ordering::Release);
                            // unset the update flag
                            self.curr_ref_cnt.fetch_and(!Self::UPDATE, Ordering::SeqCst);
                            // unset the `weak` update flag from the intermediate ptr
                            self.intermediate_ptr.fetch_and(!Self::OTHER_UPDATE, Ordering::SeqCst);
                            // drop the `virtual reference` we hold to the Arc
                            D::from(prev);
                        }
                        Err(_) => {}
                    }
                    break;
                }
                Err(old) => {
                    if old & Self::UPDATE != 0 { // FIXME: what about Self::UPDATE_OTHER?
                        // somebody else already updates the current ptr, so we wait until they finish their update
                        continue;
                    }
                    // push our update up, so it will be applied in the future
                    let old = self.updated.swap(updated.into().cast_mut(), Ordering::SeqCst);
                    if !old.is_null() {
                        // drop the `virtual reference` we hold to the Arc
                        D::from(old);
                    }
                    break;
                }
            }
        }
    }

    /// This will force an update, this means that new
    /// readers will have to wait for all old readers to
    /// finish and to update the ptr, even when no update
    /// is queued this will block new readers for a short
    /// amount of time, until failure got detected
    fn dummy() {}
    /*fn force_update(&self) -> UpdateResult {
        let curr = self.ref_cnt.fetch_or(Self::FORCE_UPDATE, Ordering::SeqCst);
        if curr & Self::UPDATE != 0 {
            return UpdateResult::AlreadyUpdating;
        }
        if self.updated.load(Ordering::SeqCst).is_null() {
            // unset the flag, as there are no upcoming updates
            self.ref_cnt.fetch_and(!Self::UPDATE, Ordering::SeqCst);
            return UpdateResult::NoUpdate;
        }
        UpdateResult::Ok
    }*/

}

impl<T, D: DataPtrConvert<T>> Drop for SwapArcIntermediate<T, D> {
    fn drop(&mut self) {
        // FIXME: how should we handle intermediate and update inside drop?
        // drop the current arc
        D::from(self.ptr.load(Ordering::SeqCst));
    }
}

pub struct SwapArcIntermediateGuard<'a, T, D: DataPtrConvert<T> = Arc<T>> {
    parent: &'a Arc<SwapArcIntermediate<T, D>>,
    fake_ref: ManuallyDrop<D>,
    ref_src: RefSource,
}

impl<'a, T, D: DataPtrConvert<T>> SwapArcIntermediateGuard<'a, T, D> {
    fn new(fake_ref: ManuallyDrop<D>, parent: &'a Arc<SwapArcIntermediate<T, D>>, ref_src: RefSource) -> Self {
        Self {
            parent,
            fake_ref,
            ref_src,
        }
    }
}

impl<T, D: DataPtrConvert<T>> Drop for SwapArcIntermediateGuard<'_, T, D> {
    fn drop(&mut self) {
        // release the reference we hold
        match self.ref_src {
            RefSource::Curr => {
                let _ref_cnt = self.parent.curr_ref_cnt.fetch_sub(1, Ordering::SeqCst);
                // FIXME: is it possible to try performing an update here as well?
            }
            RefSource::Intermediate => {
                let ref_cnt = self.parent.intermediate_ref_cnt.fetch_sub(1, Ordering::SeqCst);
                // fast-rejection path to ensure we are only trying to update if it's worth it
                // Note: UPDATE is set (seldom) on the immediate ref_cnt if there is a forced update waiting in the queue
                if (ref_cnt == 0 || ref_cnt == SwapArcIntermediate::<T>::UPDATE) && !self.parent.updated.load(Ordering::SeqCst).is_null() {
                    self.parent.try_update_curr();
                }
            }
        }
    }
}

impl<T, D: DataPtrConvert<T>> Deref for SwapArcIntermediateGuard<'_, T, D> {
    type Target = D;

    #[inline]
    fn deref(&self) -> &Self::Target {
        self.fake_ref.deref()
    }
}

impl<T, D: DataPtrConvert<T>> Borrow<D> for SwapArcIntermediateGuard<'_, T, D> {
    #[inline]
    fn borrow(&self) -> &D {
        self.fake_ref.deref()
    }
}

impl<T, D: DataPtrConvert<T>> AsRef<D> for SwapArcIntermediateGuard<'_, T, D> {
    #[inline]
    fn as_ref(&self) -> &D {
        self.fake_ref.deref()
    }
}

enum RefSource {
    Curr,
    Intermediate,
}

/// SAFETY: Types implementing this trait are expected to perform
/// reference counting through cloning/dropping internally.
pub unsafe trait RefCnt: Clone {}

pub trait DataPtrConvert<T>: RefCnt + Sized {

    /// This function may not alter the reference count of the
    /// reference counted "object".
    fn from(ptr: *const T) -> Self;

    /// This function should decrement the reference count of the
    /// reference counted "object" indirectly, by automatically
    /// decrementing it on drop inside the "object"'s drop
    /// implementation.
    fn into(self) -> *const T;

}

unsafe impl<T> RefCnt for Arc<T> {}

impl<T> DataPtrConvert<T> for Arc<T> {
    fn from(ptr: *const T) -> Self {
        unsafe { Arc::from_raw(ptr) }
    }

    fn into(self) -> *const T {
        Arc::into_raw(self)
    }
}
