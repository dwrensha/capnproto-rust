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

use capnp::primitive_list;
use capnp::Error;

use capnp_rpc::{rpc_twoparty_capnp, twoparty, RpcSystem};

use calculator_capnp::calculator;
use capnp::capability::Promise;

use futures::{Future, Stream};
use tokio_io::AsyncRead;

struct ValueImpl {
    value: f64,
}

impl ValueImpl {
    fn new(value: f64) -> ValueImpl {
        ValueImpl { value: value }
    }
}

impl calculator::value::Server for ValueImpl {
    fn read(
        &mut self,
        _params: calculator::value::ReadParams,
        mut results: calculator::value::ReadResults,
    ) -> Promise<(), Error> {
        results.get().set_value(self.value);
        Promise::ok(())
    }
}

fn evaluate_impl(
    expression: calculator::expression::Reader,
    params: Option<primitive_list::Reader<f64>>,
) -> Promise<f64, Error> {
    match pry!(expression.which()) {
        calculator::expression::Literal(v) => Promise::ok(v),
        calculator::expression::PreviousResult(p) => Promise::from_future(
            pry!(p)
                .read_request()
                .send()
                .promise
                .and_then(|v| Ok(try!(v.get()).get_value())),
        ),
        calculator::expression::Parameter(p) => match params {
            Some(params) if p < params.len() => Promise::ok(params.get(p)),
            _ => Promise::err(Error::failed(format!("bad parameter: {}", p))),
        },
        calculator::expression::Call(call) => {
            let func = pry!(call.get_function());
            let param_promises: Vec<Promise<f64, Error>> = pry!(call.get_params())
                .iter()
                .map(|p| evaluate_impl(p, params))
                .collect();
            // XXX shouldn't need to collect()
            // see https://github.com/alexcrichton/futures-rs/issues/285

            Promise::from_future(::futures::future::join_all(param_promises).and_then(
                move |param_values| {
                    let mut request = func.call_request();
                    {
                        let mut params = request.get().init_params(param_values.len() as u32);
                        for ii in 0..param_values.len() {
                            params.set(ii as u32, param_values[ii]);
                        }
                    }
                    request
                        .send()
                        .promise
                        .and_then(|result| Ok(try!(result.get()).get_value()))
                },
            ))
        }
    }
}

struct FunctionImpl {
    param_count: u32,
    body: ::capnp::message::Builder<::capnp::message::HeapAllocator>,
}

impl FunctionImpl {
    fn new(
        param_count: u32,
        body: calculator::expression::Reader,
    ) -> ::capnp::Result<FunctionImpl> {
        let mut result = FunctionImpl {
            param_count: param_count,
            body: ::capnp::message::Builder::new_default(),
        };
        try!(result.body.set_root(body));
        Ok(result)
    }
}

impl calculator::function::Server for FunctionImpl {
    fn call(
        &mut self,
        params: calculator::function::CallParams,
        mut results: calculator::function::CallResults,
    ) -> Promise<(), Error> {
        let params = pry!(pry!(params.get()).get_params());
        if params.len() != self.param_count {
            Promise::err(Error::failed(format!(
                "Expect {} parameters but got {}.",
                self.param_count,
                params.len()
            )))
        } else {
            Promise::from_future(
                evaluate_impl(
                    pry!(self.body.get_root::<calculator::expression::Builder>()).as_reader(),
                    Some(params),
                ).map(move |v| {
                    results.get().set_value(v);
                }),
            )
        }
    }
}

#[derive(Clone, Copy)]
pub struct OperatorImpl {
    op: calculator::Operator,
}

impl calculator::function::Server for OperatorImpl {
    fn call(
        &mut self,
        params: calculator::function::CallParams,
        mut results: calculator::function::CallResults,
    ) -> Promise<(), Error> {
        let params = pry!(pry!(params.get()).get_params());
        if params.len() != 2 {
            Promise::err(Error::failed("Wrong number of paramters.".to_string()))
        } else {
            let v = match self.op {
                calculator::Operator::Add => params.get(0) + params.get(1),
                calculator::Operator::Subtract => params.get(0) - params.get(1),
                calculator::Operator::Multiply => params.get(0) * params.get(1),
                calculator::Operator::Divide => params.get(0) / params.get(1),
            };
            results.get().set_value(v);
            Promise::ok(())
        }
    }
}

struct CalculatorImpl;

impl calculator::Server for CalculatorImpl {
    fn evaluate(
        &mut self,
        params: calculator::EvaluateParams,
        mut results: calculator::EvaluateResults,
    ) -> Promise<(), Error> {
        Promise::from_future(
            evaluate_impl(pry!(pry!(params.get()).get_expression()), None).map(move |v| {
                results.get().set_value(
                    calculator::value::ToClient::new(ValueImpl::new(v))
                        .from_server::<::capnp_rpc::Server>(),
                );
            }),
        )
    }
    fn def_function(
        &mut self,
        params: calculator::DefFunctionParams,
        mut results: calculator::DefFunctionResults,
    ) -> Promise<(), Error> {
        results.get().set_func(
            calculator::function::ToClient::new(pry!(FunctionImpl::new(
                pry!(params.get()).get_param_count() as u32,
                pry!(pry!(params.get()).get_body())
            ))).from_server::<::capnp_rpc::Server>(),
        );
        Promise::ok(())
    }
    fn get_operator(
        &mut self,
        params: calculator::GetOperatorParams,
        mut results: calculator::GetOperatorResults,
    ) -> Promise<(), Error> {
        let op = pry!(pry!(params.get()).get_op());
        results.get().set_func(
            calculator::function::ToClient::new(OperatorImpl { op: op })
                .from_server::<::capnp_rpc::Server>(),
        );
        Promise::ok(())
    }
}

pub fn main() {
    use std::net::ToSocketAddrs;
    let args: Vec<String> = ::std::env::args().collect();
    if args.len() != 3 {
        println!("usage: {} server ADDRESS[:PORT]", args[0]);
        return;
    }

    let mut core = ::tokio_core::reactor::Core::new().unwrap();
    let handle = core.handle();

    let addr = args[2]
        .to_socket_addrs()
        .unwrap()
        .next()
        .expect("could not parse address");
    let socket = ::tokio_core::net::TcpListener::bind(&addr, &handle).unwrap();

    let calc = calculator::ToClient::new(CalculatorImpl).from_server::<::capnp_rpc::Server>();

    let done = socket.incoming().for_each(move |(socket, _addr)| {
        try!(socket.set_nodelay(true));
        let (reader, writer) = socket.split();

        let handle = handle.clone();

        let network = twoparty::VatNetwork::new(
            reader,
            writer,
            rpc_twoparty_capnp::Side::Server,
            Default::default(),
        );

        let rpc_system = RpcSystem::new(Box::new(network), Some(calc.clone().client));
        handle.spawn(rpc_system.map_err(|e| println!("error: {:?}", e)));
        Ok(())
    });

    core.run(done).unwrap();
}
