// Copyright (c) 2013-2015 Sandstorm Development Group, Inc. and contributors
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

#![cfg(test)]

extern crate capnp;

#[macro_use]
extern crate capnp_rpc;

extern crate futures;
extern crate tokio_core;
extern crate tokio_io;

extern crate mio_uds;

use capnp::capability::Promise;
use capnp::Error;
use capnp_rpc::{rpc_twoparty_capnp, twoparty, RpcSystem};

use futures::future::Either;
use futures::sync::oneshot;
use futures::Future;

use tokio_core::reactor;
use tokio_io::AsyncRead;

pub mod test_capnp {
    include!(concat!(env!("OUT_DIR"), "/test_capnp.rs"));
}

pub mod impls;
pub mod test_util;

#[test]
fn drop_rpc_system() {
    let mut core = reactor::Core::new().unwrap();
    let handle = core.handle();

    let (instream, _outstream) = ::mio_uds::UnixStream::pair().unwrap();

    let instream = reactor::PollEvented::new(instream, &handle).unwrap();
    let (reader, writer) = instream.split();

    let network = Box::new(twoparty::VatNetwork::new(
        reader,
        writer,
        rpc_twoparty_capnp::Side::Client,
        Default::default(),
    ));
    let rpc_system = RpcSystem::new(network, None);
    drop(rpc_system);
    core.turn(Some(::std::time::Duration::from_millis(1)));
    core.turn(Some(::std::time::Duration::from_millis(1)));
    core.turn(Some(::std::time::Duration::from_millis(1)));
}

fn disconnector_setup(
    handle: &tokio_core::reactor::Handle,
) -> (
    RpcSystem<capnp_rpc::rpc_twoparty_capnp::Side>,
    RpcSystem<capnp_rpc::rpc_twoparty_capnp::Side>,
) {
    let (client_stream, server_stream) = ::mio_uds::UnixStream::pair().unwrap();
    let (client_reader, client_writer) = reactor::PollEvented::new(client_stream, &handle)
        .unwrap()
        .split();

    let client_network = Box::new(twoparty::VatNetwork::new(
        client_reader,
        client_writer,
        rpc_twoparty_capnp::Side::Client,
        Default::default(),
    ));

    let client_rpc_system = RpcSystem::new(client_network, None);

    let (server_reader, server_writer) = reactor::PollEvented::new(server_stream, &handle)
        .unwrap()
        .split();

    let server_network = Box::new(twoparty::VatNetwork::new(
        server_reader,
        server_writer,
        rpc_twoparty_capnp::Side::Server,
        Default::default(),
    ));

    let bootstrap =
        test_capnp::bootstrap::ToClient::new(impls::Bootstrap).from_server::<::capnp_rpc::Server>();

    let server_rpc_system = RpcSystem::new(server_network, Some(bootstrap.client));

    (client_rpc_system, server_rpc_system)
}

#[test]
fn drop_import_client_after_disconnect() {
    let mut core = reactor::Core::new().unwrap();
    let handle = core.handle();

    let (mut client_rpc_system, server_rpc_system) = disconnector_setup(&handle);

    let client: test_capnp::bootstrap::Client =
        client_rpc_system.bootstrap(rpc_twoparty_capnp::Side::Server);
    handle.spawn(client_rpc_system.map_err(|e| {
        println!("RpcSystem error: {:?}", e);
        ()
    }));

    let (tx, rx) = oneshot::channel::<()>();
    let rx = rx.map_err(|_| ());
    handle.spawn(
        rx.join(server_rpc_system.map_err(|e| {
            println!("RpcSystem error: {:?}", e);
            ()
        })).map(|_| ()),
    );

    core.run(client.test_interface_request().send().promise)
        .unwrap();

    drop(tx);

    match core.run(client.test_interface_request().send().promise) {
        Err(ref e) if e.kind == ::capnp::ErrorKind::Disconnected => (),
        _ => panic!("Should have gotten a 'disconnected' error."),
    }

    // At one point, attempting to call again would cause a panic.
    match core.run(client.test_interface_request().send().promise) {
        Err(ref e) if e.kind == ::capnp::ErrorKind::Disconnected => (),
        _ => panic!("Should have gotten a 'disconnected' error."),
    }

    drop(client);
}

