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

//! Helper type for generated Struct and List constants.
//!
//! `constant::Reader` does not do bounds-checking, so it is unsafe to
//! manually construct one of these.

use std::marker::PhantomData;

use any_pointer;
use private::layout::PointerReader;
use traits::Owned;
use {Result, Word};

#[derive(Copy, Clone)]
pub struct Reader<T> {
    #[doc(hidden)]
    pub phantom: PhantomData<T>,

    #[doc(hidden)]
    pub words: &'static [Word],
}

impl<T> Reader<T>
where
    T: for<'a> Owned<'a>,
{
    /// Retrieve the value.
    pub fn get(&self) -> Result<<T as Owned<'static>>::Reader> {
        any_pointer::Reader::new(PointerReader::get_root_unchecked(
            &self.words[0] as *const Word,
        )).get_as()
    }
}
