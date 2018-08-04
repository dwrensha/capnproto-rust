// Copyright (c) 2013-2017 Sandstorm Development Group, Inc. and contributors
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

use std::cell::{Cell, RefCell};
use std::slice;
use std::u64;

use message;
use message::{Allocator, ReaderSegments};
use private::units::*;
use {Error, OutputSegments, Result, Word};

pub type SegmentId = u32;

pub struct ReadLimiter {
    pub limit: Cell<u64>,
}

impl ReadLimiter {
    pub fn new(limit: u64) -> ReadLimiter {
        ReadLimiter {
            limit: Cell::new(limit),
        }
    }

    #[inline]
    pub fn can_read(&self, amount: u64) -> Result<()> {
        let current = self.limit.get();
        if amount > current {
            Err(Error::failed(format!("read limit exceeded")))
        } else {
            self.limit.set(current - amount);
            Ok(())
        }
    }
}

pub trait ReaderArena {
    fn get_segment(&self, id: u32) -> Result<(*const Word, u32)>;
    fn check_offset(
        &self,
        segment_id: u32,
        start: *const Word,
        offset_in_words: i32,
    ) -> Result<*const Word>;
    fn contains_interval(&self, segment_id: u32, start: *const Word, size: usize) -> Result<()>;
    fn amplified_read(&self, virtual_amount: u64) -> Result<()>;

    // TODO(version 0.9): Consider putting extract_cap(), inject_cap(), drop_cap() here
    //   and on message::Reader. Then we could get rid of Imbue and ImbueMut, and
    //   layout::StructReader, layout::ListReader, etc. could drop their `cap_table` fields.
}

pub struct ReaderArenaImpl<S> {
    segments: S,
    read_limiter: ReadLimiter,
}

impl<S> ReaderArenaImpl<S>
where
    S: ReaderSegments,
{
    pub fn new(segments: S, options: message::ReaderOptions) -> Self {
        let limiter = ReadLimiter::new(options.traversal_limit_in_words);
        ReaderArenaImpl {
            segments: segments,
            read_limiter: limiter,
        }
    }

    pub fn into_segments(self) -> S {
        self.segments
    }
}

impl<S> ReaderArena for ReaderArenaImpl<S>
where
    S: ReaderSegments,
{
    fn get_segment<'a>(&'a self, id: u32) -> Result<(*const Word, u32)> {
        match self.segments.get_segment(id) {
            Some(seg) => Ok((seg.as_ptr(), seg.len() as u32)),
            None => Err(Error::failed(format!("Invalid segment id: {}", id))),
        }
    }

    fn check_offset(
        &self,
        segment_id: u32,
        start: *const Word,
        offset_in_words: i32,
    ) -> Result<*const Word> {
        let (segment_start, segment_len) = try!(self.get_segment(segment_id));
        let this_start: usize = segment_start as usize;
        let this_size: usize = segment_len as usize * BYTES_PER_WORD;
        let offset: i64 = offset_in_words as i64 * BYTES_PER_WORD as i64;
        let start_idx = start as usize;
        if start_idx < this_start || ((start_idx - this_start) as i64 + offset) as usize > this_size
        {
            Err(Error::failed(format!(
                "message contained out-of-bounds pointer"
            )))
        } else {
            unsafe { Ok(start.offset(offset_in_words as isize)) }
        }
    }

    fn contains_interval(&self, id: u32, start: *const Word, size_in_words: usize) -> Result<()> {
        let (segment_start, segment_len) = try!(self.get_segment(id));
        let this_start: usize = segment_start as usize;
        let this_size: usize = segment_len as usize * BYTES_PER_WORD;
        let start = start as usize;
        let size = size_in_words * BYTES_PER_WORD;

        if !(start >= this_start && start - this_start + size <= this_size) {
            Err(Error::failed(format!(
                "message contained out-of-bounds pointer"
            )))
        } else {
            self.read_limiter.can_read(size_in_words as u64)
        }
    }

    fn amplified_read(&self, virtual_amount: u64) -> Result<()> {
        self.read_limiter.can_read(virtual_amount)
    }
}

pub trait BuilderArena: ReaderArena {
    // These methods all take an immutable &self because otherwise a StructBuilder<'a>
    // would need a `&'a mut BuilderArena` and `StructBuilder::borrow()` would
    // have lifetime issues. (If `'a: 'b`, then a `&'a (BuilderArena + 'a)` can be
    // converted to a `&'b (BuilderArena + 'b)`, but a `&'a mut (BuilderArena + 'a)`
    // *cannot* be converted to a `&'b (BuilderArena + 'b)`. See some discussion here:
    // https://botbot.me/mozilla/rust/2017-01-31/?msg=80228117&page=19 .)
    fn allocate(&self, segment_id: u32, amount: WordCount32) -> Option<u32>;
    fn allocate_anywhere(&self, amount: u32) -> (SegmentId, u32);
    fn get_segment_mut(&self, id: u32) -> (*mut Word, u32);
    fn as_reader<'a>(&'a self) -> &'a ReaderArena;
}

pub struct BuilderArenaImplInner<A>
where
    A: Allocator,
{
    allocator: A,

    // TODO(perf): Try using smallvec to avoid heap allocations in the single-segment case?
    segments: Vec<(*mut Word, u32)>,
    allocated: Vec<u32>, // number of words allocated for each segment.
}

pub struct BuilderArenaImpl<A>
where
    A: Allocator,
{
    inner: RefCell<BuilderArenaImplInner<A>>,
}

