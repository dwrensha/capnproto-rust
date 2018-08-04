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

//! List of primitives.

use std::{marker, ptr};

use private::layout::{ListBuilder, ListReader, PointerBuilder, PointerReader, PrimitiveElement};
use traits::{FromPointerBuilder, FromPointerReader, IndexMove, ListIter};
use Result;

#[derive(Clone, Copy)]
pub struct Owned<T> {
    marker: marker::PhantomData<T>,
}

impl<'a, T> ::traits::Owned<'a> for Owned<T>
where
    T: PrimitiveElement,
{
    type Reader = Reader<'a, T>;
    type Builder = Builder<'a, T>;
}

#[derive(Clone, Copy)]
pub struct Reader<'a, T>
where
    T: PrimitiveElement,
{
    marker: marker::PhantomData<T>,
    reader: ListReader<'a>,
}

impl<'a, T: PrimitiveElement> Reader<'a, T> {
    pub fn new<'b>(reader: ListReader<'b>) -> Reader<'b, T> {
        Reader::<'b, T> {
            reader: reader,
            marker: marker::PhantomData,
        }
    }

    pub fn len(&self) -> u32 {
        self.reader.len()
    }

    pub fn iter(self) -> ListIter<Reader<'a, T>, T> {
        let l = self.len();
        ListIter::new(self, l)
    }
}

impl<'a, T: PrimitiveElement> FromPointerReader<'a> for Reader<'a, T> {
    fn get_from_pointer(reader: &PointerReader<'a>) -> Result<Reader<'a, T>> {
        Ok(Reader {
            reader: try!(reader.get_list(T::element_size(), ptr::null())),
            marker: marker::PhantomData,
        })
    }
}

impl<'a, T: PrimitiveElement> IndexMove<u32, T> for Reader<'a, T> {
    fn index_move(&self, index: u32) -> T {
        self.get(index)
    }
}

impl<'a, T: PrimitiveElement> Reader<'a, T> {
    pub fn get(&self, index: u32) -> T {
        assert!(index < self.len());
        PrimitiveElement::get(&self.reader, index)
    }
}

pub struct Builder<'a, T>
where
    T: PrimitiveElement,
{
    marker: marker::PhantomData<T>,
    builder: ListBuilder<'a>,
}

impl<'a, T> Builder<'a, T>
where
    T: PrimitiveElement,
{
    pub fn new(builder: ListBuilder<'a>) -> Builder<'a, T> {
        Builder {
            builder: builder,
            marker: marker::PhantomData,
        }
    }

    pub fn len(&self) -> u32 {
        self.builder.len()
    }

    pub fn as_reader(self) -> Reader<'a, T> {
        Reader {
            marker: marker::PhantomData,
            reader: self.builder.as_reader(),
        }
    }

    pub fn set(&mut self, index: u32, value: T) {
        PrimitiveElement::set(&self.builder, index, value);
    }
}

impl<'a, T: PrimitiveElement> FromPointerBuilder<'a> for Builder<'a, T> {
    fn init_pointer(builder: PointerBuilder<'a>, size: u32) -> Builder<'a, T> {
        Builder {
            builder: builder.init_list(T::element_size(), size),
            marker: marker::PhantomData,
        }
    }
    fn get_from_pointer(builder: PointerBuilder<'a>) -> Result<Builder<'a, T>> {
        Ok(Builder {
            builder: try!(builder.get_list(T::element_size(), ptr::null())),
            marker: marker::PhantomData,
        })
    }
}

impl<'a, T: PrimitiveElement> Builder<'a, T> {
    pub fn get(&self, index: u32) -> T {
        assert!(index < self.len());
        PrimitiveElement::get_from_builder(&self.builder, index)
    }

    pub fn reborrow<'b>(&'b self) -> Builder<'b, T> {
        Builder { ..*self }
    }
}

impl<'a, T> ::traits::SetPointerBuilder<Builder<'a, T>> for Reader<'a, T>
where
    T: PrimitiveElement,
{
    fn set_pointer_builder<'b>(
        pointer: PointerBuilder<'b>,
        value: Reader<'a, T>,
        canonicalize: bool,
    ) -> Result<()> {
        pointer.set_list(&value.reader, canonicalize)
    }
}

impl<'a, T> ::std::iter::IntoIterator for Reader<'a, T>
where
    T: PrimitiveElement,
{
    type Item = T;
    type IntoIter = ListIter<Reader<'a, T>, Self::Item>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}
