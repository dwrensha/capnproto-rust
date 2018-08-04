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

use test_capnp::{
    bootstrap, test_call_order, test_extends, test_handle, test_interface, test_more_stuff,
    test_pipeline,
};

use capnp::capability::Promise;
use capnp::Error;

use futures::Future;

use std::cell::Cell;
use std::rc::Rc;

pub struct Bootstrap;

impl bootstrap::Server for Bootstrap {
    fn test_interface(
        &mut self,
        _params: bootstrap::TestInterfaceParams,
        mut results: bootstrap::TestInterfaceResults,
    ) -> Promise<(), Error> {
        {
            results.get().set_cap(
                test_interface::ToClient::new(TestInterface::new())
                    .from_server::<::capnp_rpc::Server>(),
            );
        }
        Promise::ok(())
    }

    fn test_extends(
        &mut self,
        _params: bootstrap::TestExtendsParams,
        mut results: bootstrap::TestExtendsResults,
    ) -> Promise<(), Error> {
        {
            results.get().set_cap(
                test_extends::ToClient::new(TestExtends).from_server::<::capnp_rpc::Server>(),
            );
        }
        Promise::ok(())
    }

    fn test_extends2(
        &mut self,
        _params: bootstrap::TestExtends2Params,
        _results: bootstrap::TestExtends2Results,
    ) -> Promise<(), Error> {
        unimplemented!()
    }

    fn test_pipeline(
        &mut self,
        _params: bootstrap::TestPipelineParams,
        mut results: bootstrap::TestPipelineResults,
    ) -> Promise<(), Error> {
        {
            results.get().set_cap(
                test_pipeline::ToClient::new(TestPipeline).from_server::<::capnp_rpc::Server>(),
            );
        }
        Promise::ok(())
    }

    fn test_call_order(
        &mut self,
        _params: bootstrap::TestCallOrderParams,
        mut results: bootstrap::TestCallOrderResults,
    ) -> Promise<(), Error> {
        {
            results.get().set_cap(
                test_call_order::ToClient::new(TestCallOrder::new())
                    .from_server::<::capnp_rpc::Server>(),
            );
        }
        Promise::ok(())
    }
    fn test_more_stuff(
        &mut self,
        _params: bootstrap::TestMoreStuffParams,
        mut results: bootstrap::TestMoreStuffResults,
    ) -> Promise<(), Error> {
        {
            results.get().set_cap(
                test_more_stuff::ToClient::new(TestMoreStuff::new())
                    .from_server::<::capnp_rpc::Server>(),
            );
        }
        Promise::ok(())
    }
}

pub struct TestInterface {
    call_count: Rc<Cell<u64>>,
}

impl TestInterface {
    pub fn new() -> TestInterface {
        TestInterface {
            call_count: Rc::new(Cell::new(0)),
        }
    }
    pub fn get_call_count(&self) -> Rc<Cell<u64>> {
        self.call_count.clone()
    }
    fn increment_call_count(&self) {
        self.call_count.set(self.call_count.get() + 1);
    }
}

impl test_interface::Server for TestInterface {
    fn foo(
        &mut self,
        params: test_interface::FooParams,
        mut results: test_interface::FooResults,
    ) -> Promise<(), Error> {
        self.increment_call_count();
        let params = pry!(params.get());
        if params.get_i() != 123 {
            return Promise::err(Error::failed(format!("expected i to equal 123")));
        }
        if !params.get_j() {
            return Promise::err(Error::failed(format!("expected j to be true")));
        }
        {
            let mut results = results.get();
            results.set_x("foo");
        }
        Promise::ok(())
    }

    fn bar(
        &mut self,
        _params: test_interface::BarParams,
        _results: test_interface::BarResults,
    ) -> Promise<(), Error> {
        self.increment_call_count();
        Promise::err(Error::unimplemented("bar is not implemented".to_string()))
    }

    fn baz(
        &mut self,
        params: test_interface::BazParams,
        _results: test_interface::BazResults,
    ) -> Promise<(), Error> {
        self.increment_call_count();
        ::test_util::CheckTestMessage::check_test_message(pry!(pry!(params.get()).get_s()));
        Promise::ok(())
    }
}

