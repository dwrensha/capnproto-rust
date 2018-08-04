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
use http_capnp::outgoing_http;

use futures::Future;
use tokio_core::reactor;
use tokio_io::AsyncRead;

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
    let proxy: outgoing_http::Client = rpc_system.bootstrap(rpc_twoparty_capnp::Side::Server);

    handle.spawn(rpc_system.map_err(|_e| ()));

    let mut req = proxy.new_session_request();
    req.get().set_base_url("https://www.rust-lang.org");
    let session = req.send().pipeline.get_session();

    let mut req_root = session.get_request();
    req_root.get().set_path("/");

    let mut req_english = session.get_request();
    req_english.get().set_path("/en-US/");

    println!("sending two requests to https://www.rust-lang.org...");
    let (root_response, english_response) = core
        .run(req_root.send().promise.join(req_english.send().promise))
        .unwrap();
    {
        let root = root_response.get().unwrap();
        println!(
            "got body of length {} with response code of {} for /",
            root.get_body().unwrap().len(),
            root.get_response_code()
        );
    }

    {
        let english = english_response.get().unwrap();
        println!(
            "got body of length {} with response code of {} for /en-US/",
            english.get_body().unwrap().len(),
            english.get_response_code()
        );
    }
}
