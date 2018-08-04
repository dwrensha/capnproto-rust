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
use http_capnp::{http_session, outgoing_http};

use capnp::capability::Promise;
use capnp::Error;

use futures::{Future, Stream};

use tokio_core::reactor;
use tokio_io::AsyncRead;

struct OutgoingHttp {
    handle: reactor::Handle,
}

impl OutgoingHttp {
    fn new(handle: reactor::Handle) -> OutgoingHttp {
        OutgoingHttp { handle: handle }
    }
}

impl outgoing_http::Server for OutgoingHttp {
    fn new_session(
        &mut self,
        params: outgoing_http::NewSessionParams,
        mut results: outgoing_http::NewSessionResults,
    ) -> Promise<(), Error> {
        let session = HttpSession::new(
            ::tokio_curl::Session::new(self.handle.clone()),
            pry!(pry!(params.get()).get_base_url()).to_string(),
        );
        results
            .get()
            .set_session(http_session::ToClient::new(session).from_server::<::capnp_rpc::Server>());
        Promise::ok(())
    }
}

struct HttpSession {
    session: ::tokio_curl::Session,
    base_url: String,
}

impl HttpSession {
    fn new(session: ::tokio_curl::Session, base_url: String) -> HttpSession {
        HttpSession {
            session: session,
            base_url: base_url,
        }
    }
}

fn from_curl_error(e: ::curl::Error) -> Error {
    Error::failed(format!("curl error: {:?}", e))
}

fn from_perform_error(e: ::tokio_curl::PerformError) -> Error {
    Error::failed(format!("curl perform error: {:?}", e))
}

impl http_session::Server for HttpSession {
    fn get(
        &mut self,
        params: http_session::GetParams,
        mut results: http_session::GetResults,
    ) -> Promise<(), Error> {
        let path = pry!(pry!(params.get()).get_path());
        let mut url = self.base_url.clone();
        url.push_str(path);
        let mut easy = ::curl::easy::Easy::new();
        pry!(easy.url(&url).map_err(from_curl_error));
        pry!(easy.get(true).map_err(from_curl_error));

        // We need this channel to work around the `Send` bound required by write_function().
        let (tx, stream) = ::futures::sync::mpsc::unbounded::<Vec<u8>>();
        pry!(
            easy.write_function(move |data| {
                // Error case should only happen if this request has been canceled.
                let _ = tx.unbounded_send(data.into());
                Ok(data.len())
            }).map_err(from_curl_error)
        );

        Promise::from_future(
            self.session
                .perform(easy)
                .map_err(from_perform_error)
                .and_then(|mut response| response.response_code().map_err(from_curl_error))
                .and_then(move |code| {
                    results.get().set_response_code(code);
                    stream
                        .collect()
                        .and_then(move |writes| {
                            results.get().set_body(&writes.concat());
                            Ok(())
                        }).map_err(|()| unreachable!())
                }),
        )
    }

    fn post(
        &mut self,
        _params: http_session::PostParams,
        _results: http_session::PostResults,
    ) -> Promise<(), Error> {
        // TODO
        Promise::err(Error::unimplemented(format!("post() is unimplemented")))
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

    let proxy = outgoing_http::ToClient::new(OutgoingHttp::new(handle.clone()))
        .from_server::<::capnp_rpc::Server>();

    let handle1 = handle.clone();
    let done = socket.incoming().for_each(move |(socket, _addr)| {
        try!(socket.set_nodelay(true));
        let (reader, writer) = socket.split();
        let handle = handle1.clone();

        let network = twoparty::VatNetwork::new(
            reader,
            writer,
            rpc_twoparty_capnp::Side::Server,
            Default::default(),
        );

        let rpc_system = RpcSystem::new(Box::new(network), Some(proxy.clone().client));

        handle.spawn(rpc_system.map_err(|_| ()));
        Ok(())
    });

    core.run(done).unwrap();
}
