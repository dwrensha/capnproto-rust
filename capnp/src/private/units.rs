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

pub type BitCount0 = usize; // `BitCount` clashes with a standard trait
pub type BitCount8 = u8;
pub type BitCount16 = u16;
pub type BitCount32 = u32;
pub type BitCount64 = u64;

pub type ByteCount = usize;
pub type ByteCount8 = u8;
pub type ByteCount16 = u16;
pub type ByteCount32 = u32;
pub type ByteCount64 = u64;

pub type WordCount = usize;
pub type WordCount8 = u8;
pub type WordCount16 = u16;
pub type WordCount32 = u32;
pub type WordCount64 = u64;

pub type ElementCount = usize;
pub type ElementCount8 = u8;
pub type ElementCount16 = u16;
pub type ElementCount32 = u32;
pub type ElementCount64 = u64;

pub type WirePointerCount = usize;
pub type WirePointerCount8 = u8;
pub type WirePointerCount16 = u16;
pub type WirePointerCount32 = u32;
pub type WirePointerCount64 = u64;

pub const BITS_PER_BYTE: BitCount0 = 8;
pub const BITS_PER_WORD: BitCount0 = 64;
pub const BYTES_PER_WORD: ByteCount = 8;

pub const BITS_PER_POINTER: BitCount0 = 64;
pub const _BYTES_PER_POINTER: ByteCount = 8;
pub const WORDS_PER_POINTER: WordCount = 1;

pub const POINTER_SIZE_IN_WORDS: WordCount = 1;

pub fn _bytes_per_element<T>() -> ByteCount {
    ::std::mem::size_of::<T>()
}

pub fn bits_per_element<T>() -> BitCount0 {
    8 * ::std::mem::size_of::<T>()
}