#[test]
fn disconnector_disconnects() {
    let mut core = reactor::Core::new().unwrap();
    let handle = core.handle();

    let (mut client_rpc_system, server_rpc_system) = disconnector_setup(&handle);

    let client: test_capnp::bootstrap::Client =
        client_rpc_system.bootstrap(rpc_twoparty_capnp::Side::Server);
    let disconnector = client_rpc_system.get_disconnector();

    handle.spawn(client_rpc_system.map_err(|e| {
        println!("RpcSystem error: {:?}", e);
        ()
    }));

    let (tx, rx) = oneshot::channel::<()>();
    //send on tx when server_rpc_system exits
    handle.spawn(
        server_rpc_system
            .map_err(|e| {
                println!("RpcSystem error: {:?}", e);
                e
            }).then(|_| tx.send(())),
    );

    //make sure we can make an RPC system call
    core.run(client.test_interface_request().send().promise)
        .unwrap();

    //disconnect from the server; comment this next line out to see the test fail
    core.run(disconnector).unwrap();

    let timeout =
        tokio_core::reactor::Timeout::new(std::time::Duration::from_secs(1), &handle).unwrap();
    //wait one second for server_rpc_system to exit
    match core.run(rx.select2(timeout)) {
        Ok(Either::B(_)) => panic!("timeout while waiting for server_rpc_system to exit."),
        Err(_) => panic!("an error occurred while waiting for server_rpc_system to exit."),
        _ => (),
    }

    //make sure we can't use client any more (because the server is disconnected)
    match core.run(client.test_interface_request().send().promise) {
        Err(ref e) if e.kind == ::capnp::ErrorKind::Disconnected => (),
        _ => panic!("Should have gotten a 'disconnected' error."),
    }
}

fn rpc_top_level<F>(main: F)
where
    F: FnOnce(::tokio_core::reactor::Core, test_capnp::bootstrap::Client) -> Result<(), Error>,
    F: Send + 'static,
{
    let core = reactor::Core::new().unwrap();
    let handle = core.handle();
    let (client_stream, server_stream) = ::mio_uds::UnixStream::pair().unwrap();

    let join_handle = ::std::thread::spawn(move || {
        let mut core = reactor::Core::new().unwrap();
        let handle = core.handle();
        let (server_reader, server_writer) = reactor::PollEvented::new(server_stream, &handle)
            .unwrap()
            .split();

        let network = Box::new(twoparty::VatNetwork::new(
            server_reader,
            server_writer,
            rpc_twoparty_capnp::Side::Server,
            Default::default(),
        ));

        let bootstrap = test_capnp::bootstrap::ToClient::new(impls::Bootstrap)
            .from_server::<::capnp_rpc::Server>();

        let rpc_system = RpcSystem::new(network, Some(bootstrap.client));

        core.run(rpc_system).unwrap();
    });

    let (client_reader, client_writer) = reactor::PollEvented::new(client_stream, &handle)
        .unwrap()
        .split();

    let network = Box::new(twoparty::VatNetwork::new(
        client_reader,
        client_writer,
        rpc_twoparty_capnp::Side::Client,
        Default::default(),
    ));

    let mut rpc_system = RpcSystem::new(network, None);
    let client: test_capnp::bootstrap::Client =
        rpc_system.bootstrap(rpc_twoparty_capnp::Side::Server);

    handle.spawn(rpc_system.map_err(|e| {
        println!("RpcSystem error: {:?}", e);
        ()
    }));

    main(core, client).unwrap();
    join_handle.join().expect("thread exited unsuccessfully");
}

#[test]
fn do_nothing() {
    rpc_top_level(|_core, _client| Ok(()));
}

#[test]
fn basic_rpc_calls() {
    rpc_top_level(|mut core, client| {
        let response = try!(core.run(client.test_interface_request().send().promise));
        let client = try!(try!(response.get()).get_cap());

        let mut request1 = client.foo_request();
        request1.get().set_i(123);
        request1.get().set_j(true);
        let promise1 = request1.send();

        let request3 = client.bar_request();
        let promise3 = request3.send().promise.then(|result| {
            // We expect this call to fail.
            match result {
                Ok(_) => Promise::err(Error::failed("expected bar() to fail".to_string())),
                Err(_) => Promise::ok(()),
            }
        });

        let mut request2 = client.baz_request();

        ::test_util::init_test_message(try!(request2.get().get_s()));
        let promise2 = request2.send();

        let response1 = try!(core.run(promise1.promise));

        if try!(try!(response1.get()).get_x()) != "foo" {
            return Err(Error::failed("expected X to equal 'foo'".to_string()));
        }

        try!(core.run(promise2.promise));
        try!(core.run(promise3));
        Ok(())
    });
}

