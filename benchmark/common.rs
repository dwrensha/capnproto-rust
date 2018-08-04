// Copyright (c) 2013-2014 Sandstorm Development Group, Inc. and contributors
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

use rand::*;
use std::i32;

#[derive(Clone, Copy)]
pub struct FastRand {
    x: u32,
    y: u32,
    z: u32,
    w: u32,
}

impl Rng for FastRand {
    #[inline]
    fn next_u32(&mut self) -> u32 {
        let tmp = self.x ^ (self.x << 11);
        self.x = self.y;
        self.y = self.z;
        self.z = self.w;
        self.w = self.w ^ (self.w >> 19) ^ tmp ^ (tmp >> 8);
        return self.w;
    }
}

impl FastRand {
    pub fn new() -> FastRand {
        FastRand {
            x: 0x1d2acd47,
            y: 0x58ca3e14,
            z: 0xf563f232,
            w: 0x0bc76199,
        }
    }

    #[inline]
    pub fn next_less_than(&mut self, range: u32) -> u32 {
        self.next_u32() % range
    }

    #[inline]
    pub fn next_double(&mut self, range: f64) -> f64 {
        use std::u32;
        self.next_u32() as f64 * range / (u32::MAX as f64)
    }
}

#[inline]
pub fn div(a: i32, b: i32) -> i32 {
    if b == 0 {
        return i32::MAX;
    }
    if a == i32::MIN && b == -1 {
        return i32::MAX;
    }
    return a / b;
}

#[inline]
pub fn modulus(a: i32, b: i32) -> i32 {
    if b == 0 {
        return i32::MAX;
    }
    if a == i32::MIN && b == -1 {
        return i32::MAX;
    }
    return a % b;
}

pub const WORDS: [&'static str; 13] = [
    "foo ", "bar ", "baz ", "qux ", "quux ", "corge ", "grault ", "garply ", "waldo ", "fred ",
    "plugh ", "xyzzy ", "thud ",
];