struct TestExtends;

impl test_interface::Server for TestExtends {
    fn foo(
        &mut self,
        params: test_interface::FooParams,
        mut results: test_interface::FooResults,
    ) -> Promise<(), Error> {
        let params = pry!(params.get());
        if params.get_i() != 321 {
            return Promise::err(Error::failed(format!("expected i to equal 321")));
        }
        if params.get_j() {
            return Promise::err(Error::failed(format!("expected j to be false")));
        }
        {
            let mut results = results.get();
            results.set_x("bar");
        }
        Promise::ok(())
    }

    fn bar(
        &mut self,
        _params: test_interface::BarParams,
        _results: test_interface::BarResults,
    ) -> Promise<(), Error> {
        Promise::err(Error::unimplemented("bar is not implemented".to_string()))
    }

    fn baz(
        &mut self,
        _params: test_interface::BazParams,
        _results: test_interface::BazResults,
    ) -> Promise<(), Error> {
        Promise::err(Error::unimplemented("baz is not implemented".to_string()))
    }
}

impl test_extends::Server for TestExtends {
    fn qux(
        &mut self,
        _params: test_extends::QuxParams,
        _results: test_extends::QuxResults,
    ) -> Promise<(), Error> {
        Promise::err(Error::unimplemented("qux is not implemented".to_string()))
    }

    fn corge(
        &mut self,
        _params: test_extends::CorgeParams,
        _results: test_extends::CorgeResults,
    ) -> Promise<(), Error> {
        Promise::err(Error::unimplemented("corge is not implemented".to_string()))
    }

    fn grault(
        &mut self,
        _params: test_extends::GraultParams,
        mut results: test_extends::GraultResults,
    ) -> Promise<(), Error> {
        ::test_util::init_test_message(results.get());
        Promise::ok(())
    }
}

struct TestPipeline;

impl test_pipeline::Server for TestPipeline {
    fn get_cap(
        &mut self,
        params: test_pipeline::GetCapParams,
        mut results: test_pipeline::GetCapResults,
    ) -> Promise<(), Error> {
        if pry!(params.get()).get_n() != 234 {
            return Promise::err(Error::failed("expected n to equal 234".to_string()));
        }
        let cap = pry!(pry!(params.get()).get_in_cap());
        let mut request = cap.foo_request();
        request.get().set_i(123);
        request.get().set_j(true);
        Promise::from_future(request.send().promise.and_then(move |response| {
            if try!(try!(response.get()).get_x()) != "foo" {
                return Err(Error::failed("expected x to equal 'foo'".to_string()));
            }

            results.get().set_s("bar");

            // TODO implement better casting
            results
                .get()
                .init_out_box()
                .set_cap(test_interface::Client {
                    client: test_extends::ToClient::new(TestExtends)
                        .from_server::<::capnp_rpc::Server>()
                        .client,
                });
            Ok(())
        }))
    }

    fn get_null_cap(
        &mut self,
        _params: test_pipeline::GetNullCapParams,
        _results: test_pipeline::GetNullCapResults,
    ) -> Promise<(), Error> {
        Promise::ok(())
    }
}

pub struct TestCallOrder {
    count: u32,
}

impl TestCallOrder {
    pub fn new() -> TestCallOrder {
        TestCallOrder { count: 0 }
    }
}

impl test_call_order::Server for TestCallOrder {
    fn get_call_sequence(
        &mut self,
        _params: test_call_order::GetCallSequenceParams,
        mut results: test_call_order::GetCallSequenceResults,
    ) -> Promise<(), Error> {
        results.get().set_n(self.count);
        self.count += 1;
        Promise::ok(())
    }
}

pub struct TestMoreStuff {
    call_count: u32,
    handle_count: Rc<Cell<i64>>,
    client_to_hold: Option<test_interface::Client>,
}

impl TestMoreStuff {
    pub fn new() -> TestMoreStuff {
        TestMoreStuff {
            call_count: 0,
            handle_count: Rc::new(Cell::new(0)),
            client_to_hold: None,
        }
    }
    /*
    pub fn get_call_count(&self) -> Rc<Cell<u64>> {
        self.call_count.clone()
    }
    fn increment_call_count(&self) {
        self.call_count.set(self.call_count.get() + 1);
    } */
}