#[test]
fn basic_pipelining() {
    rpc_top_level(|mut core, client| {
        let response = try!(core.run(client.test_pipeline_request().send().promise));
        let client = try!(try!(response.get()).get_cap());

        let mut request = client.get_cap_request();
        request.get().set_n(234);
        let server = impls::TestInterface::new();
        let chained_call_count = server.get_call_count();
        request.get().set_in_cap(
            ::test_capnp::test_interface::ToClient::new(server)
                .from_server::<::capnp_rpc::Server>(),
        );

        let promise = request.send();

        let mut pipeline_request = promise.pipeline.get_out_box().get_cap().foo_request();
        pipeline_request.get().set_i(321);
        let pipeline_promise = pipeline_request.send();

        let pipeline_request2 = {
            let extends_client = ::test_capnp::test_extends::Client {
                client: promise.pipeline.get_out_box().get_cap().client,
            };
            extends_client.grault_request()
        };
        let pipeline_promise2 = pipeline_request2.send();

        drop(promise); // Just to be annoying, drop the original promise.

        if chained_call_count.get() != 0 {
            return Err(Error::failed(
                "expected chained_call_count to equal 0".to_string(),
            ));
        }

        let response = try!(core.run(pipeline_promise.promise));

        if try!(try!(response.get()).get_x()) != "bar" {
            return Err(Error::failed("expected x to equal 'bar'".to_string()));
        }

        let response2 = try!(core.run(pipeline_promise2.promise));
        ::test_util::CheckTestMessage::check_test_message(try!(response2.get()));
        assert_eq!(chained_call_count.get(), 1);
        Ok(())
    });
}

#[test]
fn pipelining_return_null() {
    rpc_top_level(|mut core, client| {
        let response = try!(core.run(client.test_pipeline_request().send().promise));
        let client = try!(try!(response.get()).get_cap());

        let request = client.get_null_cap_request();
        let cap = request.send().pipeline.get_cap();
        match core.run(cap.foo_request().send().promise) {
            Err(ref e) => {
                if e.description
                    .contains("Message contains null capability pointer")
                {
                    Ok(())
                } else {
                    Err(Error::failed(format!(
                        "Should have gotten null capability error. Instead got {:?}",
                        e
                    )))
                }
            }
            Ok(_) => Err(Error::failed(format!(
                "Should have gotten null capability error."
            ))),
        }
    });
}

#[test]
fn null_capability() {
    let mut message = ::capnp::message::Builder::new_default();
    let root: ::test_capnp::test_all_types::Builder = message.get_root().unwrap();

    // In capnproto-c++, this would return a BrokenCap. Here, it returns a decode error.
    // Would it be worthwhile to try to match the C++ behavior here? We would need something
    // like the BrokenCapFactory singleton.
    assert!(root.get_interface_field().is_err());
}

#[test]
fn release_simple() {
    rpc_top_level(|mut core, client| {
        let response = try!(core.run(client.test_more_stuff_request().send().promise));
        let client = try!(try!(response.get()).get_cap());

        let handle1 = client.get_handle_request().send().promise;
        let ::capnp::capability::RemotePromise { promise, pipeline } =
            client.get_handle_request().send();
        let handle2 = try!(try!(try!(core.run(promise)).get()).get_handle());

        let get_count_response = try!(core.run(client.get_handle_count_request().send().promise));
        if try!(get_count_response.get()).get_count() != 2 {
            return Err(Error::failed(
                "expected handle count to equal 2".to_string(),
            ));
        }

        drop(handle1);

        let get_count_response = try!(core.run(client.get_handle_count_request().send().promise));
        if try!(get_count_response.get()).get_count() != 1 {
            return Err(Error::failed(
                "expected handle count to equal 1".to_string(),
            ));
        }

        drop(handle2);

        let get_count_response = try!(core.run(client.get_handle_count_request().send().promise));
        if try!(get_count_response.get()).get_count() != 1 {
            return Err(Error::failed(
                "expected handle count to equal 1".to_string(),
            ));
        }

        drop(pipeline);

        let get_count_response = try!(core.run(client.get_handle_count_request().send().promise));
        if try!(get_count_response.get()).get_count() != 0 {
            return Err(Error::failed(
                "expected handle count to equal 0".to_string(),
            ));
        }

        Ok(())
    });
}

