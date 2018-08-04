// Copyright (c) 2016 Sandstorm Development Group, Inc. and contributors
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

use futures::future::Future;
use futures::stream::Stream;
use futures::{Async, Poll};
use std::io;

use capnp::{message, Error};

#[must_use = "streams do nothing unless polled"]
pub struct ReadStream<R>
where
    R: io::Read,
{
    options: message::ReaderOptions,
    read: ::serialize::Read<R>,
}

impl<R> ReadStream<R>
where
    R: io::Read,
{
    pub fn new(reader: R, options: message::ReaderOptions) -> ReadStream<R> {
        ReadStream {
            read: ::serialize::read_message(reader, options),
            options: options,
        }
    }
}

impl<R> Stream for ReadStream<R>
where
    R: io::Read,
{
    type Item = message::Reader<::serialize::OwnedSegments>;
    type Error = Error;

    fn poll(&mut self) -> Poll<Option<Self::Item>, Error> {
        let (r, m) = try_ready!(Future::poll(&mut self.read));
        self.read = ::serialize::read_message(r, self.options);
        Ok(Async::Ready(m))
    }
}
