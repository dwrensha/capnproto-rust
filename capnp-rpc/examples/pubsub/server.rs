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

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use capnp_rpc::{rpc_twoparty_capnp, twoparty, RpcSystem};
use pubsub_capnp::{publisher, subscriber, subscription};

use capnp::capability::Promise;
use capnp::Error;

use futures::{Future, Stream};

use tokio_core::reactor;
use tokio_io::AsyncRead;

struct SubscriberHandle {
    client: subscriber::Client<::capnp::text::Owned>,
    requests_in_flight: i32,
}

struct SubscriberMap {
    subscribers: HashMap<u64, SubscriberHandle>,
}

impl SubscriberMap {
    fn new() -> SubscriberMap {
        SubscriberMap {
            subscribers: HashMap::new(),
        }
    }
}

struct SubscriptionImpl {
    id: u64,
    subscribers: Rc<RefCell<SubscriberMap>>,
}

impl SubscriptionImpl {
    fn new(id: u64, subscribers: Rc<RefCell<SubscriberMap>>) -> SubscriptionImpl {
        SubscriptionImpl {
            id: id,
            subscribers: subscribers,
        }
    }
}

impl Drop for SubscriptionImpl {
    fn drop(&mut self) {
        println!("subscription dropped");
        self.subscribers.borrow_mut().subscribers.remove(&self.id);
    }
}

impl subscription::Server for SubscriptionImpl {}

struct PublisherImpl {
    next_id: u64,
    subscribers: Rc<RefCell<SubscriberMap>>,
}

impl PublisherImpl {
    pub fn new() -> (PublisherImpl, Rc<RefCell<SubscriberMap>>) {
        let subscribers = Rc::new(RefCell::new(SubscriberMap::new()));
        (
            PublisherImpl {
                next_id: 0,
                subscribers: subscribers.clone(),
            },
            subscribers.clone(),
        )
    }
}

impl publisher::Server<::capnp::text::Owned> for PublisherImpl {
    fn subscribe(
        &mut self,
        params: publisher::SubscribeParams<::capnp::text::Owned>,
        mut results: publisher::SubscribeResults<::capnp::text::Owned>,
    ) -> Promise<(), ::capnp::Error> {
        println!("subscribe");
        self.subscribers.borrow_mut().subscribers.insert(
            self.next_id,
            SubscriberHandle {
                client: pry!(pry!(params.get()).get_subscriber()),
                requests_in_flight: 0,
            },
        );

        results.get().set_subscription(
            subscription::ToClient::new(SubscriptionImpl::new(
                self.next_id,
                self.subscribers.clone(),
            )).from_server::<::capnp_rpc::Server>(),
        );

        self.next_id += 1;
        Promise::ok(())
    }
}
pub fn main() {
    use std::net::ToSocketAddrs;
    let args: Vec<String> = ::std::env::args().collect();
    if args.len() != 3 {
        println!("usage: {} server HOST:PORT", args[0]);
        return;
    }

    let mut core = reactor::Core::new().unwrap();
    let handle = core.handle();

    let addr = args[2]
        .to_socket_addrs()
        .unwrap()
        .next()
        .expect("could not parse address");
    let socket = ::tokio_core::net::TcpListener::bind(&addr, &handle).unwrap();

    let (publisher_impl, subscribers) = PublisherImpl::new();

    let publisher = publisher::ToClient::new(publisher_impl).from_server::<::capnp_rpc::Server>();

    let handle1 = handle.clone();
    let done = socket
        .incoming()
        .for_each(move |(socket, _addr)| {
            try!(socket.set_nodelay(true));
            let (reader, writer) = socket.split();
            let handle = handle1.clone();

            let network = twoparty::VatNetwork::new(
                reader,
                writer,
                rpc_twoparty_capnp::Side::Server,
                Default::default(),
            );

            let rpc_system = RpcSystem::new(Box::new(network), Some(publisher.clone().client));

            handle.spawn(rpc_system.map_err(|_| ()));
            Ok(())
        }).map_err(|e| e.into());

    let infinite = ::futures::stream::iter_ok::<_, Error>(::std::iter::repeat(()));
    let send_to_subscribers =
        infinite.fold(
            (handle, subscribers),
            move |(handle, subscribers),
                  ()|
                  -> Promise<
                (::tokio_core::reactor::Handle, Rc<RefCell<SubscriberMap>>),
                Error,
            > {
                {
                    let subscribers1 = subscribers.clone();
                    let subs = &mut subscribers.borrow_mut().subscribers;
                    for (&idx, mut subscriber) in subs.iter_mut() {
                        if subscriber.requests_in_flight < 5 {
                            subscriber.requests_in_flight += 1;
                            let mut request = subscriber.client.push_message_request();
                            pry!(request.get().set_message(
                                &format!("system time is: {:?}", ::std::time::SystemTime::now())[..]
                            ));

                            let subscribers2 = subscribers1.clone();
                            handle.spawn(
                                request
                                    .send()
                                    .promise
                                    .then(move |r| {
                                        match r {
                                            Ok(_) => {
                                                subscribers2
                                                    .borrow_mut()
                                                    .subscribers
                                                    .get_mut(&idx)
                                                    .map(|ref mut s| {
                                                        s.requests_in_flight -= 1;
                                                    });
                                            }
                                            Err(e) => {
                                                println!(
                                                    "Got error: {:?}. Dropping subscriber.",
                                                    e
                                                );
                                                subscribers2.borrow_mut().subscribers.remove(&idx);
                                            }
                                        }
                                        Ok::<(), Error>(())
                                    }).map_err(|_| unreachable!()),
                            );
                        }
                    }
                }

                let timeout = pry!(::tokio_core::reactor::Timeout::new(
                    ::std::time::Duration::from_secs(1),
                    &handle
                ));
                let timeout = timeout
                    .and_then(move |()| Ok((handle, subscribers)))
                    .map_err(|e| e.into());
                Promise::from_future(timeout)
            },
        );

    core.run(send_to_subscribers.join(done)).unwrap();
}