#[test]
fn release_on_cancel() {
    rpc_top_level(|mut core, client| {
        let response = try!(core.run(client.test_more_stuff_request().send().promise));
        let client = try!(try!(response.get()).get_cap());

        let promise = client.get_handle_request().send();

        // If the server receives cancellation too early, it won't even return a capability in the
        // results, it will just return "canceled". We want to emulate the case where the return message
        // and the cancel (finish) message cross paths.

        // TODO Verify that this is actually testing what we're interested in.

        core.turn(Some(::std::time::Duration::from_millis(1)));
        core.turn(Some(::std::time::Duration::from_millis(1)));

        drop(promise);

        for _ in 0..16 {
            core.turn(Some(::std::time::Duration::from_millis(1)));
        }

        let get_count_response = try!(core.run(client.get_handle_count_request().send().promise));
        let handle_count = try!(get_count_response.get()).get_count();
        if handle_count != 0 {
            return Err(Error::failed(format!(
                "handle count: expected 0, but got {}",
                handle_count
            )));
        }
        Ok(())
    });
}

#[test]
fn promise_resolve() {
    rpc_top_level(|mut core, client| {
        let response = try!(core.run(client.test_more_stuff_request().send().promise));
        let client = try!(try!(response.get()).get_cap());

        let mut request = client.call_foo_request();
        let mut request2 = client.call_foo_when_resolved_request();

        let (paf_fulfiller, paf_promise) = oneshot::channel();
        let cap: ::test_capnp::test_interface::Client =
            ::capnp_rpc::new_promise_client(paf_promise.map_err(|e| e.into()));
        request.get().set_cap(cap.clone());
        request2.get().set_cap(cap);

        let promise = request.send().promise;
        let promise2 = request2.send().promise;

        // Make sure getCap() has been called on the server side by sending another call and waiting
        // for it.
        let client2 = ::test_capnp::test_call_order::Client {
            client: client.clone().client,
        };
        let _response = try!(core.run(client2.get_call_sequence_request().send().promise));

        let server = impls::TestInterface::new();
        let _ = paf_fulfiller.send(
            ::test_capnp::test_interface::ToClient::new(server)
                .from_server::<::capnp_rpc::Server>()
                .client,
        );

        let response = try!(core.run(promise));
        if try!(try!(response.get()).get_s()) != "bar" {
            return Err(Error::failed("expected s to equal 'bar'".to_string()));
        }
        let response = try!(core.run(promise2));
        if try!(try!(response.get()).get_s()) != "bar" {
            return Err(Error::failed("expected s to equal 'bar'".to_string()));
        }
        Ok(())
    });
}

