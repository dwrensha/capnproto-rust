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

//! List of lists.

use private::layout::{ListBuilder, ListReader, Pointer, PointerBuilder, PointerReader};
use traits::{FromPointerBuilder, FromPointerReader, IndexMove, ListIter};
use Result;

#[derive(Clone, Copy)]
pub struct Owned<T>
where
    T: for<'a> ::traits::Owned<'a>,
{
    marker: ::std::marker::PhantomData<<T as ::traits::Owned<'static>>::Reader>,
}

impl<'a, T> ::traits::Owned<'a> for Owned<T>
where
    T: for<'b> ::traits::Owned<'b>,
{
    type Reader = Reader<'a, T>;
    type Builder = Builder<'a, T>;
}

pub struct Reader<'a, T>
where
    T: for<'b> ::traits::Owned<'b>,
{
    marker: ::std::marker::PhantomData<<T as ::traits::Owned<'a>>::Reader>,
    reader: ListReader<'a>,
}

impl<'a, T> Reader<'a, T>
where
    T: for<'b> ::traits::Owned<'b>,
{
    pub fn new<'b>(reader: ListReader<'b>) -> Reader<'b, T> {
        Reader::<'b, T> {
            reader: reader,
            marker: ::std::marker::PhantomData,
        }
    }

    pub fn len(&self) -> u32 {
        self.reader.len()
    }
    pub fn iter(self) -> ListIter<Reader<'a, T>, Result<<T as ::traits::Owned<'a>>::Reader>> {
        ListIter::new(self, self.len())
    }
}

impl<'a, T> Clone for Reader<'a, T>
where
    T: for<'b> ::traits::Owned<'b>,
{
    fn clone(&self) -> Reader<'a, T> {
        Reader {
            marker: self.marker,
            reader: self.reader,
        }
    }
}

impl<'a, T> Copy for Reader<'a, T> where T: for<'b> ::traits::Owned<'b> {}

impl<'a, T> IndexMove<u32, Result<<T as ::traits::Owned<'a>>::Reader>> for Reader<'a, T>
where
    T: for<'b> ::traits::Owned<'b>,
{
    fn index_move(&self, index: u32) -> Result<<T as ::traits::Owned<'a>>::Reader> {
        self.get(index)
    }
}

impl<'a, T> FromPointerReader<'a> for Reader<'a, T>
where
    T: for<'b> ::traits::Owned<'b>,
{
    fn get_from_pointer(reader: &PointerReader<'a>) -> Result<Reader<'a, T>> {
        Ok(Reader {
            reader: try!(reader.get_list(Pointer, ::std::ptr::null())),
            marker: ::std::marker::PhantomData,
        })
    }
}

impl<'a, T> Reader<'a, T>
where
    T: for<'b> ::traits::Owned<'b>,
{
    pub fn get(self, index: u32) -> Result<<T as ::traits::Owned<'a>>::Reader> {
        assert!(index < self.len());
        FromPointerReader::get_from_pointer(&self.reader.get_pointer_element(index))
    }
}

pub struct Builder<'a, T>
where
    T: for<'b> ::traits::Owned<'b>,
{
    marker: ::std::marker::PhantomData<T>,
    builder: ListBuilder<'a>,
}

impl<'a, T> Builder<'a, T>
where
    T: for<'b> ::traits::Owned<'b>,
{
    pub fn new(builder: ListBuilder<'a>) -> Builder<'a, T> {
        Builder {
            builder: builder,
            marker: ::std::marker::PhantomData,
        }
    }

    pub fn len(&self) -> u32 {
        self.builder.len()
    }

    pub fn as_reader(self) -> Reader<'a, T> {
        Reader {
            reader: self.builder.as_reader(),
            marker: ::std::marker::PhantomData,
        }
    }
}

impl<'a, T> Builder<'a, T>
where
    T: for<'b> ::traits::Owned<'b>,
{
    pub fn init(self, index: u32, size: u32) -> <T as ::traits::Owned<'a>>::Builder {
        FromPointerBuilder::init_pointer(self.builder.get_pointer_element(index), size)
    }
}

impl<'a, T> Builder<'a, T>
where
    T: for<'b> ::traits::Owned<'b>,
{
    #[deprecated(since = "0.8.17", note = "use reborrow() instead")]
    pub fn borrow<'b>(&'b mut self) -> Builder<'b, T> {
        Builder {
            builder: self.builder.borrow(),
            marker: ::std::marker::PhantomData,
        }
    }

    pub fn reborrow<'b>(&'b mut self) -> Builder<'b, T> {
        Builder {
            builder: self.builder.borrow(),
            marker: ::std::marker::PhantomData,
        }
    }
}

impl<'a, T> FromPointerBuilder<'a> for Builder<'a, T>
where
    T: for<'b> ::traits::Owned<'b>,
{
    fn init_pointer(builder: PointerBuilder<'a>, size: u32) -> Builder<'a, T> {
        Builder {
            marker: ::std::marker::PhantomData,
            builder: builder.init_list(Pointer, size),
        }
    }
    fn get_from_pointer(builder: PointerBuilder<'a>) -> Result<Builder<'a, T>> {
        Ok(Builder {
            marker: ::std::marker::PhantomData,
            builder: try!(builder.get_list(Pointer, ::std::ptr::null())),
        })
    }
}

impl<'a, T> Builder<'a, T>
where
    T: for<'b> ::traits::Owned<'b>,
{
    pub fn get(self, index: u32) -> Result<<T as ::traits::Owned<'a>>::Builder> {
        assert!(index < self.len());
        FromPointerBuilder::get_from_pointer(self.builder.get_pointer_element(index))
    }
}

impl<'a, T> ::traits::SetPointerBuilder<Builder<'a, T>> for Reader<'a, T>
where
    T: for<'b> ::traits::Owned<'b>,
{
    fn set_pointer_builder<'b>(
        pointer: ::private::layout::PointerBuilder<'b>,
        value: Reader<'a, T>,
        canonicalize: bool,
    ) -> Result<()> {
        pointer.set_list(&value.reader, canonicalize)
    }
}

impl<'a, T> ::std::iter::IntoIterator for Reader<'a, T>
where
    T: for<'b> ::traits::Owned<'b>,
{
    type Item = Result<<T as ::traits::Owned<'a>>::Reader>;
    type IntoIter = ListIter<Reader<'a, T>, Self::Item>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}
