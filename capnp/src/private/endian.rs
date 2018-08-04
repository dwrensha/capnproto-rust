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

use std::mem;

/// A value casted directly from a little-endian byte buffer. On big-endian
/// processors, the bytes of the value need to be swapped upon reading and writing.
#[repr(C)]
pub struct WireValue<T> {
    value: T,
}

impl<T> WireValue<T>
where
    T: Endian,
{
    /// Reads the value, swapping bytes on big-endian processors.
    #[inline]
    pub fn get(&self) -> T {
        self.value.get()
    }

    /// Writes the value, swapping bytes on big-endian processors.
    #[inline]
    pub fn set(&mut self, value: T) {
        self.value.set(value)
    }
}

/// Something that can appear in a `WireValue`.
pub trait Endian: Sized {
    /// Reads the value, swapping bytes on big-endian processors.
    fn get(&self) -> Self;

    /// Writes the value, swapping bytes on big-endian processors.
    fn set(&mut self, value: Self);
}

macro_rules! endian_impl(
    ($typ:ty) => (
        impl Endian for $typ {
            #[inline]
            fn get(&self) -> $typ { *self }
            #[inline]
            fn set(&mut self, value: $typ) {*self = value;}
        }
        );
    ($typ:ty, $swapper:ident) => (
        impl Endian for $typ {
            #[inline]
            fn get(&self) -> $typ { self.$swapper() }
            #[inline]
            fn set(&mut self, value: $typ) {
                *self = value.$swapper();
            }
        }
        );
    );

// No swapping necessary for primitives of size less than one byte.
endian_impl!(());
endian_impl!(bool);
endian_impl!(u8);
endian_impl!(i8);

// Need to swap bytes for primitives larger than a byte.
endian_impl!(u16, to_le);
endian_impl!(i16, to_le);
endian_impl!(u32, to_le);
endian_impl!(i32, to_le);
endian_impl!(u64, to_le);
endian_impl!(i64, to_le);

impl Endian for f32 {
    fn get(&self) -> f32 {
        unsafe { mem::transmute(mem::transmute::<f32, u32>(*self).to_le()) }
    }
    fn set(&mut self, value: f32) {
        *self = unsafe { mem::transmute(mem::transmute::<f32, u32>(value).to_le()) };
    }
}

impl Endian for f64 {
    fn get(&self) -> f64 {
        unsafe { mem::transmute(mem::transmute::<f64, u64>(*self).to_le()) }
    }
    fn set(&mut self, value: f64) {
        *self = unsafe { mem::transmute(mem::transmute::<f64, u64>(value).to_le()) };
    }
}