#[test]
fn retain_and_release() {
    use std::cell::Cell;
    use std::rc::Rc;

    rpc_top_level(|mut core, client| {
        let (fulfiller, promise) = oneshot::channel::<()>();
        let destroyed = Rc::new(Cell::new(false));

        let (destroyed_done_sender, destroyed_done_receiver) = oneshot::channel::<()>();

        let destroyed1 = destroyed.clone();
        core.handle().spawn(
            promise
                .and_then(move |()| {
                    destroyed1.set(true);
                    let _ = destroyed_done_sender.send(());
                    Ok(())
                }).map_err(|_| ()),
        );

        {
            let response = try!(core.run(client.test_more_stuff_request().send().promise));
            let client = try!(try!(response.get()).get_cap());

            {
                let mut request = client.hold_request();
                request.get().set_cap(
                    ::test_capnp::test_interface::ToClient::new(impls::TestCapDestructor::new(
                        fulfiller,
                    )).from_server::<::capnp_rpc::Server>(),
                );
                try!(core.run(request.send().promise));
            }

            // Do some other call to add a round trip.
            // ugh, we need upcasting.
            let client1 = ::test_capnp::test_call_order::Client {
                client: client.clone().client,
            };
            let response = try!(core.run(client1.get_call_sequence_request().send().promise));
            if try!(response.get()).get_n() != 1 {
                return Err(Error::failed("N should equal 1".to_string()));
            }

            if destroyed.get() {
                return Err(Error::failed("shouldn't be destroyed yet".to_string()));
            }

            // We can ask it to call the held capability.
            let response = try!(core.run(client.call_held_request().send().promise));
            if try!(try!(response.get()).get_s()) != "bar" {
                return Err(Error::failed("S should equal 'bar'".to_string()));
            }

            {
                // we can get the cap back from it.
                let response = try!(core.run(client.get_held_request().send().promise));
                let cap_copy = try!(try!(response.get()).get_cap());

                // And call it, without any network communications.
                // (TODO: verify that no network communications happen here)
                {
                    let mut request = cap_copy.foo_request();
                    request.get().set_i(123);
                    request.get().set_j(true);
                    let response = try!(core.run(request.send().promise));
                    if try!(try!(response.get()).get_x()) != "foo" {
                        return Err(Error::failed("X should equal 'foo'.".to_string()));
                    }
                }

                {
                    // We can send another copy of the same cap to another method, and it works.
                    let mut request = client.call_foo_request();
                    request.get().set_cap(cap_copy);
                    let response = try!(core.run(request.send().promise));
                    if try!(try!(response.get()).get_s()) != "bar" {
                        return Err(Error::failed("S should equal 'bar'.".to_string()));
                    }
                }
            }

            // Give some time to settle.
            let response = try!(core.run(client1.get_call_sequence_request().send().promise));
            if try!(response.get()).get_n() != 5 {
                return Err(Error::failed("N should equal 5.".to_string()));
            }
            let response = try!(core.run(client1.get_call_sequence_request().send().promise));
            if try!(response.get()).get_n() != 6 {
                return Err(Error::failed("N should equal 6.".to_string()));
            }
            let response = try!(core.run(client1.get_call_sequence_request().send().promise));
            if try!(response.get()).get_n() != 7 {
                return Err(Error::failed("N should equal 7.".to_string()));
            }

            if destroyed.get() {
                return Err(Error::failed("haven't released it yet".to_string()));
            }
        }

        try!(core.run(destroyed_done_receiver));
        if !destroyed.get() {
            return Err(Error::failed("should be destroyed now".to_string()));
        }

        Ok(())
    });
}

#[test]
fn cancel_releases_params() {
    use std::cell::Cell;
    use std::rc::Rc;

    rpc_top_level(|mut core, client| {
        let response = try!(core.run(client.test_more_stuff_request().send().promise));
        let client = try!(try!(response.get()).get_cap());

        let (fulfiller, promise) = oneshot::channel::<()>();
        let destroyed = Rc::new(Cell::new(false));

        let (destroyed_done_sender, destroyed_done_receiver) = oneshot::channel::<()>();

        let destroyed1 = destroyed.clone();
        core.handle().spawn(
            promise
                .and_then(move |()| {
                    destroyed1.set(true);
                    let _ = destroyed_done_sender.send(());
                    Ok(())
                }).map_err(|_| ()),
        );

        {
            let mut request = client.never_return_request();
            request.get().set_cap(
                ::test_capnp::test_interface::ToClient::new(impls::TestCapDestructor::new(
                    fulfiller,
                )).from_server::<::capnp_rpc::Server>(),
            );

            {
                let _response_promise = request.send();

                // Allow some time to settle.

                // ugh, we need upcasting.
                let client = ::test_capnp::test_call_order::Client {
                    client: client.client,
                };
                let response = try!(core.run(client.get_call_sequence_request().send().promise));
                if try!(response.get()).get_n() != 1 {
                    return Err(Error::failed("N should equal 1.".to_string()));
                }
                let response = try!(core.run(client.get_call_sequence_request().send().promise));
                if try!(response.get()).get_n() != 2 {
                    return Err(Error::failed("N should equal 2.".to_string()));
                }
                if destroyed.get() {
                    return Err(Error::failed("Shouldn't be destroyed yet.".to_string()));
                }
            }
        }

        try!(core.run(destroyed_done_receiver));
        if !destroyed.get() {
            return Err(Error::failed("The cap should be released now.".to_string()));
        }

        Ok(())
    });
}

