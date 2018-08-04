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

use capnp_rpc::{rpc_twoparty_capnp, twoparty, RpcSystem};
use pubsub_capnp::{publisher, subscriber};

use capnp::capability::Promise;

use futures::Future;
use tokio_core::reactor;
use tokio_io::AsyncRead;

struct SubscriberImpl;

impl subscriber::Server<::capnp::text::Owned> for SubscriberImpl {
    fn push_message(
        &mut self,
        params: subscriber::PushMessageParams<::capnp::text::Owned>,
        _results: subscriber::PushMessageResults<::capnp::text::Owned>,
    ) -> Promise<(), ::capnp::Error> {
        println!(
            "message from publisher: {}",
            pry!(pry!(params.get()).get_message())
        );
        Promise::ok(())
    }
}

pub fn main() {
    use std::net::ToSocketAddrs;
    let args: Vec<String> = ::std::env::args().collect();
    if args.len() != 3 {
        println!("usage: {} client HOST:PORT", args[0]);
        return;
    }

    let mut core = reactor::Core::new().unwrap();
    let handle = core.handle();

    let addr = args[2]
        .to_socket_addrs()
        .unwrap()
        .next()
        .expect("could not parse address");
    let stream = core
        .run(::tokio_core::net::TcpStream::connect(&addr, &handle))
        .unwrap();
    stream.set_nodelay(true).unwrap();
    let (reader, writer) = stream.split();

    let rpc_network = Box::new(twoparty::VatNetwork::new(
        reader,
        writer,
        rpc_twoparty_capnp::Side::Client,
        Default::default(),
    ));

    let mut rpc_system = RpcSystem::new(rpc_network, None);
    let publisher: publisher::Client<::capnp::text::Owned> =
        rpc_system.bootstrap(rpc_twoparty_capnp::Side::Server);

    let sub = subscriber::ToClient::new(SubscriberImpl).from_server::<::capnp_rpc::Server>();

    let mut request = publisher.subscribe_request();
    request.get().set_subscriber(sub);

    // Need to make sure not to drop the returned subscription object.
    let _result = core.run(rpc_system.join(request.send().promise)).unwrap();
}
