// Copyright (c) 2013-2017 Sandstorm Development Group, Inc. and contributors
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

use capnp::capability::{self, Promise};
use capnp::private::capability::{
    ClientHook, ParamsHook, PipelineHook, PipelineOp, RequestHook, ResponseHook, ResultsHook,
};
use capnp::traits::{Imbue, ImbueMut};
use capnp::Error;
use capnp::{any_pointer, message};

use attach::Attach;
use futures::sync::oneshot;
use futures::Future;

use std::cell::RefCell;
use std::mem;
use std::rc::Rc;

pub trait ResultsDoneHook {
    fn add_ref(&self) -> Box<ResultsDoneHook>;
    fn get<'a>(&'a self) -> ::capnp::Result<any_pointer::Reader<'a>>;
}

impl Clone for Box<ResultsDoneHook> {
    fn clone(&self) -> Box<ResultsDoneHook> {
        self.add_ref()
    }
}

pub struct Response {
    results: Box<ResultsDoneHook>,
}

impl Response {
    fn new(results: Box<ResultsDoneHook>) -> Response {
        Response { results: results }
    }
}

impl ResponseHook for Response {
    fn get<'a>(&'a self) -> ::capnp::Result<any_pointer::Reader<'a>> {
        self.results.get()
    }
}

struct Params {
    request: message::Builder<message::HeapAllocator>,
    cap_table: Vec<Option<Box<ClientHook>>>,
}

impl Params {
    fn new(
        request: message::Builder<message::HeapAllocator>,
        cap_table: Vec<Option<Box<ClientHook>>>,
    ) -> Params {
        Params {
            request: request,
            cap_table: cap_table,
        }
    }
}

impl ParamsHook for Params {
    fn get<'a>(&'a self) -> ::capnp::Result<any_pointer::Reader<'a>> {
        let mut result: any_pointer::Reader = try!(self.request.get_root_as_reader());
        result.imbue(&self.cap_table);
        Ok(result)
    }
}

struct Results {
    message: Option<message::Builder<message::HeapAllocator>>,
    cap_table: Vec<Option<Box<ClientHook>>>,
    results_done_fulfiller: Option<oneshot::Sender<Box<ResultsDoneHook>>>,
}

impl Results {
    fn new(fulfiller: oneshot::Sender<Box<ResultsDoneHook>>) -> Results {
        Results {
            message: Some(::capnp::message::Builder::new_default()),
            cap_table: Vec::new(),
            results_done_fulfiller: Some(fulfiller),
        }
    }
}

impl Drop for Results {
    fn drop(&mut self) {
        if let (Some(message), Some(fulfiller)) =
            (self.message.take(), self.results_done_fulfiller.take())
        {
            let cap_table = mem::replace(&mut self.cap_table, Vec::new());
            let _ = fulfiller.send(Box::new(ResultsDone::new(message, cap_table)));
        } else {
            unreachable!()
        }
    }
}

impl ResultsHook for Results {
    fn get<'a>(&'a mut self) -> ::capnp::Result<any_pointer::Builder<'a>> {
        match *self {
            Results {
                message: Some(ref mut message),
                ref mut cap_table,
                ..
            } => {
                let mut result: any_pointer::Builder = try!(message.get_root());
                result.imbue_mut(cap_table);
                Ok(result)
            }
            _ => unreachable!(),
        }
    }

    fn tail_call(self: Box<Self>, _request: Box<RequestHook>) -> Promise<(), Error> {
        unimplemented!()
    }

    fn direct_tail_call(
        self: Box<Self>,
        _request: Box<RequestHook>,
    ) -> (Promise<(), Error>, Box<PipelineHook>) {
        unimplemented!()
    }

    fn allow_cancellation(&self) {
        unimplemented!()
    }
}

struct ResultsDoneInner {
    message: ::capnp::message::Builder<::capnp::message::HeapAllocator>,
    cap_table: Vec<Option<Box<ClientHook>>>,
}

struct ResultsDone {
    inner: Rc<ResultsDoneInner>,
}

impl ResultsDone {
    fn new(
        message: message::Builder<message::HeapAllocator>,
        cap_table: Vec<Option<Box<ClientHook>>>,
    ) -> ResultsDone {
        ResultsDone {
            inner: Rc::new(ResultsDoneInner {
                message: message,
                cap_table: cap_table,
            }),
        }
    }
}

impl ResultsDoneHook for ResultsDone {
    fn add_ref(&self) -> Box<ResultsDoneHook> {
        Box::new(ResultsDone {
            inner: self.inner.clone(),
        })
    }
    fn get<'a>(&'a self) -> ::capnp::Result<any_pointer::Reader<'a>> {
        let mut result: any_pointer::Reader = try!(self.inner.message.get_root_as_reader());
        result.imbue(&self.inner.cap_table);
        Ok(result)
    }
}

pub struct Request {
    message: message::Builder<::capnp::message::HeapAllocator>,
    cap_table: Vec<Option<Box<ClientHook>>>,
    interface_id: u64,
    method_id: u16,
    client: Box<ClientHook>,
}

