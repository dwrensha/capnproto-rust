// Copyright (c) 2013-2014 Sandstorm Development Group, Inc. and contributors
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

use catrank_capnp::*;
use common::*;

#[derive(Clone, Copy)]
pub struct ScoredResult<'a> {
    score: f64,
    result: search_result::Reader<'a>,
}

const URL_PREFIX: &'static str = "http://example.com";

pub struct CatRank;

impl ::TestCase for CatRank {
    type Request = search_result_list::Owned;
    type Response = search_result_list::Owned;
    type Expectation = i32;

    fn setup_request(&self, rng: &mut FastRand, request: search_result_list::Builder) -> i32 {
        let count = rng.next_less_than(1000);
        let mut good_count: i32 = 0;

        let mut list = request.init_results(count);

        for i in 0..count {
            let mut result = list.reborrow().get(i);
            result.set_score(1000.0 - i as f64);
            let url_size = rng.next_less_than(100);

            let url_prefix_length = URL_PREFIX.as_bytes().len();
            {
                let mut url = result
                    .reborrow()
                    .init_url(url_size + url_prefix_length as u32);

                url.push_str(URL_PREFIX);
                for _ in 0..url_size {
                    url.push_ascii((97 + rng.next_less_than(26)) as u8);
                }
            }

            let is_cat = rng.next_less_than(8) == 0;
            let is_dog = rng.next_less_than(8) == 0;
            if is_cat && !is_dog {
                good_count += 1;
            }

            let mut snippet = " ".to_string();

            let prefix = rng.next_less_than(20) as usize;
            for _ in 0..prefix {
                snippet.push_str(WORDS[rng.next_less_than(WORDS.len() as u32) as usize]);
            }
            if is_cat {
                snippet.push_str("cat ")
            }
            if is_dog {
                snippet.push_str("dog ")
            }

            let suffix = rng.next_less_than(20) as usize;
            for _ in 0..suffix {
                snippet.push_str(WORDS[rng.next_less_than(WORDS.len() as u32) as usize]);
            }

            result.set_snippet(&snippet);
        }

        good_count
    }

    fn handle_request(
        &self,
        request: search_result_list::Reader,
        response: search_result_list::Builder,
    ) -> ::capnp::Result<()> {
        let mut scored_results: Vec<ScoredResult> = Vec::new();

        let results = try!(request.get_results());
        for i in 0..results.len() {
            let result = results.get(i);
            let mut score = result.get_score();
            let snippet = try!(result.get_snippet());
            if snippet.contains(" cat ") {
                score *= 10000.0;
            }
            if snippet.contains(" dog ") {
                score /= 10000.0;
            }
            scored_results.push(ScoredResult {
                score: score,
                result: result,
            });
        }

        // sort in decreasing order
        scored_results.sort_by(|v1, v2| {
            if v1.score < v2.score {
                ::std::cmp::Ordering::Greater
            } else {
                ::std::cmp::Ordering::Less
            }
        });

        let mut list = response.init_results(scored_results.len() as u32);
        for i in 0..list.len() {
            let mut item = list.reborrow().get(i);
            let result = scored_results[i as usize];
            item.set_score(result.score);
            item.set_url(try!(result.result.get_url()));
            item.set_snippet(try!(result.result.get_snippet()));
        }

        Ok(())
    }

    fn check_response(
        &self,
        response: search_result_list::Reader,
        expected_good_count: i32,
    ) -> ::capnp::Result<()> {
        let mut good_count: i32 = 0;
        let results = try!(response.get_results());
        for result in results.iter() {
            if result.get_score() > 1001.0 {
                good_count += 1;
            } else {
                break;
            }
        }

        if good_count == expected_good_count {
            Ok(())
        } else {
            Err(::capnp::Error::failed(format!(
                "check_response() expected {} but got {}",
                expected_good_count, good_count
            )))
        }
    }
}
