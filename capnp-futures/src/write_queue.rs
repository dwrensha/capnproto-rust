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

use futures_channel::oneshot;
use futures_core::future::Future;
use futures_io::AsyncWrite;
use futures_util::{AsyncWriteExt, StreamExt, TryFutureExt};

use capnp::Error;

use crate::serialize::AsOutputSegments;

enum Item<M>
where
    M: AsOutputSegments,
{
    Message(M, oneshot::Sender<M>),
    Done(Result<(), Error>, oneshot::Sender<()>),
}
/// A handle that allows message to be sent to a write queue`.
pub struct Sender<M>
where
    M: AsOutputSegments,
{
    sender: futures_channel::mpsc::UnboundedSender<Item<M>>,
}

impl<M> Clone for Sender<M>
where
    M: AsOutputSegments,
{
    fn clone(&self) -> Sender<M> {
        Sender {
            sender: self.sender.clone(),
        }
    }
}

/// Creates a new WriteQueue that wraps the given writer.
pub fn write_queue<W, M>(mut writer: W) -> (Sender<M>, impl Future<Output = Result<(), Error>>)
where
    W: AsyncWrite + Unpin,
    M: AsOutputSegments,
{
    let (tx, mut rx) = futures_channel::mpsc::unbounded();

    let sender = Sender { sender: tx };

    let queue = async move {
        while let Some(item) = rx.next().await {
            match item {
                Item::Message(m, returner) => {
                    crate::serialize::write_message(&mut writer, &m).await?;
                    writer.flush().await?;
                    let _ = returner.send(m);
                }
                Item::Done(r, finisher) => {
                    let _ = finisher.send(());
                    return r;
                }
            }
        }
        Ok(())
    };

    (sender, queue)
}

impl<M> Sender<M>
where
    M: AsOutputSegments,
{
    /// Enqueues a message to be written. The returned future resolves once the write
    /// has completed.
    pub fn send(&mut self, message: M) -> impl Future<Output = Result<M, Error>> + Unpin {
        let (complete, oneshot) = oneshot::channel();

        let _ = self.sender.unbounded_send(Item::Message(message, complete));

        oneshot.map_err(|oneshot::Canceled| Error::disconnected("WriteQueue has terminated".into()))
    }

    /// Returns the number of messages queued to be written, not including any in-progress write.
    pub fn len(&mut self) -> usize {
        unimplemented!()
    }

    /// Commands the queue to stop writing messages once it is empty. After this method has been called,
    /// any new calls to `send()` will return a future that immediately resolves to an error.
    /// If the passed-in `result` is an error, then the `WriteQueue` will resolve to that error.
    pub fn terminate(
        &mut self,
        result: Result<(), Error>,
    ) -> impl Future<Output = Result<(), Error>> + Unpin {
        let (complete, receiver) = oneshot::channel();

        let _ = self.sender.unbounded_send(Item::Done(result, complete));

        receiver
            .map_err(|oneshot::Canceled| Error::disconnected("WriteQueue has terminated".into()))
    }
}
