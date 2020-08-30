// Copyright (c) 2015 Sandstorm Development Group, Inc. and contributors
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

//! An implementation of `VatNetwork` for the common case of a client-server connection.

use capnp::capability::Promise;
use capnp::message::ReaderOptions;
use futures_channel::oneshot;
use futures_io::{AsyncRead, AsyncWrite};
use futures_util::{FutureExt, TryFutureExt};

use std::cell::RefCell;
use std::rc::{Rc, Weak};

pub type VatId = crate::rpc_twoparty_capnp::Side;

struct IncomingMessage {
    message: ::capnp::message::Reader<capnp::serialize::OwnedSegments>,
}

impl IncomingMessage {
    pub fn new(
        message: ::capnp::message::Reader<capnp::serialize::OwnedSegments>,
    ) -> IncomingMessage {
        IncomingMessage { message: message }
    }
}

impl crate::IncomingMessage for IncomingMessage {
    fn get_body<'a>(&'a self) -> ::capnp::Result<::capnp::any_pointer::Reader<'a>> {
        self.message.get_root()
    }
}

struct OutgoingMessage {
    message: ::capnp::message::Builder<::capnp::message::HeapAllocator>,
    sender: ::capnp_futures::Sender<Rc<::capnp::message::Builder<::capnp::message::HeapAllocator>>>,
}

impl crate::OutgoingMessage for OutgoingMessage {
    fn get_body<'a>(&'a mut self) -> ::capnp::Result<::capnp::any_pointer::Builder<'a>> {
        self.message.get_root()
    }

    fn get_body_as_reader<'a>(&'a self) -> ::capnp::Result<::capnp::any_pointer::Reader<'a>> {
        self.message.get_root_as_reader()
    }

    fn send(
        self: Box<Self>,
    ) -> (
        Promise<Rc<::capnp::message::Builder<::capnp::message::HeapAllocator>>, ::capnp::Error>,
        Rc<::capnp::message::Builder<::capnp::message::HeapAllocator>>,
    ) {
        let tmp = *self;
        let OutgoingMessage {
            message,
            mut sender,
        } = tmp;
        let m = Rc::new(message);
        (
            Promise::from_future(sender.send(m.clone()).map_err(|e| e.into())),
            m,
        )
    }

    fn take(self: Box<Self>) -> ::capnp::message::Builder<::capnp::message::HeapAllocator> {
        self.message
    }
}

struct ConnectionInner<T>
where
    T: AsyncRead + 'static,
{
    input_stream: Rc<RefCell<Option<T>>>,
    sender: ::capnp_futures::Sender<Rc<::capnp::message::Builder<::capnp::message::HeapAllocator>>>,
    side: crate::rpc_twoparty_capnp::Side,
    receive_options: ReaderOptions,
    on_disconnect_fulfiller: Option<oneshot::Sender<()>>,
}

struct Connection<T>
where
    T: AsyncRead + 'static,
{
    inner: Rc<RefCell<ConnectionInner<T>>>,
}

impl<T> Drop for ConnectionInner<T>
where
    T: AsyncRead,
{
    fn drop(&mut self) {
        let maybe_fulfiller = ::std::mem::replace(&mut self.on_disconnect_fulfiller, None);
        match maybe_fulfiller {
            Some(fulfiller) => {
                let _ = fulfiller.send(());
            }
            None => unreachable!(),
        }
    }
}

impl<T> Connection<T>
where
    T: AsyncRead,
{
    fn new(
        input_stream: T,
        sender: ::capnp_futures::Sender<
            Rc<::capnp::message::Builder<::capnp::message::HeapAllocator>>,
        >,
        side: crate::rpc_twoparty_capnp::Side,
        receive_options: ReaderOptions,
        on_disconnect_fulfiller: oneshot::Sender<()>,
    ) -> Connection<T> {
        Connection {
            inner: Rc::new(RefCell::new(ConnectionInner {
                input_stream: Rc::new(RefCell::new(Some(input_stream))),
                sender: sender,
                side: side,
                receive_options: receive_options,
                on_disconnect_fulfiller: Some(on_disconnect_fulfiller),
            })),
        }
    }
}