impl<A> BuilderArenaImpl<A>
where
    A: Allocator,
{
    pub fn new(allocator: A) -> Self {
        BuilderArenaImpl {
            inner: RefCell::new(BuilderArenaImplInner {
                allocator: allocator,
                segments: Vec::new(),
                allocated: Vec::new(),
            }),
        }
    }

    pub fn allocate_segment(&self, minimum_size: u32) -> Result<()> {
        self.inner.borrow_mut().allocate_segment(minimum_size)
    }

    pub fn get_segments_for_output<'a>(&'a self) -> OutputSegments<'a> {
        let reff = self.inner.borrow();
        if reff.allocated.len() == 1 {
            let seg = reff.segments[0];

            // The user must mutably borrow the `message::Builder` to be able to modify segment memory.
            // No such borrow will be possible while `self` is still immutably borrowed from this method,
            // so returning this slice is safe.
            let slice =
                unsafe { slice::from_raw_parts(seg.0 as *const _, reff.allocated[0] as usize) };
            OutputSegments::SingleSegment([slice])
        } else {
            let mut v = Vec::with_capacity(reff.allocated.len());
            for idx in 0..reff.allocated.len() {
                let seg = reff.segments[idx];

                // See safety argument in above branch.
                let slice = unsafe {
                    slice::from_raw_parts(seg.0 as *const _, reff.allocated[idx] as usize)
                };
                v.push(slice);
            }
            OutputSegments::MultiSegment(v)
        }
    }

    pub fn len(&self) -> usize {
        self.inner.borrow().allocated.len()
    }
}

impl<A> ReaderArena for BuilderArenaImpl<A>
where
    A: Allocator,
{
    fn get_segment(&self, id: u32) -> Result<(*const Word, u32)> {
        let borrow = self.inner.borrow();
        let seg = borrow.segments[id as usize];
        Ok((seg.0 as *const _, seg.1))
    }

    fn check_offset(
        &self,
        _segment_id: u32,
        start: *const Word,
        offset_in_words: i32,
    ) -> Result<*const Word> {
        unsafe { Ok(start.offset(offset_in_words as isize)) }
    }

    fn contains_interval(&self, _id: u32, _start: *const Word, _size: usize) -> Result<()> {
        Ok(())
    }

    fn amplified_read(&self, _virtual_amount: u64) -> Result<()> {
        Ok(())
    }
}

impl<A> BuilderArenaImplInner<A>
where
    A: Allocator,
{
    fn allocate_segment(&mut self, minimum_size: WordCount32) -> Result<()> {
        let seg = self.allocator.allocate_segment(minimum_size);
        self.segments.push(seg);
        self.allocated.push(0);
        Ok(())
    }

    fn allocate(&mut self, segment_id: u32, amount: WordCount32) -> Option<u32> {
        if amount > self.get_segment_mut(segment_id).1 as u32 - self.allocated[segment_id as usize]
        {
            None
        } else {
            let result = self.allocated[segment_id as usize];
            self.allocated[segment_id as usize] += amount;
            Some(result)
        }
    }

    fn allocate_anywhere(&mut self, amount: u32) -> (SegmentId, u32) {
        // first try the existing segments, then try allocating a new segment.
        let allocated_len = self.allocated.len() as u32;
        for segment_id in 0..allocated_len {
            match self.allocate(segment_id, amount) {
                Some(idx) => return (segment_id, idx),
                None => (),
            }
        }

        // Need to allocate a new segment.

        self.allocate_segment(amount).expect("allocate new segment");
        (
            allocated_len,
            self.allocate(allocated_len, amount)
                .expect("use freshly-allocated segment"),
        )
    }

    fn get_segment_mut(&mut self, id: u32) -> (*mut Word, u32) {
        self.segments[id as usize]
    }
}

impl<A> BuilderArena for BuilderArenaImpl<A>
where
    A: Allocator,
{
    fn allocate(&self, segment_id: u32, amount: WordCount32) -> Option<u32> {
        self.inner.borrow_mut().allocate(segment_id, amount)
    }

    fn allocate_anywhere(&self, amount: u32) -> (SegmentId, u32) {
        self.inner.borrow_mut().allocate_anywhere(amount)
    }

    fn get_segment_mut(&self, id: u32) -> (*mut Word, u32) {
        self.inner.borrow_mut().get_segment_mut(id)
    }

    fn as_reader<'a>(&'a self) -> &'a ReaderArena {
        self
    }
}

impl<A> Drop for BuilderArenaImplInner<A>
where
    A: Allocator,
{
    fn drop(&mut self) {
        if self.allocated.len() > 0 {
            self.allocator.pre_drop(self.allocated[0]);
        }
    }
}

pub struct NullArena;

impl ReaderArena for NullArena {
    fn get_segment(&self, _id: u32) -> Result<(*const Word, u32)> {
        Err(Error::failed(format!("tried to read from null arena")))
    }

    fn check_offset(
        &self,
        _segment_id: u32,
        start: *const Word,
        offset_in_words: i32,
    ) -> Result<*const Word> {
        unsafe { Ok(start.offset(offset_in_words as isize)) }
    }

    fn contains_interval(&self, _id: u32, _start: *const Word, _size: usize) -> Result<()> {
        Ok(())
    }

    fn amplified_read(&self, _virtual_amount: u64) -> Result<()> {
        Ok(())
    }
}

impl BuilderArena for NullArena {
    fn allocate(&self, _segment_id: u32, _amount: WordCount32) -> Option<u32> {
        None
    }

    fn allocate_anywhere(&self, _amount: u32) -> (SegmentId, u32) {
        panic!("tried to allocate from a null arena")
    }

    fn get_segment_mut(&self, _id: u32) -> (*mut Word, u32) {
        (::std::ptr::null_mut(), 0)
    }

    fn as_reader<'a>(&'a self) -> &'a ReaderArena {
        self
    }
}
