// Copyright (c) 2017 David Renshaw and contributors
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

extern crate capnp;

use std::{env, process, time};

fn run_one(
    executable: &str,
    case: &str,
    mode: &str,
    scratch: &str,
    compression: &str,
    iteration_count: u64,
) {
    let mut command = process::Command::new(executable);
    command
        .arg(case)
        .arg(mode)
        .arg(scratch)
        .arg(compression)
        .arg(format!("{}", iteration_count));

    println!("{} {} {}", mode, compression, scratch);
    let start_time = time::Instant::now();
    let result_status = command.spawn().unwrap().wait().unwrap();
    let elapsed = start_time.elapsed();

    if !result_status.success() {
        panic!("failed to run test case");
    }

    let elapsed_secs = elapsed.as_secs() as f64 + (elapsed.subsec_nanos() as f64 / 1e9);
    println!("{}\n", elapsed_secs);
}

fn run_case(executable: &str, case: &str, scratch_options: &[&str], iteration_count: u64) {
    for scratch in scratch_options {
        run_one(executable, case, "object", scratch, "none", iteration_count);
    }

    for mode in &["bytes", "pipe"] {
        for compression in &["none", "packed"] {
            for scratch in scratch_options {
                run_one(
                    executable,
                    case,
                    mode,
                    scratch,
                    compression,
                    iteration_count,
                );
            }
        }
    }
}

fn try_main() -> ::capnp::Result<()> {
    let args: Vec<String> = env::args().collect();

    assert!(
        args.len() == 2 || args.len() == 5,
        "USAGE: {} BENCHMARK_EXECUTABLE [CARSALES_ITERS CATRANK_ITERS EVAL_ITERS]",
        args[0]
    );

    let (carsales_iters, catrank_iters, eval_iters) = if args.len() > 2 {
        (
            args[2].parse::<u64>().unwrap(),
            args[3].parse::<u64>().unwrap(),
            args[4].parse::<u64>().unwrap(),
        )
    } else {
        (10000, 1000, 200000)
    };

    let executable = &*args[1];

    println!("running carsales with {} iterations", carsales_iters);
    run_case(
        executable,
        "carsales",
        &["reuse", "no-reuse"],
        carsales_iters,
    );

    println!("running catrank with {} iterations", catrank_iters);
    run_case(executable, "catrank", &["no-reuse"], catrank_iters);

    println!("running eval with {} iterations", eval_iters);
    run_case(executable, "eval", &["no-reuse"], eval_iters);

    Ok(())
}

pub fn main() {
    match try_main() {
        Ok(()) => (),
        Err(e) => {
            panic!("error: {:?}", e);
        }
    }
}
