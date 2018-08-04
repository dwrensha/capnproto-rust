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

//! List of sequences of bytes.

use private::layout::*;
use traits::{FromPointerBuilder, FromPointerReader, IndexMove, ListIter};
use Result;

#[derive(Copy, Clone)]
pub struct Owned;

impl<'a> ::traits::Owned<'a> for Owned {
    type Reader = Reader<'a>;
    type Builder = Builder<'a>;
}

#[derive(Clone, Copy)]
pub struct Reader<'a> {
    pub reader: ListReader<'a>,
}

impl<'a> Reader<'a> {
    pub fn new<'b>(reader: ListReader<'b>) -> Reader<'b> {
        Reader { reader: reader }
    }

    pub fn len(&self) -> u32 {
        self.reader.len()
    }

    pub fn iter(self) -> ListIter<Reader<'a>, Result<::data::Reader<'a>>> {
        let l = self.len();
        ListIter::new(self, l)
    }
}

impl<'a> FromPointerReader<'a> for Reader<'a> {
    fn get_from_pointer(reader: &PointerReader<'a>) -> Result<Reader<'a>> {
        Ok(Reader {
            reader: try!(reader.get_list(Pointer, ::std::ptr::null())),
        })
    }
}

impl<'a> IndexMove<u32, Result<::data::Reader<'a>>> for Reader<'a> {
    fn index_move(&self, index: u32) -> Result<::data::Reader<'a>> {
        self.get(index)
    }
}

impl<'a> Reader<'a> {
    pub fn get(self, index: u32) -> Result<::data::Reader<'a>> {
        assert!(index < self.len());
        self.reader
            .get_pointer_element(index)
            .get_data(::std::ptr::null(), 0)
    }
}

pub struct Builder<'a> {
    builder: ListBuilder<'a>,
}

impl<'a> Builder<'a> {
    pub fn new(builder: ListBuilder<'a>) -> Builder<'a> {
        Builder { builder: builder }
    }

    pub fn len(&self) -> u32 {
        self.builder.len()
    }

    pub fn as_reader(self) -> Reader<'a> {
        Reader {
            reader: self.builder.as_reader(),
        }
    }

    pub fn set(&mut self, index: u32, value: ::data::Reader) {
        assert!(index < self.len());
        self.builder
            .borrow()
            .get_pointer_element(index)
            .set_data(value);
    }

    #[deprecated(since = "0.8.17", note = "use reborrow() instead")]
    pub fn borrow<'b>(&'b mut self) -> Builder<'b> {
        Builder {
            builder: self.builder.borrow(),
        }
    }

    pub fn reborrow<'b>(&'b mut self) -> Builder<'b> {
        Builder {
            builder: self.builder.borrow(),
        }
    }
}

impl<'a> FromPointerBuilder<'a> for Builder<'a> {
    fn init_pointer(builder: PointerBuilder<'a>, size: u32) -> Builder<'a> {
        Builder {
            builder: builder.init_list(Pointer, size),
        }
    }

    fn get_from_pointer(builder: PointerBuilder<'a>) -> Result<Builder<'a>> {
        Ok(Builder {
            builder: try!(builder.get_list(Pointer, ::std::ptr::null())),
        })
    }
}

impl<'a> Builder<'a> {
    pub fn get(self, index: u32) -> Result<::data::Builder<'a>> {
        assert!(index < self.len());
        self.builder
            .get_pointer_element(index)
            .get_data(::std::ptr::null(), 0)
    }
}

impl<'a> ::traits::SetPointerBuilder<Builder<'a>> for Reader<'a> {
    fn set_pointer_builder<'b>(
        pointer: ::private::layout::PointerBuilder<'b>,
        value: Reader<'a>,
        canonicalize: bool,
    ) -> Result<()> {
        try!(pointer.set_list(&value.reader, canonicalize));
        Ok(())
    }
}

impl<'a> ::std::iter::IntoIterator for Reader<'a> {
    type Item = Result<::data::Reader<'a>>;
    type IntoIter = ListIter<Reader<'a>, Self::Item>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}