#[test]
fn dont_hold() {
    rpc_top_level(|mut core, client| {
        let response = try!(core.run(client.test_more_stuff_request().send().promise));
        let client = try!(try!(response.get()).get_cap());

        let (fulfiller, promise) = oneshot::channel();
        let cap: ::test_capnp::test_interface::Client =
            ::capnp_rpc::new_promise_client(promise.map_err(|e| e.into()));

        let mut request = client.dont_hold_request();
        request.get().set_cap(cap.clone());

        core.run(request.send().promise.and_then(move |_response| {
            let mut request = client.dont_hold_request();
            request.get().set_cap(cap.clone());
            request.send().promise.and_then(move |_| {
                drop(fulfiller);
                Promise::ok(())
            })
        }))
    });
}

fn get_call_sequence(
    client: &::test_capnp::test_call_order::Client,
    expected: u32,
) -> ::capnp::capability::RemotePromise<
    ::test_capnp::test_call_order::get_call_sequence_results::Owned,
> {
    let mut req = client.get_call_sequence_request();
    req.get().set_expected(expected);
    req.send()
}

#[test]
fn embargo_success() {
    rpc_top_level(|mut core, client| {
        let response = try!(core.run(client.test_more_stuff_request().send().promise));
        let client = try!(try!(response.get()).get_cap());

        let server = ::impls::TestCallOrder::new();

        // ugh, we need upcasting.
        let client2 = ::test_capnp::test_call_order::Client {
            client: client.clone().client,
        };
        let early_call = client2.get_call_sequence_request().send();
        drop(client2);

        let mut echo_request = client.echo_request();
        echo_request.get().set_cap(
            ::test_capnp::test_call_order::ToClient::new(server)
                .from_server::<::capnp_rpc::Server>(),
        );
        let echo = echo_request.send();

        let pipeline = echo.pipeline.get_cap();

        let call0 = get_call_sequence(&pipeline, 0);
        let call1 = get_call_sequence(&pipeline, 1);

        try!(core.run(early_call.promise));

        let call2 = get_call_sequence(&pipeline, 2);

        let _resolved = try!(core.run(echo.promise));

        let call3 = get_call_sequence(&pipeline, 3);
        let call4 = get_call_sequence(&pipeline, 4);
        let call5 = get_call_sequence(&pipeline, 5);

        core.run(
            ::futures::future::join_all(vec![
                call0.promise,
                call1.promise,
                call2.promise,
                call3.promise,
                call4.promise,
                call5.promise,
            ]).and_then(|responses| {
                let mut counter = 0;
                for r in responses.into_iter() {
                    if counter != try!(r.get()).get_n() {
                        return Err(Error::failed("calls arrived out of order".to_string()));
                    }
                    counter += 1;
                }
                Ok(())
            }),
        )
    });
}

fn expect_promise_throws<T>(
    promise: Promise<T, Error>,
    core: &mut ::tokio_core::reactor::Core,
) -> Result<(), Error> {
    core.run(promise.then(|r| match r {
        Ok(_) => Err(Error::failed("expected promise to fail".to_string())),
        Err(_) => Ok(()),
    }))
}

#[test]
fn embargo_error() {
    rpc_top_level(|mut core, client| {
        let response = try!(core.run(client.test_more_stuff_request().send().promise));
        let client = try!(try!(response.get()).get_cap());

        let (fulfiller, promise) = oneshot::channel();
        let cap: ::test_capnp::test_call_order::Client =
            ::capnp_rpc::new_promise_client(promise.map_err(|e| e.into()));

        // ugh, we need upcasting.
        let client2 = ::test_capnp::test_call_order::Client {
            client: client.clone().client,
        };
        let early_call = client2.get_call_sequence_request().send();
        drop(client2);

        let mut echo_request = client.echo_request();
        echo_request.get().set_cap(cap);
        let echo = echo_request.send();

        let pipeline = echo.pipeline.get_cap();

        let call0 = get_call_sequence(&pipeline, 0);
        let call1 = get_call_sequence(&pipeline, 1);

        try!(core.run(early_call.promise));

        let call2 = get_call_sequence(&pipeline, 2);

        let _resolved = core.run(echo.promise);

        let call3 = get_call_sequence(&pipeline, 3);
        let call4 = get_call_sequence(&pipeline, 4);
        let call5 = get_call_sequence(&pipeline, 5);

        drop(fulfiller);

        try!(expect_promise_throws(call0.promise, &mut core));
        try!(expect_promise_throws(call1.promise, &mut core));
        try!(expect_promise_throws(call2.promise, &mut core));
        try!(expect_promise_throws(call3.promise, &mut core));
        try!(expect_promise_throws(call4.promise, &mut core));
        try!(expect_promise_throws(call5.promise, &mut core));
        Ok(())
    });
}

