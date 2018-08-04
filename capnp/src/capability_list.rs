// Copyright (c) 2017 David Renshaw and contributors
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

//! List of capabilities.

use std::marker::PhantomData;

use capability::FromClientHook;
use private::capability::ClientHook;
use private::layout::{ListBuilder, ListReader, Pointer, PointerBuilder, PointerReader};
use traits::{FromPointerBuilder, FromPointerReader, IndexMove, ListIter};
use Result;

#[derive(Copy, Clone)]
pub struct Owned<T>
where
    T: FromClientHook,
{
    marker: PhantomData<T>,
}

impl<'a, T> ::traits::Owned<'a> for Owned<T>
where
    T: FromClientHook,
{
    type Reader = Reader<'a, T>;
    type Builder = Builder<'a, T>;
}

pub struct Reader<'a, T>
where
    T: FromClientHook,
{
    marker: PhantomData<T>,
    reader: ListReader<'a>,
}

impl<'a, T> Clone for Reader<'a, T>
where
    T: FromClientHook,
{
    fn clone(&self) -> Reader<'a, T> {
        Reader {
            marker: self.marker,
            reader: self.reader,
        }
    }
}
impl<'a, T> Copy for Reader<'a, T> where T: FromClientHook {}

impl<'a, T> Reader<'a, T>
where
    T: FromClientHook,
{
    pub fn new<'b>(reader: ListReader<'b>) -> Reader<'b, T> {
        Reader::<'b, T> {
            reader,
            marker: PhantomData,
        }
    }

    pub fn len(&self) -> u32 {
        self.reader.len()
    }

    pub fn iter(self) -> ListIter<Reader<'a, T>, Result<T>> {
        ListIter::new(self, self.len())
    }
}

impl<'a, T> Reader<'a, T>
where
    T: FromClientHook,
{
    #[deprecated(since = "0.8.17", note = "use reborrow() instead")]
    pub fn borrow<'b>(&'b self) -> Reader<'b, T> {
        Reader {
            reader: self.reader,
            marker: PhantomData,
        }
    }

    pub fn reborrow<'b>(&'b self) -> Reader<'b, T> {
        Reader {
            reader: self.reader,
            marker: PhantomData,
        }
    }
}

impl<'a, T> FromPointerReader<'a> for Reader<'a, T>
where
    T: FromClientHook,
{
    fn get_from_pointer(reader: &PointerReader<'a>) -> Result<Reader<'a, T>> {
        Ok(Reader {
            reader: try!(reader.get_list(Pointer, ::std::ptr::null())),
            marker: PhantomData,
        })
    }
}

impl<'a, T> Reader<'a, T>
where
    T: FromClientHook,
{
    pub fn get(self, index: u32) -> Result<T> {
        assert!(index < self.len());
        Ok(FromClientHook::new(try!(
            self.reader.get_pointer_element(index).get_capability()
        )))
    }
}

impl<'a, T> IndexMove<u32, Result<T>> for Reader<'a, T>
where
    T: FromClientHook,
{
    fn index_move(&self, index: u32) -> Result<T> {
        self.get(index)
    }
}

pub struct Builder<'a, T>
where
    T: FromClientHook,
{
    marker: PhantomData<T>,
    builder: ListBuilder<'a>,
}

impl<'a, T> Builder<'a, T>
where
    T: FromClientHook,
{
    pub fn new(builder: ListBuilder<'a>) -> Builder<'a, T> {
        Builder {
            builder: builder,
            marker: PhantomData,
        }
    }

    pub fn len(&self) -> u32 {
        self.builder.len()
    }

    pub fn as_reader(self) -> Reader<'a, T> {
        Reader {
            marker: PhantomData,
            reader: self.builder.as_reader(),
        }
    }

    pub fn set(&mut self, index: u32, value: Box<ClientHook>) {
        assert!(index < self.len());
        self.builder
            .borrow()
            .get_pointer_element(index)
            .set_capability(value);
    }
}

impl<'a, T> Builder<'a, T>
where
    T: FromClientHook,
{
    #[deprecated(since = "0.8.17", note = "use reborrow() instead")]
    pub fn borrow<'b>(&'b mut self) -> Builder<'b, T> {
        Builder {
            builder: self.builder,
            marker: PhantomData,
        }
    }

    pub fn reborrow<'b>(&'b mut self) -> Builder<'b, T> {
        Builder {
            builder: self.builder,
            marker: PhantomData,
        }
    }
}

impl<'a, T> FromPointerBuilder<'a> for Builder<'a, T>
where
    T: FromClientHook,
{
    fn init_pointer(builder: PointerBuilder<'a>, size: u32) -> Builder<'a, T> {
        Builder {
            marker: PhantomData,
            builder: builder.init_list(Pointer, size),
        }
    }
    fn get_from_pointer(builder: PointerBuilder<'a>) -> Result<Builder<'a, T>> {
        Ok(Builder {
            marker: PhantomData,
            builder: try!(builder.get_list(Pointer, ::std::ptr::null())),
        })
    }
}

impl<'a, T> Builder<'a, T>
where
    T: FromClientHook,
{
    pub fn get(self, index: u32) -> Result<T> {
        assert!(index < self.len());
        Ok(FromClientHook::new(try!(
            self.builder.get_pointer_element(index).get_capability()
        )))
    }
}

impl<'a, T> ::traits::SetPointerBuilder<Builder<'a, T>> for Reader<'a, T>
where
    T: FromClientHook,
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
    T: FromClientHook,
{
    type Item = Result<T>;
    type IntoIter = ListIter<Reader<'a, T>, Self::Item>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}
