// Copyright (c) 2017 Sandstorm Development Group, Inc. and contributors
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

use capnp::{any_pointer, message, Word};

use codegen::FormattedText::{Branch, Indent, Line};
use codegen::{FormattedText, GeneratorContext};
use codegen_types::{Leaf, RustTypeInfo};
use schema_capnp::type_;

pub fn generate_pointer_constant(
    gen: &GeneratorContext,
    styled_name: &str,
    typ: type_::Reader,
    value: any_pointer::Reader,
) -> ::capnp::Result<FormattedText> {
    let allocator = message::HeapAllocator::new()
        .first_segment_words(try!(value.target_size()).word_count as u32 + 1);
    let mut message = message::Builder::new(allocator);
    try!(message.set_root(value));
    let mut words_lines = Vec::new();
    let words = message.get_segments_for_output()[0];
    for &word in words {
        let tmp = &[word];
        let bytes = Word::words_to_bytes(tmp);
        words_lines.push(Line(format!(
            "capnp_word!({}, {}, {}, {}, {}, {}, {}, {}),",
            bytes[0], bytes[1], bytes[2], bytes[3], bytes[4], bytes[5], bytes[6], bytes[7]
        )));
    }
    Ok(Branch(vec![
        Line(format!(
            "pub static {}: ::capnp::constant::Reader<{}> = {{",
            styled_name,
            try!(typ.type_string(gen, Leaf::Owned))
        )),
        Indent(Box::new(Branch(vec![
            Line(format!(
                "static WORDS: [::capnp::Word; {}] = [",
                words.len()
            )),
            Indent(Box::new(Branch(words_lines))),
            Line("];".to_string()),
            Line("::capnp::constant::Reader {".into()),
            Indent(Box::new(Branch(vec![
                Line("phantom: ::std::marker::PhantomData,".into()),
                Line("words: &WORDS,".into()),
            ]))),
            Line("}".into()),
        ]))),
        Line("};".to_string()),
    ]))
}