impl<T> crate::Connection<crate::rpc_twoparty_capnp::Side> for Connection<T>
where
    T: AsyncRead + Unpin,
{
    fn get_peer_vat_id(&self) -> crate::rpc_twoparty_capnp::Side {
        self.inner.borrow().side
    }

    fn new_outgoing_message(
        &mut self,
        _first_segment_word_size: u32,
    ) -> Box<dyn crate::OutgoingMessage> {
        Box::new(OutgoingMessage {
            message: ::capnp::message::Builder::new_default(),
            sender: self.inner.borrow().sender.clone(),
        })
    }

    fn receive_incoming_message(
        &mut self,
    ) -> Promise<Option<Box<dyn crate::IncomingMessage + 'static>>, ::capnp::Error> {
        let mut inner = self.inner.borrow_mut();
        let maybe_input_stream = ::std::mem::replace(&mut *inner.input_stream.borrow_mut(), None);
        let return_it_here = inner.input_stream.clone();
        match maybe_input_stream {
            Some(mut s) => {
                let receive_options = inner.receive_options;
                Promise::from_future(async move {
                    let maybe_message =
                        ::capnp_futures::serialize::read_message(&mut s, receive_options).await?;
                    *return_it_here.borrow_mut() = Some(s);
                    Ok(maybe_message.map(|message| {
                        Box::new(IncomingMessage::new(message)) as Box<dyn crate::IncomingMessage>
                    }))
                })
            }
            None => {
                Promise::err(::capnp::Error::failed(
                    "this should not be possible".to_string(),
                ))
                //   unreachable!(),
            }
        }
    }

    fn shutdown(&mut self, result: ::capnp::Result<()>) -> Promise<(), ::capnp::Error> {
        Promise::from_future(
            self.inner
                .borrow_mut()
                .sender
                .terminate(result)
                .map_err(|e| e.into()),
        )
    }
}

/// A vat network with two parties, the client and the server.
pub struct VatNetwork<T>
where
    T: AsyncRead + 'static + Unpin,
{
    connection: Option<Connection<T>>,

    // HACK
    weak_connection_inner: Weak<RefCell<ConnectionInner<T>>>,

    execution_driver: futures_util::future::Shared<Promise<(), ::capnp::Error>>,
    side: crate::rpc_twoparty_capnp::Side,
}

impl<T> VatNetwork<T>
where
    T: AsyncRead + Unpin,
{
    /// Creates a new two-party vat network that will receive data on `input_stream` and send data on
    /// `output_stream`.
    ///
    /// `side` indicates whether this is the client or the server side of the connection. This has no
    /// effect on the data sent over the connection; it merely exists so that `RpcNetwork::bootstrap` knows
    /// whether to return the local or the remote bootstrap capability. `VatId` parameters like this one
    /// will make more sense once we have vat networks with more than two parties.
    ///
    /// The options in `receive_options` will be used when reading the messages that come in on `input_stream`.
    pub fn new<U>(
        input_stream: T,
        output_stream: U,
        side: crate::rpc_twoparty_capnp::Side,
        receive_options: ReaderOptions,
    ) -> VatNetwork<T>
    where
        U: AsyncWrite + 'static + Unpin,
    {
        let (fulfiller, disconnect_promise) = oneshot::channel();
        let disconnect_promise =
            disconnect_promise.map_err(|_| ::capnp::Error::disconnected("disconnected".into()));

        let (execution_driver, sender) = {
            let (tx, write_queue) = ::capnp_futures::write_queue(output_stream);

            // Don't use `.join()` here because we need to make sure to wait for `disconnect_promise` to
            // resolve even if `write_queue` resolves to an error.
            (
                Promise::from_future(write_queue.then(move |r| {
                    disconnect_promise
                        .then(move |_| futures_util::future::ready(r))
                        .map_ok(|_| ())
                }))
                .shared(),
                tx,
            )
        };

        let connection = Connection::new(input_stream, sender, side, receive_options, fulfiller);
        let weak_inner = Rc::downgrade(&connection.inner);
        VatNetwork {
            connection: Some(connection),
            weak_connection_inner: weak_inner,
            execution_driver: execution_driver,
            side: side,
        }
    }
}

impl<T> crate::VatNetwork<VatId> for VatNetwork<T>
where
    T: AsyncRead + Unpin,
{
    fn connect(&mut self, host_id: VatId) -> Option<Box<dyn crate::Connection<VatId>>> {
        if host_id == self.side {
            None
        } else {
            let connection = ::std::mem::replace(&mut self.connection, None);
            match connection {
                Some(c) => Some(Box::new(c)),
                None => match self.weak_connection_inner.upgrade() {
                    Some(connection_inner) => Some(Box::new(Connection {
                        inner: connection_inner,
                    })),
                    None => panic!("tried to reconnect a disconnected twoparty vat network."),
                },
            }
        }
    }

    fn accept(&mut self) -> Promise<Box<dyn crate::Connection<VatId>>, ::capnp::Error> {
        let connection = ::std::mem::replace(&mut self.connection, None);
        match connection {
            Some(c) => Promise::ok(Box::new(c) as Box<dyn crate::Connection<VatId>>),
            None => Promise::from_future(futures_util::future::pending()),
        }
    }

    fn drive_until_shutdown(&mut self) -> Promise<(), ::capnp::Error> {
        Promise::from_future(self.execution_driver.clone())
    }
}
