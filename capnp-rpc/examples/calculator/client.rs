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

use calculator_capnp::calculator;
use capnp::capability::Promise;
use capnp_rpc::{rpc_twoparty_capnp, twoparty, RpcSystem};

use futures::Future;
use tokio_io::AsyncRead;

#[derive(Clone, Copy)]
pub struct PowerFunction;

impl calculator::function::Server for PowerFunction {
    fn call(
        &mut self,
        params: calculator::function::CallParams,
        mut results: calculator::function::CallResults,
    ) -> Promise<(), ::capnp::Error> {
        let params = pry!(pry!(params.get()).get_params());
        if params.len() != 2 {
            Promise::err(::capnp::Error::failed(
                "Wrong number of parameters".to_string(),
            ))
        } else {
            results.get().set_value(params.get(0).powf(params.get(1)));
            Promise::ok(())
        }
    }
}

pub fn main() {
    let args: Vec<String> = ::std::env::args().collect();
    if args.len() != 3 {
        println!("usage: {} client HOST:PORT", args[0]);
        return;
    }

    try_main(args).unwrap();
}

fn try_main(args: Vec<String>) -> Result<(), ::capnp::Error> {
    use std::net::ToSocketAddrs;

    let mut core = try!(::tokio_core::reactor::Core::new());
    let handle = core.handle();

    let addr = try!(args[2].to_socket_addrs())
        .next()
        .expect("could not parse address");
    let stream = core
        .run(::tokio_core::net::TcpStream::connect(&addr, &handle))
        .unwrap();
    try!(stream.set_nodelay(true));
    let (reader, writer) = stream.split();

    let network = Box::new(twoparty::VatNetwork::new(
        reader,
        writer,
        rpc_twoparty_capnp::Side::Client,
        Default::default(),
    ));
    let mut rpc_system = RpcSystem::new(network, None);
    let calculator: calculator::Client = rpc_system.bootstrap(rpc_twoparty_capnp::Side::Server);
    handle.spawn(rpc_system.map_err(|_e| ()));

    {
        // Make a request that just evaluates the literal value 123.
        //
        // What's interesting here is that evaluate() returns a "Value", which is
        // another interface and therefore points back to an object living on the
        // server.  We then have to call read() on that object to read it.
        // However, even though we are making two RPC's, this block executes in
        // *one* network round trip because of promise pipelining:  we do not wait
        // for the first call to complete before we send the second call to the
        // server.

        println!("Evaluating a literal...");
        let mut request = calculator.evaluate_request();
        request.get().init_expression().set_literal(123.0);
        let value = request.send().pipeline.get_value();
        let request = value.read_request();
        try!(core.run(request.send().promise.and_then(|response| {
            assert_eq!(pry!(response.get()).get_value(), 123.0);
            Promise::ok(())
        })));
        println!("PASS");
    }

    {
        // Make a request to evaluate 123 + 45 - 67.
        //
        // The Calculator interface requires that we first call getOperator() to
        // get the addition and subtraction functions, then call evaluate() to use
        // them.  But, once again, we can get both functions, call evaluate(), and
        // then read() the result -- four RPCs -- in the time of *one* network
        // round trip, because of promise pipelining.

        println!("Using add and subtract... ");

        let add = {
            // Get the "add" function from the server.
            let mut request = calculator.get_operator_request();
            request.get().set_op(calculator::Operator::Add);
            request.send().pipeline.get_func()
        };

        let subtract = {
            // Get the "subtract" function from the server.
            let mut request = calculator.get_operator_request();
            request.get().set_op(calculator::Operator::Subtract);
            request.send().pipeline.get_func()
        };

        // Build the request to evaluate 123 + 45 - 67.
        let mut request = calculator.evaluate_request();

        {
            let mut subtract_call = request.get().init_expression().init_call();
            subtract_call.set_function(subtract);
            let mut subtract_params = subtract_call.init_params(2);
            subtract_params.reborrow().get(1).set_literal(67.0);

            let mut add_call = subtract_params.get(0).init_call();
            add_call.set_function(add);
            let mut add_params = add_call.init_params(2);
            add_params.reborrow().get(0).set_literal(123.0);
            add_params.get(1).set_literal(45.0);
        }

        // Send the evaluate() request, read() the result, and wait for read() to
        // finish.
        let eval_promise = request.send();
        let read_promise = eval_promise.pipeline.get_value().read_request().send();

        let response = try!(core.run(read_promise.promise));
        assert_eq!(try!(response.get()).get_value(), 101.0);

        println!("PASS");
    }

    {
        // Make a request to evaluate 4 * 6, then use the result in two more
        // requests that add 3 and 5.
        //
        // Since evaluate() returns its result wrapped in a `Value`, we can pass
        // that `Value` back to the server in subsequent requests before the first
        // `evaluate()` has actually returned.  Thus, this example again does only
        // one network round trip.

        println!("Pipelining eval() calls... ");

        let add = {
            // Get the "add" function from the server.
            let mut request = calculator.get_operator_request();
            request.get().set_op(calculator::Operator::Add);
            request.send().pipeline.get_func()
        };

        let multiply = {
            // Get the "multiply" function from the server.
            let mut request = calculator.get_operator_request();
            request.get().set_op(calculator::Operator::Multiply);
            request.send().pipeline.get_func()
        };

        // Build the request to evaluate 4 * 6
        let mut request = calculator.evaluate_request();

        {
            let mut multiply_call = request.get().init_expression().init_call();
            multiply_call.set_function(multiply);
            let mut multiply_params = multiply_call.init_params(2);
            multiply_params.reborrow().get(0).set_literal(4.0);
            multiply_params.reborrow().get(1).set_literal(6.0);
        }

        let multiply_result = request.send().pipeline.get_value();

        // Use the result in two calls that add 3 and add 5.
        let mut add3_request = calculator.evaluate_request();
        {
            let mut add3_call = add3_request.get().init_expression().init_call();
            add3_call.set_function(add.clone());
            let mut add3_params = add3_call.init_params(2);
            add3_params
                .reborrow()
                .get(0)
                .set_previous_result(multiply_result.clone());
            add3_params.reborrow().get(1).set_literal(3.0);
        }

        let add3_promise = add3_request
            .send()
            .pipeline
            .get_value()
            .read_request()
            .send();

        let mut add5_request = calculator.evaluate_request();
        {
            let mut add5_call = add5_request.get().init_expression().init_call();
            add5_call.set_function(add);
            let mut add5_params = add5_call.init_params(2);
            add5_params
                .reborrow()
                .get(0)
                .set_previous_result(multiply_result);
            add5_params.get(1).set_literal(5.0);
        }

        let add5_promise = add5_request
            .send()
            .pipeline
            .get_value()
            .read_request()
            .send();

        // Now wait for the results.
        assert!(try!(try!(core.run(add3_promise.promise)).get()).get_value() == 27.0);
        assert!(try!(try!(core.run(add5_promise.promise)).get()).get_value() == 29.0);

        println!("PASS")
    }

    {
        // Our calculator interface supports defining functions.  Here we use it
        // to define two functions and then make calls to them as follows:
        //
        //   f(x, y) = x * 100 + y
        //   g(x) = f(x, x + 1) * 2;
        //   f(12, 34)
        //   g(21)
        //
        // Once again, the whole thing takes only one network round trip.

        println!("Defining functions... ");

        let add = {
            let mut request = calculator.get_operator_request();
            request.get().set_op(calculator::Operator::Add);
            request.send().pipeline.get_func()
        };

        let multiply = {
            let mut request = calculator.get_operator_request();
            request.get().set_op(calculator::Operator::Multiply);
            request.send().pipeline.get_func()
        };

        let f = {
            let mut request = calculator.def_function_request();
            {
                let mut def_function_params = request.get();
                def_function_params.set_param_count(2);
                {
                    let mut add_call = def_function_params.init_body().init_call();
                    add_call.set_function(add.clone());
                    let mut add_params = add_call.init_params(2);
                    add_params.reborrow().get(1).set_parameter(1);

                    let mut multiply_call = add_params.get(0).init_call();
                    multiply_call.set_function(multiply.clone());
                    let mut multiply_params = multiply_call.init_params(2);
                    multiply_params.reborrow().get(0).set_parameter(0);
                    multiply_params.get(1).set_literal(100.0);
                }
            }
            request.send().pipeline.get_func()
        };

        let g = {
            let mut request = calculator.def_function_request();
            {
                let mut def_function_params = request.get();
                def_function_params.set_param_count(1);
                {
                    let mut multiply_call = def_function_params.init_body().init_call();
                    multiply_call.set_function(multiply);
                    let mut multiply_params = multiply_call.init_params(2);
                    multiply_params.reborrow().get(1).set_literal(2.0);

                    let mut f_call = multiply_params.get(0).init_call();
                    f_call.set_function(f.clone());
                    let mut f_params = f_call.init_params(2);
                    f_params.reborrow().get(0).set_parameter(0);

                    let mut add_call = f_params.get(1).init_call();
                    add_call.set_function(add);
                    let mut add_params = add_call.init_params(2);
                    add_params.reborrow().get(0).set_parameter(0);
                    add_params.get(1).set_literal(1.0);
                }
            }
            request.send().pipeline.get_func()
        };

        let mut f_eval_request = calculator.evaluate_request();
        {
            let mut f_call = f_eval_request.get().init_expression().init_call();
            f_call.set_function(f);
            let mut f_params = f_call.init_params(2);
            f_params.reborrow().get(0).set_literal(12.0);
            f_params.get(1).set_literal(34.0);
        }
        let f_eval_promise = f_eval_request
            .send()
            .pipeline
            .get_value()
            .read_request()
            .send();

        let mut g_eval_request = calculator.evaluate_request();
        {
            let mut g_call = g_eval_request.get().init_expression().init_call();
            g_call.set_function(g);
            g_call.init_params(1).get(0).set_literal(21.0);
        }
        let g_eval_promise = g_eval_request
            .send()
            .pipeline
            .get_value()
            .read_request()
            .send();

        assert!(try!(try!(core.run(f_eval_promise.promise)).get()).get_value() == 1234.0);
        assert!(try!(try!(core.run(g_eval_promise.promise)).get()).get_value() == 4244.0);

        println!("PASS")
    }

    {
        // Make a request that will call back to a function defined locally.
        //
        // Specifically, we will compute 2^(4 + 5).  However, exponent is not
        // defined by the Calculator server.  So, we'll implement the Function
        // interface locally and pass it to the server for it to use when
        // evaluating the expression.
        //
        // This example requires two network round trips to complete, because the
        // server calls back to the client once before finishing.  In this
        // particular case, this could potentially be optimized by using a tail
        // call on the server side -- see CallContext::tailCall().  However, to
        // keep the example simpler, we haven't implemented this optimization in
        // the sample server.

        println!("Using a callback... ");

        let add = {
            let mut request = calculator.get_operator_request();
            request.get().set_op(calculator::Operator::Add);
            request.send().pipeline.get_func()
        };

        let mut request = calculator.evaluate_request();
        {
            let mut pow_call = request.get().init_expression().init_call();
            pow_call.set_function(
                calculator::function::ToClient::new(PowerFunction)
                    .from_server::<::capnp_rpc::Server>(),
            );
            let mut pow_params = pow_call.init_params(2);
            pow_params.reborrow().get(0).set_literal(2.0);

            let mut add_call = pow_params.get(1).init_call();
            add_call.set_function(add);
            let mut add_params = add_call.init_params(2);
            add_params.reborrow().get(0).set_literal(4.0);
            add_params.get(1).set_literal(5.0);
        }

        let response_promise = request.send().pipeline.get_value().read_request().send();

        assert!(try!(try!(core.run(response_promise.promise)).get()).get_value() == 512.0);

        println!("PASS");
    }

    Ok(())
}
