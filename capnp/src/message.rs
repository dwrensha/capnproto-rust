// Copyright (c) 2013-2015 Sandstorm Development Group, Inc. and contributors
// Licensed under the MIT License:
//
// Permission is hereby granted, free of charge, to any person obtaining a copy
// of this software and associated documentation files (the "Software"), to deal
// in the Software without restriction, including without limitation the rights
// to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
// copies of the Software, and to permit persons to whom the Software is
// furnished to do so, subject to the following conditions:
//
// The above copyright notice and this permission notice shall be included in
// all copies or substantial portions of the Software.
//
// THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
// IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
// FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
// AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
// LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
// OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN
// THE SOFTWARE.

//! Untyped root container for a Cap'n Proto value.

use std::convert::From;

use any_pointer;
use private::arena::{BuilderArena, BuilderArenaImpl, ReaderArena, ReaderArenaImpl};
use private::layout;
use private::units::BYTES_PER_WORD;
use traits::{FromPointerBuilder, FromPointerReader, Owned, SetPointerBuilder};
use {OutputSegments, Result, Word};

/// Options controlling how data is read.
#[derive(Clone, Copy, Debug)]
pub struct ReaderOptions {
    /// Limits how many total words of data are allowed to be traversed. Traversal is counted when
    /// a new struct or list builder is obtained, e.g. from a get() accessor. This means that calling
    /// the getter for the same sub-struct multiple times will cause it to be double-counted. Once
    /// the traversal limit is reached, an error will be reported.
    ///
    /// This limit exists for security reasons. It is possible for an attacker to construct a message
    /// in which multiple pointers point at the same location. This is technically invalid, but hard
    /// to detect. Using such a message, an attacker could cause a message which is small on the wire
    /// to appear much larger when actually traversed, possibly exhausting server resources leading to
    /// denial-of-service.
    ///
    /// It makes sense to set a traversal limit that is much larger than the underlying message.
    /// Together with sensible coding practices (e.g. trying to avoid calling sub-object getters
    /// multiple times, which is expensive anyway), this should provide adequate protection without
    /// inconvenience.
    pub traversal_limit_in_words: u64,

    /// Limits how deeply nested a message structure can be, e.g. structs containing other structs or
    /// lists of structs.
    ///
    /// Like the traversal limit, this limit exists for security reasons. Since it is common to use
    /// recursive code to traverse recursive data structures, an attacker could easily cause a stack
    /// overflow by sending a very-depply-nested (or even cyclic) message, without the message even
    /// being very large. The default limit of 64 is probably low enough to prevent any chance of
    /// stack overflow, yet high enough that it is never a problem in practice.
    pub nesting_limit: i32,
}

pub const DEFAULT_READER_OPTIONS: ReaderOptions = ReaderOptions {
    traversal_limit_in_words: 8 * 1024 * 1024,
    nesting_limit: 64,
};

impl Default for ReaderOptions {
    fn default() -> ReaderOptions {
        DEFAULT_READER_OPTIONS
    }
}

impl ReaderOptions {
    pub fn new() -> ReaderOptions {
        DEFAULT_READER_OPTIONS
    }

    pub fn nesting_limit<'a>(&'a mut self, value: i32) -> &'a mut ReaderOptions {
        self.nesting_limit = value;
        self
    }

    pub fn traversal_limit_in_words<'a>(&'a mut self, value: u64) -> &'a mut ReaderOptions {
        self.traversal_limit_in_words = value;
        self
    }
}

/// An object that manages the buffers underlying a Cap'n Proto message reader.
pub trait ReaderSegments {
    /// Gets the segment with index `idx`. Returns `None` if `idx` is out of range.
    fn get_segment<'a>(&'a self, idx: u32) -> Option<&'a [Word]>;

    /// Gets the number of segments.
    fn len(&self) -> usize {
        for i in 0.. {
            if self.get_segment(i as u32).is_none() {
                return i;
            }
        }
        unreachable!()
    }
}

/// An array of segments.
pub struct SegmentArray<'a> {
    segments: &'a [&'a [Word]],
}

impl<'a> SegmentArray<'a> {
    pub fn new(segments: &'a [&'a [Word]]) -> SegmentArray<'a> {
        SegmentArray { segments: segments }
    }
}

impl<'b> ReaderSegments for SegmentArray<'b> {
    fn get_segment<'a>(&'a self, id: u32) -> Option<&'a [Word]> {
        self.segments.get(id as usize).map(|slice| *slice)
    }

    fn len(&self) -> usize {
        self.segments.len()
    }
}

impl<'b> ReaderSegments for [&'b [Word]] {
    fn get_segment<'a>(&'a self, id: u32) -> Option<&'a [Word]> {
        self.get(id as usize).map(|slice| *slice)
    }

    fn len(&self) -> usize {
        self.len()
    }
}