impl test_call_order::Server for TestMoreStuff {
    fn get_call_sequence(
        &mut self,
        _params: test_call_order::GetCallSequenceParams,
        mut results: test_call_order::GetCallSequenceResults,
    ) -> Promise<(), Error> {
        results.get().set_n(self.call_count);
        self.call_count += 1;
        Promise::ok(())
    }
}

impl test_more_stuff::Server for TestMoreStuff {
    fn call_foo(
        &mut self,
        params: test_more_stuff::CallFooParams,
        mut results: test_more_stuff::CallFooResults,
    ) -> Promise<(), Error> {
        self.call_count += 1;
        let cap = pry!(pry!(params.get()).get_cap());
        let mut request = cap.foo_request();
        request.get().set_i(123);
        request.get().set_j(true);

        Promise::from_future(request.send().promise.and_then(move |response| {
            if try!(try!(response.get()).get_x()) != "foo" {
                return Err(Error::failed("expected x to equal 'foo'".to_string()));
            }
            results.get().set_s("bar");
            Ok(())
        }))
    }

    fn call_foo_when_resolved(
        &mut self,
        params: test_more_stuff::CallFooWhenResolvedParams,
        mut results: test_more_stuff::CallFooWhenResolvedResults,
    ) -> Promise<(), Error> {
        self.call_count += 1;
        let cap = pry!(pry!(params.get()).get_cap());
        Promise::from_future(cap.client.when_resolved().and_then(move |()| {
            let mut request = cap.foo_request();
            request.get().set_i(123);
            request.get().set_j(true);
            request.send().promise.and_then(move |response| {
                if try!(try!(response.get()).get_x()) != "foo" {
                    return Err(Error::failed("expected x to equal 'foo'".to_string()));
                }
                results.get().set_s("bar");
                Ok(())
            })
        }))
    }

    fn never_return(
        &mut self,
        params: test_more_stuff::NeverReturnParams,
        mut results: test_more_stuff::NeverReturnResults,
    ) -> Promise<(), Error> {
        self.call_count += 1;

        let cap = pry!(pry!(params.get()).get_cap());

        // Attach `cap` to the promise to make sure it is released.
        let attached = cap.clone();
        let promise = Promise::from_future(::futures::future::empty().map(|()| {
            drop(attached);
        }));

        // Also attach `cap` to the result struct so we can make sure that the results are released.
        results.get().set_cap_copy(cap);

        promise
    }

    fn hold(
        &mut self,
        params: test_more_stuff::HoldParams,
        _results: test_more_stuff::HoldResults,
    ) -> Promise<(), Error> {
        self.call_count += 1;
        self.client_to_hold = Some(pry!(pry!(params.get()).get_cap()));
        Promise::ok(())
    }

    fn dont_hold(
        &mut self,
        params: test_more_stuff::DontHoldParams,
        _results: test_more_stuff::DontHoldResults,
    ) -> Promise<(), Error> {
        self.call_count += 1;
        let _ = Some(pry!(pry!(params.get()).get_cap()));
        Promise::ok(())
    }

    fn call_held(
        &mut self,
        _params: test_more_stuff::CallHeldParams,
        mut results: test_more_stuff::CallHeldResults,
    ) -> Promise<(), Error> {
        self.call_count += 1;
        match self.client_to_hold {
            None => Promise::err(Error::failed("no held client".to_string())),
            Some(ref client) => {
                let mut request = client.foo_request();
                {
                    let mut params = request.get();
                    params.set_i(123);
                    params.set_j(true);
                }
                Promise::from_future(request.send().promise.and_then(move |response| {
                    if try!(try!(response.get()).get_x()) != "foo" {
                        Err(Error::failed("expected X to equal 'foo'".to_string()))
                    } else {
                        results.get().set_s("bar");
                        Ok(())
                    }
                }))
            }
        }
    }

    fn get_held(
        &mut self,
        _params: test_more_stuff::GetHeldParams,
        mut results: test_more_stuff::GetHeldResults,
    ) -> Promise<(), Error> {
        self.call_count += 1;
        match self.client_to_hold {
            None => Promise::err(Error::failed("no held client".to_string())),
            Some(ref client) => {
                results.get().set_cap(client.clone());
                Promise::ok(())
            }
        }
    }

