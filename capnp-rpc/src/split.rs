// Copyright (c) 2013-2016 Sandstorm Development Group, Inc. and contributors
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

use futures_core::Future;
use futures_util::FutureExt;

use std::cell::RefCell;
use std::rc::Rc;

pub fn split<F, T1, T2, E>(
    f: F,
) -> (
    impl Future<Output = Result<T1, E>>,
    impl Future<Output = Result<T2, E>>,
)
where
    F: Future<Output = Result<(T1, T2), E>>,
    E: Clone,
{
    let shared = f
        .map(|r| {
            let (v1, v2) = r?;
            Ok(Rc::new(RefCell::new((Some(v1), Some(v2)))))
        })
        .shared();
    (
        shared
            .clone()
            .map(|r| Ok::<T1, E>(r?.borrow_mut().0.take().unwrap())),
        shared.map(|r| Ok::<T2, E>(r?.borrow_mut().1.take().unwrap())),
    )
}