/// A container used to read a message.
pub struct Reader<S>
where
    S: ReaderSegments,
{
    arena: ReaderArenaImpl<S>,
    nesting_limit: i32,
}

impl<S> Reader<S>
where
    S: ReaderSegments,
{
    pub fn new(segments: S, options: ReaderOptions) -> Self {
        Reader {
            arena: ReaderArenaImpl::new(segments, options),
            nesting_limit: options.nesting_limit,
        }
    }

    fn get_root_internal<'a>(&'a self) -> Result<any_pointer::Reader<'a>> {
        let (segment_start, _seg_len) = try!(self.arena.get_segment(0));
        let pointer_reader = try!(layout::PointerReader::get_root(
            &self.arena,
            0,
            segment_start,
            self.nesting_limit
        ));
        Ok(any_pointer::Reader::new(pointer_reader))
    }

    /// Gets the root of the message, interpreting it as the given type.
    pub fn get_root<'a, T: FromPointerReader<'a>>(&'a self) -> Result<T> {
        try!(self.get_root_internal()).get_as()
    }

    pub fn into_segments(self) -> S {
        self.arena.into_segments()
    }

    /// Checks whether the message is [canonical](https://capnproto.org/encoding.html#canonicalization).
    pub fn is_canonical(&self) -> Result<bool> {
        let (segment_start, seg_len) = try!(self.arena.get_segment(0));

        if self.arena.get_segment(1).is_ok() {
            // TODO(cleanup, apibump): should there be a nicer way to ask the arena how many
            // segments there are?

            // There is more than one segment, so the message cannot be canonical.
            return Ok(false);
        }

        let pointer_reader = try!(layout::PointerReader::get_root(
            &self.arena,
            0,
            segment_start,
            self.nesting_limit
        ));
        let read_head = ::std::cell::Cell::new(unsafe { segment_start.offset(1) });
        let root_is_canonical = try!(pointer_reader.is_canonical(&read_head));
        let all_words_consumed = (read_head.get() as usize - segment_start as usize)
            / BYTES_PER_WORD
            == seg_len as usize;
        Ok(root_is_canonical && all_words_consumed)
    }

    /// Gets the [canonical](https://capnproto.org/encoding.html#canonicalization) form
    /// of this message.
    // TODO: Once we've updated code generation so that all generated types implement
    // the new version of SetPointerBuilder, it will probably make sense for this function
    // to live somewhere else and be parameterized by `S: SetPointerBuilder`.
    pub fn canonicalize(&self) -> Result<Vec<Word>> {
        let root = try!(self.get_root_internal());
        let size = try!(root.target_size()).word_count + 1;
        let mut message = Builder::new(HeapAllocator::new().first_segment_words(size as u32));
        try!(message.set_root_canonical(root));
        let output_segments = message.get_segments_for_output();
        assert_eq!(1, output_segments.len());
        let output = output_segments[0];
        assert!(output.len() as u64 <= size);
        let mut result = Word::allocate_zeroed_vec(output.len());
        result.copy_from_slice(output);
        Ok(result)
    }

    pub fn into_typed<T: for<'a> Owned<'a>>(self) -> TypedReader<S, T> {
        TypedReader::new(self)
    }
}

/// A message reader whose value is known to be of type `T`.
pub struct TypedReader<S, T>
where
    S: ReaderSegments,
    T: for<'a> Owned<'a>,
{
    marker: ::std::marker::PhantomData<T>,
    message: Reader<S>,
}

impl<S, T> TypedReader<S, T>
where
    S: ReaderSegments,
    T: for<'a> Owned<'a>,
{
    pub fn new(message: Reader<S>) -> Self {
        TypedReader {
            marker: ::std::marker::PhantomData,
            message: message,
        }
    }

    pub fn get<'a>(&'a self) -> Result<<T as Owned<'a>>::Reader> {
        self.message.get_root()
    }

    pub fn into_inner(self) -> Reader<S> {
        self.message
    }
}

impl<S, T> From<Reader<S>> for TypedReader<S, T>
where
    S: ReaderSegments,
    T: for<'a> Owned<'a>,
{
    fn from(message: Reader<S>) -> TypedReader<S, T> {
        TypedReader::new(message)
    }
}

impl<A, T> From<Builder<A>> for TypedReader<Builder<A>, T>
where
    A: Allocator,
    T: for<'a> Owned<'a>,
{
    fn from(message: Builder<A>) -> TypedReader<Builder<A>, T> {
        let reader = message.into_reader();
        reader.into_typed()
    }
}

