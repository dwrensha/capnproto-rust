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

//! Sequence of bytes.

use private::layout::{PointerBuilder, PointerReader};
use Result;

#[derive(Copy, Clone)]
pub struct Owned(());

impl<'a> ::traits::Owned<'a> for Owned {
    type Reader = Reader<'a>;
    type Builder = Builder<'a>;
}

pub type Reader<'a> = &'a [u8];

pub fn new_reader<'a>(p: *const u8, len: u32) -> Reader<'a> {
    unsafe { ::std::slice::from_raw_parts(p, len as usize) }
}

impl<'a> ::traits::FromPointerReader<'a> for Reader<'a> {
    fn get_from_pointer(reader: &PointerReader<'a>) -> Result<Reader<'a>> {
        reader.get_data(::std::ptr::null(), 0)
    }
}

pub type Builder<'a> = &'a mut [u8];

pub fn new_builder<'a>(p: *mut u8, len: u32) -> Builder<'a> {
    unsafe { ::std::slice::from_raw_parts_mut(p, len as usize) }
}

impl<'a> ::traits::FromPointerBuilder<'a> for Builder<'a> {
    fn init_pointer(builder: PointerBuilder<'a>, size: u32) -> Builder<'a> {
        builder.init_data(size)
    }
    fn get_from_pointer(builder: PointerBuilder<'a>) -> Result<Builder<'a>> {
        builder.get_data(::std::ptr::null(), 0)
    }
}

impl<'a> ::traits::SetPointerBuilder<Builder<'a>> for Reader<'a> {
    fn set_pointer_builder<'b>(
        pointer: PointerBuilder<'b>,
        value: Reader<'a>,
        _canonicalize: bool,
    ) -> Result<()> {
        pointer.set_data(value);
        Ok(())
    }
}