impl Request {
    pub fn new(
        interface_id: u64,
        method_id: u16,
        _size_hint: Option<::capnp::MessageSize>,
        client: Box<ClientHook>,
    ) -> Request {
        Request {
            message: message::Builder::new_default(),
            cap_table: Vec::new(),
            interface_id: interface_id,
            method_id: method_id,
            client: client,
        }
    }
}

impl RequestHook for Request {
    fn get<'a>(&'a mut self) -> any_pointer::Builder<'a> {
        let mut result: any_pointer::Builder = self.message.get_root().unwrap();
        result.imbue_mut(&mut self.cap_table);
        result
    }
    fn get_brand(&self) -> usize {
        0
    }
    fn send<'a>(self: Box<Self>) -> capability::RemotePromise<any_pointer::Owned> {
        let tmp = *self;
        let Request {
            message,
            cap_table,
            interface_id,
            method_id,
            client,
        } = tmp;
        let params = Params::new(message, cap_table);

        let (results_done_fulfiller, results_done_promise) =
            oneshot::channel::<Box<ResultsDoneHook>>();
        let results_done_promise = results_done_promise.map_err(|e| e.into());
        let results = Results::new(results_done_fulfiller);
        let promise = client.call(interface_id, method_id, Box::new(params), Box::new(results));

        let (pipeline_sender, mut pipeline) = ::queued::Pipeline::new();

        let p = promise
            .join(results_done_promise)
            .and_then(move |((), results_done_hook)| {
                pipeline_sender.complete(
                    Box::new(Pipeline::new(results_done_hook.add_ref())) as Box<PipelineHook>
                );
                Ok((
                    capability::Response::new(Box::new(Response::new(results_done_hook))),
                    (),
                ))
            });

        let (left, right) = ::split::split(p);

        pipeline.drive(right);
        let pipeline = any_pointer::Pipeline::new(Box::new(pipeline));

        capability::RemotePromise {
            promise: Promise::from_future(left),
            pipeline: pipeline,
        }
    }
    fn tail_send(self: Box<Self>) -> Option<(u32, Promise<(), Error>, Box<PipelineHook>)> {
        unimplemented!()
    }
}

struct PipelineInner {
    results: Box<ResultsDoneHook>,
}

pub struct Pipeline {
    inner: Rc<RefCell<PipelineInner>>,
}

impl Pipeline {
    pub fn new(results: Box<ResultsDoneHook>) -> Pipeline {
        Pipeline {
            inner: Rc::new(RefCell::new(PipelineInner { results: results })),
        }
    }
}

impl Clone for Pipeline {
    fn clone(&self) -> Pipeline {
        Pipeline {
            inner: self.inner.clone(),
        }
    }
}

impl PipelineHook for Pipeline {
    fn add_ref(&self) -> Box<PipelineHook> {
        Box::new(self.clone())
    }
    fn get_pipelined_cap(&self, ops: &[PipelineOp]) -> Box<ClientHook> {
        match self
            .inner
            .borrow_mut()
            .results
            .get()
            .unwrap()
            .get_pipelined_cap(ops)
        {
            Ok(v) => v,
            Err(e) => Box::new(::broken::Client::new(e, true, 0)) as Box<ClientHook>,
        }
    }
}

struct ClientInner {
    server: Box<capability::Server>,
}

pub struct Client {
    inner: Rc<RefCell<ClientInner>>,
}

impl Client {
    pub fn new(server: Box<capability::Server>) -> Client {
        Client {
            inner: Rc::new(RefCell::new(ClientInner { server: server })),
        }
    }
}

impl Clone for Client {
    fn clone(&self) -> Client {
        Client {
            inner: self.inner.clone(),
        }
    }
}

impl ClientHook for Client {
    fn add_ref(&self) -> Box<ClientHook> {
        Box::new(self.clone())
    }
    fn new_call(
        &self,
        interface_id: u64,
        method_id: u16,
        size_hint: Option<::capnp::MessageSize>,
    ) -> capability::Request<any_pointer::Owned, any_pointer::Owned> {
        capability::Request::new(Box::new(Request::new(
            interface_id,
            method_id,
            size_hint,
            self.add_ref(),
        )))
    }

    fn call(
        &self,
        interface_id: u64,
        method_id: u16,
        params: Box<ParamsHook>,
        results: Box<ResultsHook>,
    ) -> Promise<(), Error> {
        // We don't want to actually dispatch the call synchronously, because we don't want the callee
        // to have any side effects before the promise is returned to the caller.  This helps avoid
        // race conditions.
        //
        // TODO: actually use some kind of queue here to guarantee that call order in maintained.
        // This currently relies on the task scheduler being first-in-first-out.
        let inner = self.inner.clone();
        let promise = ::futures::future::lazy(move || {
            let server = &mut inner.borrow_mut().server;
            server.dispatch_call(
                interface_id,
                method_id,
                ::capnp::capability::Params::new(params),
                ::capnp::capability::Results::new(results),
            )
        }).attach(self.add_ref());

        Promise::from_future(promise)
    }

    fn get_ptr(&self) -> usize {
        (&*self.inner.borrow()) as *const _ as usize
    }

    fn get_brand(&self) -> usize {
        0
    }

    fn get_resolved(&self) -> Option<Box<ClientHook>> {
        None
    }

    fn when_more_resolved(&self) -> Option<Promise<Box<ClientHook>, Error>> {
        None
    }
}
