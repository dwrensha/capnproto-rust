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

use crate::{Result};
use crate::private::layout::{CapTable, ListReader, StructReader, StructBuilder, StructSize,
                             PointerBuilder, PointerReader};

use core::marker::PhantomData;

pub trait FromStructReader<'a> {
    fn new(reader: StructReader<'a>) -> Self;
}

pub trait HasStructSize {
    const STRUCT_SIZE: StructSize;
}

pub trait IntoInternalStructReader<'a> {
    fn into_internal_struct_reader(self) -> StructReader<'a>;
}

pub trait FromStructBuilder<'a> {
    fn new(struct_builder: StructBuilder<'a>) -> Self;
}

pub trait IntoInternalListReader<'a> {
    fn into_internal_list_reader(self) -> ListReader<'a>;
}

pub trait FromPointerReader<'a> : Sized {
    fn get_from_pointer(reader: &PointerReader<'a>, default: Option<&'a [crate::Word]>) -> Result<Self>;
}

/// Associated types hackery that allows us to reason about Cap'n Proto types
/// without needing to give them a lifetime `'a`.
///
/// If `Foo` is a Cap'n Proto struct and `Bar` is a Rust-native struct, then
/// `foo::Reader<'a>` is to `foo::Owned` as `&'a Bar` is to `Bar`, and
/// `foo::Builder<'a>` is to `foo::Owned` as `&'a mut Bar` is to `Bar`.
/// The relationship is formalized by an `impl <'a> capnp::traits::Owned<'a> for foo::Owned`.
/// Because Cap'n Proto struct layout differs from Rust struct layout, a `foo::Owned` value
/// cannot be used for anything interesting on its own; the `foo::Owned` type is useful
/// nonetheless as a type parameter, e.g. for a generic container that owns a Cap'n Proto
/// message of type `T: for<'a> capnp::traits::Owned<'a>`.
pub trait Owned<'a> {
    type Reader: FromPointerReader<'a> + SetPointerBuilder;
    type Builder: FromPointerBuilder<'a>;
}

pub trait OwnedStruct<'a> {
    type Reader: FromStructReader<'a> + SetPointerBuilder + IntoInternalStructReader<'a>;
    type Builder: FromStructBuilder<'a> + HasStructSize;
}

pub trait Pipelined {
    type Pipeline;
}

pub trait FromPointerBuilder<'a> : Sized {
    fn init_pointer(builder: PointerBuilder<'a>, length: u32) -> Self;
    fn get_from_pointer(builder: PointerBuilder<'a>, default: Option<&'a [crate::Word]>) -> Result<Self>;
}

pub trait SetPointerBuilder {
    fn set_pointer_builder<'a>(builder: PointerBuilder<'a>, from: Self, canonicalize: bool) -> Result<()>;
}

pub trait Imbue<'a> {
    fn imbue(&mut self, caps: &'a CapTable);
}

pub trait ImbueMut<'a> {
    fn imbue_mut(&mut self, caps: &'a mut CapTable);
}

pub trait HasTypeId {
    fn type_id() -> u64;
}

pub trait ToU16 {
    fn to_u16(self) -> u16;
}

pub trait FromU16 : Sized {
    fn from_u16(value: u16) -> ::core::result::Result<Self, crate::NotInSchema>;
}

pub trait IndexMove<I, T> {
    fn index_move(&self, index: I) -> T;
}

pub struct ListIter<T, U> {
    marker: PhantomData<U>,
    list: T,
    index: u32,
    size: u32,
}

impl <T, U> ListIter<T, U>{
    pub fn new(list: T, size: u32) -> ListIter<T, U> {
        ListIter { list: list, index: 0, size: size, marker: PhantomData }
    }
}

impl <U, T : IndexMove<u32, U>> ::core::iter::Iterator for ListIter<T, U> {
    type Item = U;
    fn next(&mut self) -> ::core::option::Option<U> {
        if self.index < self.size {
            let result = self.list.index_move(self.index);
            self.index += 1;
            Some(result)
        } else {
            None
        }
    }

    fn size_hint(&self) -> (usize, Option<usize>){
        (self.size as usize, Some(self.size as usize))
    }

    fn nth(&mut self, p: usize) -> Option<U>{
        if self.index + (p as u32) < self.size  {
            self.index += p as u32;
            let result = self.list.index_move(self.index);
            self.index += 1;
            Some(result)
        } else {
            self.index = self.size;
            None
        }
    }
}

impl <U, T: IndexMove<u32, U>> ::core::iter::ExactSizeIterator for ListIter<T, U>{
    fn len(&self) -> usize{
        self.size as usize
    }
}

impl <U, T: IndexMove<u32, U>> ::core::iter::DoubleEndedIterator for ListIter<T, U>{
    fn next_back(&mut self) -> ::core::option::Option<U> {
        if self.size > self.index {
            self.size -= 1;
            Some(self.list.index_move(self.size))
        } else {
            None
        }
    }
}