#[test]
fn echo_destruction() {
    rpc_top_level(|mut core, client| {
        let response = try!(core.run(client.test_more_stuff_request().send().promise));
        let client = try!(try!(response.get()).get_cap());

        let (fulfiller, promise) = oneshot::channel();
        let cap: ::test_capnp::test_call_order::Client =
            ::capnp_rpc::new_promise_client(promise.map_err(|e| e.into()));

        // ugh, we need upcasting.
        let client2 = ::test_capnp::test_call_order::Client {
            client: client.clone().client,
        };
        let early_call = client2.get_call_sequence_request().send();
        drop(client2);

        let mut echo_request = client.echo_request();
        echo_request.get().set_cap(cap);
        let echo = echo_request.send();

        let pipeline = echo.pipeline.get_cap();

        core.run(early_call.promise.and_then(move |_early_call_response| {
            let _ = get_call_sequence(&pipeline, 2);
            echo.promise.and_then(move |_echo_response| {
                drop(fulfiller);
                Promise::ok(())
            })
        }))
    })
}

#[test]
fn local_client_call_not_immediate() {
    let server = ::impls::TestInterface::new();
    let call_count = server.get_call_count();
    assert_eq!(call_count.get(), 0);
    let client =
        ::test_capnp::test_interface::ToClient::new(server).from_server::<::capnp_rpc::Server>();
    let mut req = client.foo_request();
    req.get().set_i(123);
    req.get().set_j(true);
    let remote_promise = req.send();

    // Hm... do we actually care about this?
    assert_eq!(call_count.get(), 0);

    let _ = remote_promise.promise.wait();
    assert_eq!(call_count.get(), 1);
}

#[test]
fn local_client_send_cap() {
    let server1 = ::impls::TestMoreStuff::new();
    let server2 = ::impls::TestInterface::new();
    let client1 =
        ::test_capnp::test_more_stuff::ToClient::new(server1).from_server::<::capnp_rpc::Server>();
    let client2 =
        ::test_capnp::test_interface::ToClient::new(server2).from_server::<::capnp_rpc::Server>();

    let mut req = client1.call_foo_request();
    req.get().set_cap(client2);
    let response = req.send().promise.wait().unwrap();
    assert_eq!(response.get().unwrap().get_s().unwrap(), "bar");
}

#[test]
fn local_client_return_cap() {
    let server = ::impls::Bootstrap;
    let client =
        ::test_capnp::bootstrap::ToClient::new(server).from_server::<::capnp_rpc::Server>();
    let response = client
        .test_interface_request()
        .send()
        .promise
        .wait()
        .unwrap();
    let client1 = response.get().unwrap().get_cap().unwrap();

    let mut request = client1.foo_request();
    request.get().set_i(123);
    request.get().set_j(true);
    let response1 = request.send().promise.wait().unwrap();
    assert_eq!(response1.get().unwrap().get_x().unwrap(), "foo");
}

#[test]
fn capability_list() {
    rpc_top_level(|mut core, client| {
        let response = try!(core.run(client.test_more_stuff_request().send().promise));
        let client = try!(try!(response.get()).get_cap());

        let server1 = ::impls::TestInterface::new();
        let call_count1 = server1.get_call_count();
        assert_eq!(call_count1.get(), 0);
        let client1 = ::test_capnp::test_interface::ToClient::new(server1)
            .from_server::<::capnp_rpc::Server>();

        let server2 = ::impls::TestInterface::new();
        let call_count2 = server2.get_call_count();
        assert_eq!(call_count2.get(), 0);
        let client2 = ::test_capnp::test_interface::ToClient::new(server2)
            .from_server::<::capnp_rpc::Server>();

        let mut request = client.call_each_capability_request();
        {
            let mut caps = request.get().init_caps(2);
            caps.set(0, client1.client.hook);
            caps.set(1, client2.client.hook);
        }
        try!(core.run(request.send().promise));
        assert_eq!(call_count1.get(), 1);
        assert_eq!(call_count2.get(), 1);
        Ok(())
    })
}