/// An object that allocates memory for a Cap'n Proto message as it is being built.
pub unsafe trait Allocator {
    /// Allocates memory for a new segment, returning a pointer to the start of the segment
    /// and a u32 indicating the length of the segment.
    ///
    /// The allocated memory MUST be initialized to all zeroes.
    ///
    /// UNSAFETY ALERT: The callee is responsible for ensuring that the returned memory is valid
    /// for the lifetime of the object and doesn't overlap with other allocated memory.
    fn allocate_segment(&mut self, minimum_size: u32) -> (*mut Word, u32);

    fn pre_drop(&mut self, _segment0_currently_allocated: u32) {}
}

/* TODO(version 0.9): update to a more user-friendly trait here?
pub trait Allocator {
    /// Allocates memory for a new segment.
    fn allocate_segment(&mut self, minimum_size: u32) -> Result<()>;

    fn get_segment<'a>(&'a self, id: u32) -> &'a [Word];
    fn get_segment_mut<'a>(&'a mut self, id: u32) -> &'a mut [Word];

    fn pre_drop(&mut self, _segment0_currently_allocated: u32) {}
}
*/

/// A container used to build a message.
pub struct Builder<A>
where
    A: Allocator,
{
    arena: BuilderArenaImpl<A>,
    cap_table: Vec<Option<Box<::private::capability::ClientHook>>>,
}

// TODO(version 0.9): Consider removing this unsafe impl somwhow.
//   As soon as a message::Builder has caps in its table, it is not
//   in fact safe to send to other threads. Perhaps we should remove
//   the Builder::cap_table field, requiring imbue_mut() to be called
//   manually when the situation calls for it.
unsafe impl<A> Send for Builder<A> where A: Send + Allocator {}

fn _assert_kinds() {
    fn _assert_send<T: Send>() {}
    fn _assert_reader<S: ReaderSegments + Send>() {
        _assert_send::<Reader<S>>();
    }
    fn _assert_builder<A: Allocator + Send>() {
        _assert_send::<Builder<A>>();
    }
}

impl<A> Builder<A>
where
    A: Allocator,
{
    pub fn new(allocator: A) -> Self {
        Builder {
            arena: BuilderArenaImpl::new(allocator),
            cap_table: Vec::new(),
        }
    }

    fn get_root_internal<'a>(&'a mut self) -> any_pointer::Builder<'a> {
        use traits::ImbueMut;
        if self.arena.len() == 0 {
            self.arena
                .allocate_segment(1)
                .expect("allocate root pointer");
            self.arena.allocate(0, 1).expect("allocate root pointer");
        }
        let (seg_start, _seg_len) = self.arena.get_segment_mut(0);
        let location: *mut Word = seg_start;
        let Builder {
            ref mut arena,
            ref mut cap_table,
        } = *self;

        let mut result =
            any_pointer::Builder::new(layout::PointerBuilder::get_root(arena, 0, location));
        result.imbue_mut(cap_table);
        result
    }

    /// Initializes the root as a value of the given type.
    pub fn init_root<'a, T: FromPointerBuilder<'a>>(&'a mut self) -> T {
        let root = self.get_root_internal();
        root.init_as()
    }

    /// Gets the root, interpreting it as the given type.
    pub fn get_root<'a, T: FromPointerBuilder<'a>>(&'a mut self) -> Result<T> {
        let root = self.get_root_internal();
        root.get_as()
    }

    pub fn get_root_as_reader<'a, T: FromPointerReader<'a>>(&'a self) -> Result<T> {
        if self.arena.len() == 0 {
            any_pointer::Reader::new(layout::PointerReader::new_default()).get_as()
        } else {
            use traits::Imbue;
            let (segment_start, _segment_len) = try!(self.arena.get_segment(0));
            let pointer_reader = try!(layout::PointerReader::get_root(
                self.arena.as_reader(),
                0,
                segment_start,
                0x7fffffff
            ));
            let mut root = any_pointer::Reader::new(pointer_reader);
            root.imbue(&self.cap_table);
            root.get_as()
        }
    }

    /// Sets the root to a deep copy of the given value.
    pub fn set_root<To, From: SetPointerBuilder<To>>(&mut self, value: From) -> Result<()> {
        let root = self.get_root_internal();
        root.set_as(value)
    }

    pub fn set_root_canonical<To, From: SetPointerBuilder<To>>(&mut self, value: From) -> Result<()> {
        if self.arena.len() == 0 {
            self.arena
                .allocate_segment(1)
                .expect("allocate root pointer");
            self.arena.allocate(0, 1).expect("allocate root pointer");
        }
        let (seg_start, _seg_len) = self.arena.get_segment_mut(0);
        let pointer = layout::PointerBuilder::get_root(&self.arena, 0, seg_start);
        SetPointerBuilder::set_pointer_builder(pointer, value, true)?;
        assert_eq!(self.get_segments_for_output().len(), 1);
        Ok(())
    }

    pub fn get_segments_for_output<'a>(&'a self) -> OutputSegments<'a> {
        self.arena.get_segments_for_output()
    }

    pub fn into_reader(self) -> Reader<Builder<A>> {
        Reader::new(
            self,
            ReaderOptions {
                traversal_limit_in_words: u64::max_value(),
                nesting_limit: i32::max_value(),
            },
        )
    }
}

