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

use futures_core::Future;
use std::pin::Pin;
use std::task::{Context, Poll};

pub struct AttachFuture<F, T>
where
    F: Future + Unpin,
{
    original_future: F,
    value: Option<T>,
}

impl<F, T> Unpin for AttachFuture<F, T> where F: Future + Unpin {}

impl<F, T> Future for AttachFuture<F, T>
where
    F: Future + Unpin,
{
    type Output = F::Output;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context) -> Poll<Self::Output> {
        let result = Pin::new(&mut self.original_future).poll(cx);
        if let Poll::Ready(_) = result {
            self.value.take();
        }
        result
    }
}

pub trait Attach: Future
where
    Self: Unpin,
{
    fn attach<T>(self, value: T) -> AttachFuture<Self, T>
    where
        Self: Sized,
    {
        AttachFuture {
            original_future: self,
            value: Some(value),
        }
    }
}

impl<F> Attach for F where F: Future + Unpin {}
