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

//! List of structs.

use core::marker::PhantomData;

use crate::private::layout::{ListReader, ListBuilder, PointerReader, PointerBuilder, InlineComposite};
use crate::traits::{FromPointerReader, FromPointerBuilder,
                    FromStructBuilder, FromStructReader, HasStructSize,
                    IndexMove, ListIter};
use crate::Result;

#[derive(Copy, Clone)]
pub struct Owned<T> where T: for<'a> crate::traits::OwnedStruct<'a> {
    marker: PhantomData<T>,
}

impl<'a, T> crate::traits::Owned<'a> for Owned<T> where T: for<'b> crate::traits::OwnedStruct<'b> {
    type Reader = Reader<'a, T>;
    type Builder = Builder<'a, T>;
}

pub struct Reader<'a, T> where T: for<'b> crate::traits::OwnedStruct<'b> {
    marker: PhantomData<T>,
    reader: ListReader<'a>
}

impl <'a, T> Clone for Reader<'a, T> where T: for<'b> crate::traits::OwnedStruct<'b> {
    fn clone(&self) -> Reader<'a, T> {
        Reader { marker : self.marker, reader : self.reader }
    }
}
impl <'a, T> Copy for Reader<'a, T> where T: for<'b> crate::traits::OwnedStruct<'b> {}

impl <'a, T> Reader<'a, T> where T: for<'b> crate::traits::OwnedStruct<'b> {
    pub fn len(&self) -> u32 { self.reader.len() }

    pub fn iter(self) -> ListIter<Reader<'a, T>, <T as crate::traits::OwnedStruct<'a>>::Reader> {
        ListIter::new(self, self.len())
    }
}

impl <'a, T> Reader<'a, T> where T: for<'b> crate::traits::OwnedStruct<'b> {
    pub fn reborrow<'b>(&'b self) -> Reader<'b, T>  {
        Reader { reader: self.reader, marker: PhantomData }
    }
}

impl <'a, T> FromPointerReader<'a> for Reader<'a, T> where T: for<'b> crate::traits::OwnedStruct<'b> {
    fn get_from_pointer(reader: &PointerReader<'a>, default: Option<&'a [crate::Word]>) -> Result<Reader<'a, T>> {
        Ok(Reader { reader: reader.get_list(InlineComposite, default)?,
                    marker: PhantomData })
    }
}

impl <'a, T>  IndexMove<u32, <T as crate::traits::OwnedStruct<'a>>::Reader> for Reader<'a, T>
where T: for<'b> crate::traits::OwnedStruct<'b> {
    fn index_move(&self, index: u32) -> <T as crate::traits::OwnedStruct<'a>>::Reader {
        self.get(index)
    }
}

impl <'a, T> Reader<'a, T> where T: for<'b> crate::traits::OwnedStruct<'b> {
    pub fn get(self, index: u32) -> <T as crate::traits::OwnedStruct<'a>>::Reader {
        assert!(index < self.len());
        FromStructReader::new(self.reader.get_struct_element(index))
    }
}

impl <'a, T> crate::traits::IntoInternalListReader<'a> for Reader<'a, T> where T: for<'b> crate::traits::OwnedStruct<'b> {
    fn into_internal_list_reader(self) -> ListReader<'a> {
        self.reader
    }
}

pub struct Builder<'a, T> where T: for<'b> crate::traits::OwnedStruct<'b> {
    marker: PhantomData<T>,
    builder: ListBuilder<'a>
}

impl <'a, T> Builder<'a, T> where T: for<'b> crate::traits::OwnedStruct<'b> {
    pub fn len(&self) -> u32 { self.builder.len() }

    pub fn into_reader(self) -> Reader<'a, T> {
        Reader {
            marker: PhantomData,
            reader: self.builder.into_reader(),
        }
    }

    /// Sets the list element, with the following limitation based on the fact that structs in a
    /// struct list are allocated inline: if the source struct is larger than the target struct
    /// (as can happen if it was created with a newer version of the schema), then it will be
    /// truncated, losing fields.
    pub fn set_with_caveats<'b>(&self, index: u32, value: <T as crate::traits::OwnedStruct<'b>>::Reader)
               -> Result<()>
        where <T as crate::traits::OwnedStruct<'b>>::Reader: crate::traits::IntoInternalStructReader<'b>
    {
        use crate::traits::IntoInternalStructReader;
        self.builder.get_struct_element(index).copy_content_from(&value.into_internal_struct_reader())
    }
}

impl <'a, T> Builder<'a, T> where T: for<'b> crate::traits::OwnedStruct<'b> {
    pub fn reborrow<'b>(&'b mut self) -> Builder<'b, T> {
        Builder { builder: self.builder, marker: PhantomData }
    }

}

impl <'a, T> FromPointerBuilder<'a> for Builder<'a, T> where T: for<'b> crate::traits::OwnedStruct<'b> {
    fn init_pointer(builder: PointerBuilder<'a>, size: u32) -> Builder<'a, T> {
        Builder {
            marker: PhantomData,
            builder: builder.init_struct_list(
                size,
                <<T as crate::traits::OwnedStruct>::Builder as HasStructSize>::STRUCT_SIZE)
        }
    }
    fn get_from_pointer(builder: PointerBuilder<'a>, default: Option<&'a [crate::Word]>) -> Result<Builder<'a, T>> {
        Ok(Builder {
            marker: PhantomData,
            builder:
            builder.get_struct_list(<<T as crate::traits::OwnedStruct>::Builder as HasStructSize>::STRUCT_SIZE,
                                    default)?
        })
    }
}

impl <'a, T> Builder<'a, T> where T: for<'b> crate::traits::OwnedStruct<'b> {
    pub fn get(self, index: u32) -> <T as crate::traits::OwnedStruct<'a>>::Builder {
        assert!(index < self.len());
        FromStructBuilder::new(self.builder.get_struct_element(index))
    }
}

impl <'a, T> crate::traits::SetPointerBuilder for Reader<'a, T>
    where T: for<'b> crate::traits::OwnedStruct<'b>
{
    fn set_pointer_builder<'b>(pointer: crate::private::layout::PointerBuilder<'b>,
                               value: Reader<'a, T>,
                               canonicalize: bool) -> Result<()> {
        pointer.set_list(&value.reader, canonicalize)
    }
}

impl <'a, T> ::core::iter::IntoIterator for Reader<'a, T>
    where T: for<'b> crate::traits::OwnedStruct<'b>
{
    type Item = <T as crate::traits::OwnedStruct<'a>>::Reader;
    type IntoIter = ListIter<Reader<'a, T>, Self::Item>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}
