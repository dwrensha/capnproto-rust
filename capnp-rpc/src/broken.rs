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

use capnp::any_pointer;
use capnp::private::capability::{
    ClientHook, ParamsHook, PipelineHook, PipelineOp, RequestHook, ResultsHook,
};
use capnp::Error;

use capnp::capability::{Promise, RemotePromise};

use std::rc::Rc;

pub struct Pipeline {
    error: Error,
}

impl Pipeline {
    pub fn new(error: Error) -> Pipeline {
        Pipeline { error: error }
    }
}

impl PipelineHook for Pipeline {
    fn add_ref(&self) -> Box<PipelineHook> {
        Box::new(Pipeline::new(self.error.clone()))
    }
    fn get_pipelined_cap(&self, _ops: &[PipelineOp]) -> Box<ClientHook> {
        new_cap(self.error.clone())
    }
}

pub struct Request {
    error: Error,
    message: ::capnp::message::Builder<::capnp::message::HeapAllocator>,
}

impl Request {
    pub fn new(error: Error, _size_hint: Option<::capnp::MessageSize>) -> Request {
        Request {
            error: error,
            message: ::capnp::message::Builder::new_default(),
        }
    }
}

impl RequestHook for Request {
    fn get<'a>(&'a mut self) -> any_pointer::Builder<'a> {
        self.message.get_root().unwrap()
    }
    fn get_brand(&self) -> usize {
        0
    }
    fn send<'a>(self: Box<Self>) -> RemotePromise<any_pointer::Owned> {
        let pipeline = Pipeline::new(self.error.clone());
        RemotePromise {
            promise: Promise::err(self.error),
            pipeline: any_pointer::Pipeline::new(Box::new(pipeline)),
        }
    }
    fn tail_send(self: Box<Self>) -> Option<(u32, Promise<(), Error>, Box<PipelineHook>)> {
        None
    }
}

struct ClientInner {
    error: Error,
    _resolved: bool,
    brand: usize,
}

pub struct Client {
    inner: Rc<ClientInner>,
}

impl Client {
    pub fn new(error: Error, resolved: bool, brand: usize) -> Client {
        Client {
            inner: Rc::new(ClientInner {
                error: error,
                _resolved: resolved,
                brand: brand,
            }),
        }
    }
}

impl ClientHook for Client {
    fn add_ref(&self) -> Box<ClientHook> {
        Box::new(Client {
            inner: self.inner.clone(),
        })
    }
    fn new_call(
        &self,
        _interface_id: u64,
        _method_id: u16,
        size_hint: Option<::capnp::MessageSize>,
    ) -> ::capnp::capability::Request<any_pointer::Owned, any_pointer::Owned> {
        ::capnp::capability::Request::new(Box::new(Request::new(
            self.inner.error.clone(),
            size_hint,
        )))
    }

    fn call(
        &self,
        _interface_id: u64,
        _method_id: u16,
        _params: Box<ParamsHook>,
        _results: Box<ResultsHook>,
    ) -> Promise<(), Error> {
        Promise::err(self.inner.error.clone())
    }

    fn get_ptr(&self) -> usize {
        (self.inner.as_ref()) as *const _ as usize
    }

    fn get_brand(&self) -> usize {
        self.inner.brand
    }

    fn get_resolved(&self) -> Option<Box<ClientHook>> {
        None
    }

    fn when_more_resolved(&self) -> Option<Promise<Box<ClientHook>, Error>> {
        None
    }
}

pub fn new_cap(exception: Error) -> Box<ClientHook> {
    Box::new(Client::new(exception, false, 0))
}