    fn echo(
        &mut self,
        params: test_more_stuff::EchoParams,
        mut results: test_more_stuff::EchoResults,
    ) -> Promise<(), Error> {
        self.call_count += 1;
        results.get().set_cap(pry!(pry!(params.get()).get_cap()));
        Promise::ok(())
    }

    fn expect_cancel(
        &mut self,
        _params: test_more_stuff::ExpectCancelParams,
        _results: test_more_stuff::ExpectCancelResults,
    ) -> Promise<(), Error> {
        unimplemented!()
    }

    fn get_handle(
        &mut self,
        _params: test_more_stuff::GetHandleParams,
        mut results: test_more_stuff::GetHandleResults,
    ) -> Promise<(), Error> {
        self.call_count += 1;
        let handle = Handle::new(&self.handle_count);
        results
            .get()
            .set_handle(test_handle::ToClient::new(handle).from_server::<::capnp_rpc::Server>());
        Promise::ok(())
    }

    fn get_handle_count(
        &mut self,
        _params: test_more_stuff::GetHandleCountParams,
        mut results: test_more_stuff::GetHandleCountResults,
    ) -> Promise<(), Error> {
        self.call_count += 1;
        results.get().set_count(self.handle_count.get());
        Promise::ok(())
    }

    fn get_null(
        &mut self,
        _params: test_more_stuff::GetNullParams,
        _results: test_more_stuff::GetNullResults,
    ) -> Promise<(), Error> {
        unimplemented!()
    }

    fn method_with_defaults(
        &mut self,
        _params: test_more_stuff::MethodWithDefaultsParams,
        _results: test_more_stuff::MethodWithDefaultsResults,
    ) -> Promise<(), Error> {
        unimplemented!()
    }

    fn call_each_capability(
        &mut self,
        params: test_more_stuff::CallEachCapabilityParams,
        _results: test_more_stuff::CallEachCapabilityResults,
    ) -> Promise<(), Error> {
        let mut results = Vec::new();
        for cap in pry!(pry!(params.get()).get_caps()) {
            let mut request = pry!(cap).foo_request();
            request.get().set_i(123);
            request.get().set_j(true);
            results.push(request.send().promise);
        }

        Promise::from_future(::futures::future::join_all(results).map(|_| ()))
    }
}

struct Handle {
    count: Rc<Cell<i64>>,
}

impl Handle {
    fn new(count: &Rc<Cell<i64>>) -> Handle {
        let count = count.clone();
        count.set(count.get() + 1);
        Handle { count: count }
    }
}

impl Drop for Handle {
    fn drop(&mut self) {
        self.count.set(self.count.get() - 1);
    }
}

impl test_handle::Server for Handle {}

pub struct TestCapDestructor {
    fulfiller: Option<::futures::sync::oneshot::Sender<()>>,
    imp: TestInterface,
}

impl TestCapDestructor {
    pub fn new(fulfiller: ::futures::sync::oneshot::Sender<()>) -> TestCapDestructor {
        TestCapDestructor {
            fulfiller: Some(fulfiller),
            imp: TestInterface::new(),
        }
    }
}

impl Drop for TestCapDestructor {
    fn drop(&mut self) {
        match self.fulfiller.take() {
            Some(f) => {
                let _ = f.send(());
            }
            None => (),
        }
    }
}

impl test_interface::Server for TestCapDestructor {
    fn foo(
        &mut self,
        params: test_interface::FooParams,
        results: test_interface::FooResults,
    ) -> Promise<(), Error> {
        self.imp.foo(params, results)
    }

    fn bar(
        &mut self,
        _params: test_interface::BarParams,
        _results: test_interface::BarResults,
    ) -> Promise<(), Error> {
        Promise::err(Error::unimplemented("bar is not implemented".to_string()))
    }

    fn baz(
        &mut self,
        _params: test_interface::BazParams,
        _results: test_interface::BazResults,
    ) -> Promise<(), Error> {
        Promise::err(Error::unimplemented("bar is not implemented".to_string()))
    }
}
