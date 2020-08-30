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

use futures_core::future::Future;
use futures_io::AsyncRead;
use futures_util::stream::Stream;
use std::pin::Pin;
use std::task::{Context, Poll};

use capnp::{message, Error};

async fn read_next_message<R>(
    mut reader: R,
    options: message::ReaderOptions,
) -> Result<(R, Option<message::Reader<capnp::serialize::OwnedSegments>>), Error>
where
    R: AsyncRead + Unpin,
{
    let m = crate::serialize::read_message(&mut reader, options).await?;
    Ok((reader, m))
}

#[must_use = "streams do nothing unless polled"]
pub struct ReadStream<'a, R>
where
    R: AsyncRead + Unpin,
{
    options: message::ReaderOptions,
    read: Pin<
        Box<
            dyn Future<
                    Output = Result<
                        (R, Option<message::Reader<capnp::serialize::OwnedSegments>>),
                        Error,
                    >,
                > + 'a,
        >,
    >,
}

impl<'a, R> Unpin for ReadStream<'a, R> where R: AsyncRead + Unpin {}

impl<'a, R> ReadStream<'a, R>
where
    R: AsyncRead + Unpin + 'a,
{
    pub fn new(reader: R, options: message::ReaderOptions) -> Self {
        ReadStream {
            read: Box::pin(read_next_message(reader, options)),
            options: options,
        }
    }
}

impl<'a, R> Stream for ReadStream<'a, R>
where
    R: AsyncRead + Unpin + 'a,
{
    type Item = Result<message::Reader<capnp::serialize::OwnedSegments>, Error>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context) -> Poll<Option<Self::Item>> {
        let (r, m) = match Future::poll(self.read.as_mut(), cx) {
            Poll::Pending => return Poll::Pending,
            Poll::Ready(Err(e)) => return Poll::Ready(Some(Err(e))),
            Poll::Ready(Ok(x)) => x,
        };
        self.read = Box::pin(read_next_message(r, self.options));
        match m {
            Some(message) => Poll::Ready(Some(Ok(message))),
            None => Poll::Ready(None),
        }
    }
}