impl<A> ReaderSegments for Builder<A>
where
    A: Allocator,
{
    fn get_segment<'a>(&'a self, id: u32) -> Option<&'a [Word]> {
        self.get_segments_for_output().get(id as usize).map(|x| *x)
    }

    fn len(&self) -> usize {
        self.get_segments_for_output().len()
    }
}

#[derive(Debug)]
pub struct HeapAllocator {
    owned_memory: Vec<Vec<Word>>,
    next_size: u32,
    allocation_strategy: AllocationStrategy,
}

#[derive(Clone, Copy, Debug)]
pub enum AllocationStrategy {
    FixedSize,
    GrowHeuristically,
}

pub const SUGGESTED_FIRST_SEGMENT_WORDS: u32 = 1024;
pub const SUGGESTED_ALLOCATION_STRATEGY: AllocationStrategy = AllocationStrategy::GrowHeuristically;

impl HeapAllocator {
    pub fn new() -> HeapAllocator {
        HeapAllocator {
            owned_memory: Vec::new(),
            next_size: SUGGESTED_FIRST_SEGMENT_WORDS,
            allocation_strategy: SUGGESTED_ALLOCATION_STRATEGY,
        }
    }

    pub fn first_segment_words(mut self, value: u32) -> HeapAllocator {
        self.next_size = value;
        self
    }

    pub fn allocation_strategy(mut self, value: AllocationStrategy) -> HeapAllocator {
        self.allocation_strategy = value;
        self
    }
}

unsafe impl Allocator for HeapAllocator {
    fn allocate_segment(&mut self, minimum_size: u32) -> (*mut Word, u32) {
        let size = ::std::cmp::max(minimum_size, self.next_size);
        let mut new_words = Word::allocate_zeroed_vec(size as usize);
        let ptr = new_words.as_mut_ptr();
        self.owned_memory.push(new_words);

        match self.allocation_strategy {
            AllocationStrategy::GrowHeuristically => {
                self.next_size += size;
            }
            _ => {}
        }
        (ptr, size as u32)
    }
}

impl Builder<HeapAllocator> {
    pub fn new_default() -> Builder<HeapAllocator> {
        Builder::new(HeapAllocator::new())
    }
}

#[derive(Debug)]
pub struct ScratchSpace<'a> {
    slice: &'a mut [Word],
    in_use: bool,
}

impl<'a> ScratchSpace<'a> {
    pub fn new(slice: &'a mut [Word]) -> ScratchSpace<'a> {
        ScratchSpace {
            slice: slice,
            in_use: false,
        }
    }
}

pub struct ScratchSpaceHeapAllocator<'a, 'b: 'a> {
    scratch_space: &'a mut ScratchSpace<'b>,
    allocator: HeapAllocator,
}

impl<'a, 'b: 'a> ScratchSpaceHeapAllocator<'a, 'b> {
    pub fn new(scratch_space: &'a mut ScratchSpace<'b>) -> ScratchSpaceHeapAllocator<'a, 'b> {
        ScratchSpaceHeapAllocator {
            scratch_space: scratch_space,
            allocator: HeapAllocator::new(),
        }
    }

    pub fn second_segment_words(self, value: u32) -> ScratchSpaceHeapAllocator<'a, 'b> {
        ScratchSpaceHeapAllocator {
            scratch_space: self.scratch_space,
            allocator: self.allocator.first_segment_words(value),
        }
    }

    pub fn allocation_strategy(
        self,
        value: AllocationStrategy,
    ) -> ScratchSpaceHeapAllocator<'a, 'b> {
        ScratchSpaceHeapAllocator {
            scratch_space: self.scratch_space,
            allocator: self.allocator.allocation_strategy(value),
        }
    }
}

unsafe impl<'a, 'b: 'a> Allocator for ScratchSpaceHeapAllocator<'a, 'b> {
    fn allocate_segment(&mut self, minimum_size: u32) -> (*mut Word, u32) {
        if !self.scratch_space.in_use {
            self.scratch_space.in_use = true;
            (
                self.scratch_space.slice.as_mut_ptr(),
                self.scratch_space.slice.len() as u32,
            )
        } else {
            self.allocator.allocate_segment(minimum_size)
        }
    }

    fn pre_drop(&mut self, segment0_currently_allocated: u32) {
        let ptr = self.scratch_space.slice.as_mut_ptr();
        unsafe {
            ::std::ptr::write_bytes(ptr, 0u8, segment0_currently_allocated as usize);
        }
        self.scratch_space.in_use = false;
    }
}
