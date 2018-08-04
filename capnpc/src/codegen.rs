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

use std::collections;
use std::collections::HashSet;

use capnp;
use capnp::Error;

use self::FormattedText::{BlankLine, Branch, Indent, Line};
use codegen_types::{do_branding, Leaf, RustNodeInfo, RustTypeInfo, TypeParameterTexts};
use pointer_constants::generate_pointer_constant;
use schema_capnp;

pub struct GeneratorContext<'a> {
    pub request: schema_capnp::code_generator_request::Reader<'a>,
    pub node_map: collections::hash_map::HashMap<u64, schema_capnp::node::Reader<'a>>,
    pub scope_map: collections::hash_map::HashMap<u64, Vec<String>>,
}

impl<'a> GeneratorContext<'a> {
    pub fn new(
        message: &'a capnp::message::Reader<capnp::serialize::OwnedSegments>,
    ) -> ::capnp::Result<GeneratorContext<'a>> {
        let mut gen = GeneratorContext {
            request: try!(message.get_root()),
            node_map: collections::hash_map::HashMap::<u64, schema_capnp::node::Reader<'a>>::new(),
            scope_map: collections::hash_map::HashMap::<u64, Vec<String>>::new(),
        };

        for node in try!(gen.request.get_nodes()).iter() {
            gen.node_map.insert(node.get_id(), node);
        }

        for requested_file in try!(gen.request.get_requested_files()).iter() {
            let id = requested_file.get_id();

            let imports = try!(requested_file.get_imports());
            for import in imports.iter() {
                let importpath = ::std::path::Path::new(try!(import.get_name()));
                let root_name: String = format!(
                    "::{}_capnp",
                    try!(path_to_stem_string(importpath)).replace("-", "_")
                );
                try!(populate_scope_map(
                    &gen.node_map,
                    &mut gen.scope_map,
                    vec![root_name],
                    import.get_id()
                ));
            }

            let root_name = try!(path_to_stem_string(try!(requested_file.get_filename())));
            let root_mod = format!("::{}_capnp", root_name.replace("-", "_"));
            try!(populate_scope_map(
                &gen.node_map,
                &mut gen.scope_map,
                vec![root_mod],
                id
            ));
        }
        Ok(gen)
    }

    fn get_last_name<'b>(&'b self, id: u64) -> ::capnp::Result<&'b str> {
        match self.scope_map.get(&id) {
            None => Err(Error::failed(format!("node not found: {}", id))),
            Some(v) => match v.last() {
                None => Err(Error::failed(format!("node has no scope: {}", id))),
                Some(n) => Ok(&n),
            },
        }
    }
}

fn path_to_stem_string<P: AsRef<::std::path::Path>>(path: P) -> ::capnp::Result<String> {
    match path.as_ref().file_stem() {
        None => Err(Error::failed(format!(
            "file has no stem: {:?}",
            path.as_ref()
        ))),
        Some(stem) => match stem.to_owned().into_string() {
            Err(os_string) => Err(Error::failed(format!("bad filename: {:?}", os_string))),
            Ok(s) => Ok(s),
        },
    }
}

fn snake_to_upper_case(s: &str) -> String {
    let mut result_chars: Vec<char> = Vec::new();
    for c in s.chars() {
        if c == '_' {
            result_chars.push('_');
        } else {
            assert!(
                c.is_alphanumeric(),
                format!(
                    "non-alphanumeric character '{}', i.e. {} in identifier '{}'",
                    c, c as usize, s
                )
            );
            result_chars.push(c.to_ascii_uppercase());
        }
    }
    result_chars.into_iter().collect()
}

fn camel_to_snake_case(s: &str) -> String {
    let mut result_chars: Vec<char> = Vec::new();
    let mut first_char = true;
    for c in s.chars() {
        assert!(
            c.is_alphanumeric(),
            format!(
                "non-alphanumeric character '{}', i.e. {} in identifier '{}'",
                c, c as usize, s
            )
        );
        if c.is_uppercase() && !first_char {
            result_chars.push('_');
        }
        result_chars.push(c.to_ascii_lowercase());
        first_char = false;
    }
    result_chars.into_iter().collect()
}

fn capitalize_first_letter(s: &str) -> String {
    let mut result_chars: Vec<char> = Vec::new();
    for c in s.chars() {
        result_chars.push(c)
    }
    result_chars[0] = result_chars[0].to_ascii_uppercase();
    result_chars.into_iter().collect()
}

#[test]
fn test_camel_to_snake_case() {
    assert_eq!(camel_to_snake_case("fooBar"), "foo_bar".to_string());
    assert_eq!(camel_to_snake_case("FooBar"), "foo_bar".to_string());
    assert_eq!(camel_to_snake_case("fooBarBaz"), "foo_bar_baz".to_string());
    assert_eq!(camel_to_snake_case("FooBarBaz"), "foo_bar_baz".to_string());
    assert_eq!(camel_to_snake_case("helloWorld"), "hello_world".to_string());
    assert_eq!(camel_to_snake_case("HelloWorld"), "hello_world".to_string());
    assert_eq!(camel_to_snake_case("uint32Id"), "uint32_id".to_string());
}

#[derive(PartialEq, Clone)]
pub enum FormattedText {
    Indent(Box<FormattedText>),
    Branch(Vec<FormattedText>),
    Line(String),
    BlankLine,
}

fn to_lines(ft: &FormattedText, indent: usize) -> Vec<String> {
    match *ft {
        Indent(ref ft) => {
            return to_lines(&**ft, indent + 1);
        }
        Branch(ref fts) => {
            let mut result = Vec::new();
            for ft in fts.iter() {
                for line in to_lines(ft, indent).iter() {
                    result.push(line.clone()); // TODO there's probably a better way to do this.
                }
            }
            return result;
        }
        Line(ref s) => {
            let mut s1: String = ::std::iter::repeat(' ').take(indent * 2).collect();
            s1.push_str(&s);
            return vec![s1.to_string()];
        }
        BlankLine => return vec!["".to_string()],
    }
}

fn stringify(ft: &FormattedText) -> String {
    let mut result = to_lines(ft, 0).join("\n");
    result.push_str("\n");
    result.to_string()
}

const RUST_KEYWORDS: [&'static str; 53] = [
    "abstract", "alignof", "as", "be", "become", "box", "break", "const", "continue", "crate",
    "do", "else", "enum", "extern", "false", "final", "fn", "for", "if", "impl", "in", "let",
    "loop", "macro", "match", "mod", "move", "mut", "offsetof", "once", "override", "priv", "proc",
    "pub", "pure", "ref", "return", "self", "sizeof", "static", "struct", "super", "trait", "true",
    "type", "typeof", "unsafe", "unsized", "use", "virtual", "where", "while", "yield",
];

fn module_name(camel_case: &str) -> String {
    let mut name = camel_to_snake_case(camel_case);
    if RUST_KEYWORDS.contains(&&*name) {
        name.push('_');
    }
    name
}

fn populate_scope_map(
    node_map: &collections::hash_map::HashMap<u64, schema_capnp::node::Reader>,
    scope_map: &mut collections::hash_map::HashMap<u64, Vec<String>>,
    scope_names: Vec<String>,
    node_id: u64,
) -> ::capnp::Result<()> {
    scope_map.insert(node_id, scope_names.clone());

    // unused nodes in imported files might be omitted from the node map
    let node_reader = match node_map.get(&node_id) {
        Some(node) => node,
        None => return Ok(()),
    };

    let nested_nodes = try!(node_reader.get_nested_nodes());
    for nested_node in nested_nodes.iter() {
        let mut scope_names = scope_names.clone();
        let nested_node_id = nested_node.get_id();
        match node_map.get(&nested_node_id) {
            None => {}
            Some(node_reader) => match node_reader.which() {
                Ok(schema_capnp::node::Enum(_enum_reader)) => {
                    scope_names.push(try!(nested_node.get_name()).to_string());
                    try!(populate_scope_map(
                        node_map,
                        scope_map,
                        scope_names,
                        nested_node_id
                    ));
                }
                _ => {
                    scope_names.push(module_name(try!(nested_node.get_name())));
                    try!(populate_scope_map(
                        node_map,
                        scope_map,
                        scope_names,
                        nested_node_id
                    ));
                }
            },
        }
    }

    match node_reader.which() {
        Ok(schema_capnp::node::Struct(struct_reader)) => {
            let fields = try!(struct_reader.get_fields());
            for field in fields.iter() {
                match field.which() {
                    Ok(schema_capnp::field::Group(group)) => {
                        let name = module_name(try!(field.get_name()));
                        let mut scope_names = scope_names.clone();
                        scope_names.push(name);
                        try!(populate_scope_map(
                            node_map,
                            scope_map,
                            scope_names,
                            group.get_type_id()
                        ));
                    }
                    _ => {}
                }
            }
        }
        _ => {}
    }
    Ok(())
}

fn prim_default(value: &schema_capnp::value::Reader) -> ::capnp::Result<Option<String>> {
    use schema_capnp::value;
    match try!(value.which()) {
        value::Bool(false)
        | value::Int8(0)
        | value::Int16(0)
        | value::Int32(0)
        | value::Int64(0)
        | value::Uint8(0)
        | value::Uint16(0)
        | value::Uint32(0)
        | value::Uint64(0) => Ok(None),

        value::Bool(true) => Ok(Some(format!("true"))),
        value::Int8(i) => Ok(Some(i.to_string())),
        value::Int16(i) => Ok(Some(i.to_string())),
        value::Int32(i) => Ok(Some(i.to_string())),
        value::Int64(i) => Ok(Some(i.to_string())),
        value::Uint8(i) => Ok(Some(i.to_string())),
        value::Uint16(i) => Ok(Some(i.to_string())),
        value::Uint32(i) => Ok(Some(i.to_string())),
        value::Uint64(i) => Ok(Some(i.to_string())),
        value::Float32(f) => match f.classify() {
            ::std::num::FpCategory::Zero => Ok(None),
            _ => Ok(Some(format!(
                "{}u32",
                unsafe { ::std::mem::transmute::<f32, u32>(f) }.to_string()
            ))),
        },
        value::Float64(f) => match f.classify() {
            ::std::num::FpCategory::Zero => Ok(None),
            _ => Ok(Some(format!(
                "{}u64",
                unsafe { ::std::mem::transmute::<f64, u64>(f) }.to_string()
            ))),
        },
        _ => Err(Error::failed(
            "Non-primitive value found where primitive was expected.".to_string(),
        )),
    }
}

pub fn getter_text(
    gen: &GeneratorContext,
    field: &schema_capnp::field::Reader,
    is_reader: bool,
) -> ::capnp::Result<(String, FormattedText)> {
    use schema_capnp::*;

    match try!(field.which()) {
        field::Group(group) => {
            let the_mod = gen.scope_map[&group.get_type_id()].join("::");
            if is_reader {
                Ok((
                    format!("{}::Reader<'a>", the_mod),
                    Line("::capnp::traits::FromStructReader::new(self.reader)".to_string()),
                ))
            } else {
                Ok((
                    format!("{}::Builder<'a>", the_mod),
                    Line("::capnp::traits::FromStructBuilder::new(self.builder)".to_string()),
                ))
            }
        }
        field::Slot(reg_field) => {
            let offset = reg_field.get_offset() as usize;
            let module_string = if is_reader { "Reader" } else { "Builder" };
            let module = if is_reader {
                Leaf::Reader("'a")
            } else {
                Leaf::Builder("'a")
            };
            let member = camel_to_snake_case(&*format!("{}", module_string));

            fn primitive_case<T: PartialEq + ::std::fmt::Display>(
                typ: &str,
                member: String,
                offset: usize,
                default: T,
                zero: T,
            ) -> FormattedText {
                if default == zero {
                    Line(format!(
                        "self.{}.get_data_field::<{}>({})",
                        member, typ, offset
                    ))
                } else {
                    Line(format!(
                        "self.{}.get_data_field_mask::<{typ}>({}, {})",
                        member,
                        offset,
                        default,
                        typ = typ
                    ))
                }
            }

            let raw_type = try!(reg_field.get_type());
            let typ = try!(raw_type.type_string(gen, module));
            let default_value = try!(reg_field.get_default_value());
            let default = try!(default_value.which());

            let result_type = match try!(raw_type.which()) {
                type_::Enum(_) => format!("::std::result::Result<{},::capnp::NotInSchema>", typ),
                type_::AnyPointer(_) if !try!(raw_type.is_parameter()) => typ.clone(),
                type_::Interface(_) => format!(
                    "::capnp::Result<{}>",
                    try!(raw_type.type_string(gen, Leaf::Client))
                ),
                _ if try!(raw_type.is_prim()) => typ.clone(),
                _ => format!("::capnp::Result<{}>", typ),
            };

            let getter_code = match (try!(raw_type.which()), default) {
                (type_::Void(()), value::Void(())) => Line("()".to_string()),
                (type_::Bool(()), value::Bool(b)) => {
                    if b {
                        Line(format!("self.{}.get_bool_field_mask({}, true)", member, offset))
                    } else {
                        Line(format!("self.{}.get_bool_field({})", member, offset))
                    }
                }
                (type_::Int8(()), value::Int8(i)) => primitive_case(&*typ, member, offset, i, 0),
                (type_::Int16(()), value::Int16(i)) => primitive_case(&*typ, member, offset, i, 0),
                (type_::Int32(()), value::Int32(i)) => primitive_case(&*typ, member, offset, i, 0),
                (type_::Int64(()), value::Int64(i)) => primitive_case(&*typ, member, offset, i, 0),
                (type_::Uint8(()), value::Uint8(i)) => primitive_case(&*typ, member, offset, i, 0),
                (type_::Uint16(()), value::Uint16(i)) => primitive_case(&*typ, member, offset, i, 0),
                (type_::Uint32(()), value::Uint32(i)) => primitive_case(&*typ, member, offset, i, 0),
                (type_::Uint64(()), value::Uint64(i)) => primitive_case(&*typ, member, offset, i, 0),
                (type_::Float32(()), value::Float32(f)) =>
                    primitive_case(&*typ, member, offset, unsafe { ::std::mem::transmute::<f32, u32>(f) }, 0),
                (type_::Float64(()), value::Float64(f)) =>
                    primitive_case(&*typ, member, offset, unsafe { ::std::mem::transmute::<f64, u64>(f) }, 0),
                (type_::Enum(_), value::Enum(d)) => {
                    if d == 0 {
                        Line(format!("::capnp::traits::FromU16::from_u16(self.{}.get_data_field::<u16>({}))",
                                     member, offset))
                    } else {
                        Line(
                            format!(
                                "::capnp::traits::FromU16::from_u16(self.{}.get_data_field_mask::<u16>({}, {}))",
                                member, offset, d))
                    }
                }

                (type_::Text(()), value::Text(t)) => {
                    if default_value.has_text() && try!(t).len() > 0 && is_reader {
                        println!("cargo:warning=[UNSUPPORTED] Ignoring default value {:?} for text field {}. \
                                  See https://github.com/dwrensha/capnpc-rust/issues/38",
                                 try!(t), try!(field.get_name()));
                    }
                    Line(format!("self.{}.get_pointer_field({}).get_text(::std::ptr::null(), 0)", member, offset))
                }
                (type_::Data(()), value::Data(d)) => {
                    if default_value.has_data() && try!(d).len() > 0 && is_reader {
                        println!("cargo:warning=[UNSUPPORTED] Ignoring default value {:?} for data field {}. \
                                  See https://github.com/dwrensha/capnpc-rust/issues/38",
                                 try!(d), try!(field.get_name()));
                    }

                    Line(format!("self.{}.get_pointer_field({}).get_data(::std::ptr::null(), 0)", member, offset))
                }
                (type_::List(_), value::List(_)) => {
                    if default_value.has_list() && is_reader {
                        // TODO: Don't emit warning if the list if of length zero.
                        println!("cargo:warning=[UNSUPPORTED] Ignoring default for list field {}. \
                                  See https://github.com/dwrensha/capnpc-rust/issues/38",
                                 try!(field.get_name()));
                    }

                    if is_reader {
                        Line(format!(
                            "::capnp::traits::FromPointerReader::get_from_pointer(&self.{}.get_pointer_field({}))",
                            member, offset))
                    } else {
                        Line(format!("::capnp::traits::FromPointerBuilder::get_from_pointer(self.{}.get_pointer_field({}))",
                                     member, offset))

                    }
                }
                (type_::Struct(_), value::Struct(_)) => {
                    if default_value.has_struct() && is_reader {
                        // TODO: Don't emit warning if the struct is zero-sized.
                        println!("cargo:warning=[UNSUPPORTED] Ignoring default for struct field {}. \
                                  See https://github.com/dwrensha/capnpc-rust/issues/38",
                                 try!(field.get_name()));
                    }

                    if is_reader {
                        Line(format!("::capnp::traits::FromPointerReader::get_from_pointer(&self.{}.get_pointer_field({}))",
                                     member, offset))
                    } else {
                        Line(format!("::capnp::traits::FromPointerBuilder::get_from_pointer(self.{}.get_pointer_field({}))",
                                     member, offset))
                    }
                }
                (type_::Interface(_), value::Interface(_)) => {
                    Line(format!("match self.{}.get_pointer_field({}).get_capability() {{ ::std::result::Result::Ok(c) => ::std::result::Result::Ok(::capnp::capability::FromClientHook::new(c)), ::std::result::Result::Err(e) => ::std::result::Result::Err(e)}}",
                                 member, offset))
                }
                (type_::AnyPointer(_), value::AnyPointer(_)) => {
                    if !try!(raw_type.is_parameter()) {
                        Line(format!("::capnp::any_pointer::{}::new(self.{}.get_pointer_field({}))", module_string, member, offset))
                    } else {
                        if is_reader {
                            Line(format!("::capnp::traits::FromPointerReader::get_from_pointer(&self.{}.get_pointer_field({}))", member, offset))
                        } else {
                            Line(format!("::capnp::traits::FromPointerBuilder::get_from_pointer(self.{}.get_pointer_field({}))", member, offset))
                        }
                    }
                }
                _ => return Err(Error::failed(format!("default value was of wrong type"))),
            };
            Ok((result_type, getter_code))
        }
    }
}

fn zero_fields_of_group(gen: &GeneratorContext, node_id: u64) -> ::capnp::Result<FormattedText> {
    use schema_capnp::{field, node, type_};
    match try!(gen.node_map[&node_id].which()) {
        node::Struct(st) => {
            let mut result = Vec::new();
            if st.get_discriminant_count() != 0 {
                result.push(Line(format!(
                    "self.builder.set_data_field::<u16>({}, 0);",
                    st.get_discriminant_offset()
                )));
            }
            let fields = try!(st.get_fields());
            for field in fields.iter() {
                match try!(field.which()) {
                    field::Group(group) => {
                        result.push(try!(zero_fields_of_group(gen, group.get_type_id())));
                    }
                    field::Slot(slot) => {
                        let typ = try!(try!(slot.get_type()).which());
                        match typ {
                            type_::Void(()) => {}
                            type_::Bool(()) => {
                                let line = Line(format!("self.builder.set_bool_field({}, false);",
                                                        slot.get_offset()));
                                // PERF could dedup more efficiently
                                if !result.contains(&line) { result.push(line) }
                            }
                            type_::Int8(()) |
                            type_::Int16(()) | type_::Int32(()) | type_::Int64(()) |
                            type_::Uint8(()) | type_::Uint16(()) | type_::Uint32(()) |
                            type_::Uint64(()) | type_::Float32(()) | type_::Float64(()) => {
                                let line = Line(format!(
                                    "self.builder.set_data_field::<{0}>({1}, 0u8 as {0});",
                                    try!(try!(slot.get_type()).type_string(gen, Leaf::Builder("'a"))),
                                    slot.get_offset()));
                                // PERF could dedup more efficiently
                                if !result.contains(&line) { result.push(line) }
                            }
                            type_::Enum(_) => {
                                let line = Line(format!("self.builder.set_data_field::<u16>({}, 0u16);",
                                                        slot.get_offset()));
                                // PERF could dedup more efficiently
                                if !result.contains(&line) { result.push(line) }
                            }
                            type_::Struct(_) | type_::List(_) | type_::Text(()) | type_::Data(()) |
                            type_::AnyPointer(_) |
                            type_::Interface(_) // Is this the right thing to do for interfaces?
                                => {
                                    let line = Line(format!("self.builder.get_pointer_field({}).clear();",
                                                            slot.get_offset()));
                                    // PERF could dedup more efficiently
                                    if !result.contains(&line) { result.push(line) }
                                }
                        }
                    }
                }
            }
            Ok(Branch(result))
        }
        _ => Err(Error::failed(format!(
            "zero_fields_of_groupd() expected a struct"
        ))),
    }
}

fn generate_setter(
    gen: &GeneratorContext,
    discriminant_offset: u32,
    styled_name: &str,
    field: &schema_capnp::field::Reader,
) -> ::capnp::Result<FormattedText> {
    use schema_capnp::*;

    let mut setter_interior = Vec::new();
    let mut setter_param = "value".to_string();
    let mut initter_interior = Vec::new();
    let mut initn_interior = Vec::new();
    let mut initter_params = Vec::new();

    let discriminant_value = field.get_discriminant_value();
    if discriminant_value != field::NO_DISCRIMINANT {
        setter_interior.push(Line(format!(
            "self.builder.set_data_field::<u16>({}, {});",
            discriminant_offset as usize, discriminant_value as usize
        )));
        let init_discrim = Line(format!(
            "self.builder.set_data_field::<u16>({}, {});",
            discriminant_offset as usize, discriminant_value as usize
        ));
        initter_interior.push(init_discrim.clone());
        initn_interior.push(init_discrim);
    }

    let mut setter_generic_param = String::new();
    let mut return_result = false;
    let mut result = Vec::new();

    let (maybe_reader_type, maybe_builder_type): (Option<String>, Option<String>) = match try!(
        field.which()
    ) {
        field::Group(group) => {
            let scope = &gen.scope_map[&group.get_type_id()];
            let the_mod = scope.join("::");

            initter_interior.push(try!(zero_fields_of_group(gen, group.get_type_id())));

            initter_interior.push(Line(format!(
                "::capnp::traits::FromStructBuilder::new(self.builder)"
            )));

            (None, Some(format!("{}::Builder<'a>", the_mod)))
        }
        field::Slot(reg_field) => {
            let offset = reg_field.get_offset() as usize;
            let typ = try!(reg_field.get_type());
            match typ.which().ok().expect("unrecognized type") {
                type_::Void(()) => {
                    setter_param = "_value".to_string();
                    (Some("()".to_string()), None)
                }
                type_::Bool(()) => {
                    match try!(prim_default(&try!(reg_field.get_default_value()))) {
                        None => {
                            setter_interior.push(Line(format!(
                                "self.builder.set_bool_field({}, value);",
                                offset
                            )));
                        }
                        Some(s) => {
                            setter_interior.push(Line(format!(
                                "self.builder.set_bool_field_mask({}, value, {});",
                                offset, s
                            )));
                        }
                    }
                    (Some("bool".to_string()), None)
                }
                _ if try!(typ.is_prim()) => {
                    let tstr = try!(typ.type_string(gen, Leaf::Reader("'a")));
                    match try!(prim_default(&try!(reg_field.get_default_value()))) {
                        None => {
                            setter_interior.push(Line(format!(
                                "self.builder.set_data_field::<{}>({}, value);",
                                tstr, offset
                            )));
                        }
                        Some(s) => {
                            setter_interior.push(Line(format!(
                                "self.builder.set_data_field_mask::<{}>({}, value, {});",
                                tstr, offset, s
                            )));
                        }
                    };
                    (Some(tstr), None)
                }
                type_::Text(()) => {
                    setter_interior.push(Line(format!(
                        "self.builder.get_pointer_field({}).set_text(value);",
                        offset
                    )));
                    initter_interior.push(Line(format!(
                        "self.builder.get_pointer_field({}).init_text(size)",
                        offset
                    )));
                    initter_params.push("size: u32");
                    (
                        Some("::capnp::text::Reader".to_string()),
                        Some("::capnp::text::Builder<'a>".to_string()),
                    )
                }
                type_::Data(()) => {
                    setter_interior.push(Line(format!(
                        "self.builder.get_pointer_field({}).set_data(value);",
                        offset
                    )));
                    initter_interior.push(Line(format!(
                        "self.builder.get_pointer_field({}).init_data(size)",
                        offset
                    )));
                    initter_params.push("size: u32");
                    (
                        Some("::capnp::data::Reader".to_string()),
                        Some("::capnp::data::Builder<'a>".to_string()),
                    )
                }
                type_::List(ot1) => {
                    return_result = true;
                    setter_interior.push(
                        Line(format!("::capnp::traits::SetPointerBuilder::set_pointer_builder(self.builder.get_pointer_field({}), value, false)",
                                     offset)));

                    initter_params.push("size: u32");
                    initter_interior.push(
                        Line(format!("::capnp::traits::FromPointerBuilder::init_pointer(self.builder.get_pointer_field({}), size)", offset)));

                    match try!(try!(ot1.get_element_type()).which()) {
                        type_::List(_) => {
                            setter_generic_param = "<'b>".to_string();
                            (
                                Some(try!(
                                    try!(reg_field.get_type()).type_string(gen, Leaf::Reader("'b"))
                                )),
                                Some(try!(
                                    try!(reg_field.get_type())
                                        .type_string(gen, Leaf::Builder("'a"))
                                )),
                            )
                        }
                        _ => (
                            Some(try!(
                                try!(reg_field.get_type()).type_string(gen, Leaf::Reader("'a"))
                            )),
                            Some(try!(
                                try!(reg_field.get_type()).type_string(gen, Leaf::Builder("'a"))
                            )),
                        ),
                    }
                }
                type_::Enum(e) => {
                    let id = e.get_type_id();
                    let the_mod = gen.scope_map[&id].join("::");
                    setter_interior.push(Line(format!(
                        "self.builder.set_data_field::<u16>({}, value as u16)",
                        offset
                    )));
                    (Some(format!("{}", the_mod)), None)
                }
                type_::Struct(_) => {
                    return_result = true;
                    setter_generic_param = "<'b>".to_string();
                    initter_interior.push(
                      Line(format!("::capnp::traits::FromPointerBuilder::init_pointer(self.builder.get_pointer_field({}), 0)",
                                   offset)));
                    if try!(typ.is_branded()) {
                        setter_interior.push(
                            Line(format!(
                                "<{} as ::capnp::traits::SetPointerBuilder<{}>>::set_pointer_builder(self.builder.get_pointer_field({}), value, false)",
                                try!(typ.type_string(gen, Leaf::Reader("'b"))),
                                try!(typ.type_string(gen, Leaf::Builder("'b"))),
                                offset)));
                        (
                            Some(try!(typ.type_string(gen, Leaf::Reader("'b")))),
                            Some(try!(typ.type_string(gen, Leaf::Builder("'a")))),
                        )
                    } else {
                        setter_interior.push(
                            Line(format!("::capnp::traits::SetPointerBuilder::set_pointer_builder(self.builder.get_pointer_field({}), value, false)", offset)));
                        (
                            Some(try!(
                                try!(reg_field.get_type()).type_string(gen, Leaf::Reader("'b"))
                            )),
                            Some(try!(
                                try!(reg_field.get_type()).type_string(gen, Leaf::Builder("'a"))
                            )),
                        )
                    }
                }
                type_::Interface(_) => {
                    setter_interior.push(Line(format!(
                        "self.builder.get_pointer_field({}).set_capability(value.client.hook);",
                        offset
                    )));
                    (Some(try!(typ.type_string(gen, Leaf::Client))), None)
                }
                type_::AnyPointer(_) => {
                    if try!(typ.is_parameter()) {
                        initter_interior.push(Line(format!("::capnp::any_pointer::Builder::new(self.builder.get_pointer_field({})).init_as()", offset)));
                        setter_generic_param = format!(
                            "<SPB: ::capnp::traits::SetPointerBuilder<{}>>",
                            try!(typ.type_string(gen, Leaf::Builder("'a")))
                        );
                        setter_interior.push(Line(format!("::capnp::traits::SetPointerBuilder::set_pointer_builder(self.builder.get_pointer_field({}), value, false)", offset)));
                        return_result = true;

                        let builder_type = try!(typ.type_string(gen, Leaf::Builder("'a")));

                        result.push(Line("#[inline]".to_string()));
                        result.push(Line(format!(
                            "pub fn initn_{}(self, length: u32) -> {} {{",
                            styled_name, builder_type
                        )));
                        result.push(Indent(Box::new(Branch(initn_interior))));
                        result.push(Indent(Box::new(
                            Line(format!("::capnp::any_pointer::Builder::new(self.builder.get_pointer_field({})).initn_as(length)", offset)))));
                        result.push(Line("}".to_string()));

                        (Some("SPB".to_string()), Some(builder_type))
                    } else {
                        initter_interior.push(Line(format!("let mut result = ::capnp::any_pointer::Builder::new(self.builder.get_pointer_field({}));",
                                                   offset)));
                        initter_interior.push(Line("result.clear();".to_string()));
                        initter_interior.push(Line("result".to_string()));
                        (None, Some("::capnp::any_pointer::Builder<'a>".to_string()))
                    }
                }
                _ => return Err(Error::failed(format!("unrecognized type"))),
            }
        }
    };

    match maybe_reader_type {
        Some(ref reader_type) => {
            let return_type = if return_result {
                "-> ::capnp::Result<()>"
            } else {
                ""
            };
            result.push(Line("#[inline]".to_string()));
            result.push(Line(format!(
                "pub fn set_{}{}(&mut self, {}: {}) {} {{",
                styled_name, setter_generic_param, setter_param, reader_type, return_type
            )));
            result.push(Indent(Box::new(Branch(setter_interior))));
            result.push(Line("}".to_string()));
        }
        None => {}
    }
    match maybe_builder_type {
        Some(builder_type) => {
            result.push(Line("#[inline]".to_string()));
            let args = initter_params.join(", ");
            result.push(Line(format!(
                "pub fn init_{}(self, {}) -> {} {{",
                styled_name, args, builder_type
            )));
            result.push(Indent(Box::new(Branch(initter_interior))));
            result.push(Line("}".to_string()));
        }
        None => {}
    }
    Ok(Branch(result))
}

// return (the 'Which' enum, the 'which()' accessor, typedef)
fn generate_union(
    gen: &GeneratorContext,
    discriminant_offset: u32,
    fields: &[schema_capnp::field::Reader],
    is_reader: bool,
    params: &TypeParameterTexts,
) -> ::capnp::Result<(FormattedText, FormattedText, FormattedText)> {
    use schema_capnp::*;

    fn new_ty_param(ty_params: &mut Vec<String>) -> String {
        let result = format!("A{}", ty_params.len());
        ty_params.push(result.clone());
        result
    }

    let mut getter_interior = Vec::new();
    let mut interior = Vec::new();
    let mut enum_interior = Vec::new();

    let mut ty_params = Vec::new();
    let mut ty_args = Vec::new();

    let doffset = discriminant_offset as usize;

    for field in fields.iter() {
        let dvalue = field.get_discriminant_value() as usize;

        let field_name = try!(field.get_name());
        let enumerant_name = capitalize_first_letter(field_name);

        let (ty, get) = try!(getter_text(gen, field, is_reader));

        getter_interior.push(Branch(vec![
            Line(format!("{} => {{", dvalue)),
            Indent(Box::new(Line(format!(
                "return ::std::result::Result::Ok({}(",
                enumerant_name.clone()
            )))),
            Indent(Box::new(Indent(Box::new(get)))),
            Indent(Box::new(Line("));".to_string()))),
            Line("}".to_string()),
        ]));

        let ty1 = match field.which() {
            Ok(field::Group(_)) => {
                ty_args.push(ty);
                new_ty_param(&mut ty_params)
            }
            Ok(field::Slot(reg_field)) => match try!(reg_field.get_type()).which() {
                Ok(type_::Text(()))
                | Ok(type_::Data(()))
                | Ok(type_::List(_))
                | Ok(type_::Struct(_))
                | Ok(type_::AnyPointer(_)) => {
                    ty_args.push(ty);
                    new_ty_param(&mut ty_params)
                }
                Ok(type_::Interface(_)) => ty,
                _ => ty,
            },
            _ => ty,
        };

        enum_interior.push(Line(format!("{}({}),", enumerant_name, ty1)));
    }

    let enum_name = format!(
        "Which{}",
        if ty_params.len() > 0 {
            format!("<{}>", ty_params.join(","))
        } else {
            "".to_string()
        }
    );

    getter_interior.push(Line(
        "x => return ::std::result::Result::Err(::capnp::NotInSchema(x))".to_string(),
    ));

    interior.push(Branch(vec![
        Line(format!("pub enum {} {{", enum_name)),
        Indent(Box::new(Branch(enum_interior))),
        Line("}".to_string()),
    ]));

    let result = Branch(interior);

    let field_name = if is_reader { "reader" } else { "builder" };

    let concrete_type = format!(
        "Which{}{}",
        if is_reader { "Reader" } else { "Builder" },
        if ty_params.len() > 0 {
            format!("<'a,{}>", params.params)
        } else {
            "".to_string()
        }
    );

    let typedef = Line(format!(
        "pub type {} = Which{};",
        concrete_type,
        if ty_args.len() > 0 {
            format!("<{}>", ty_args.join(","))
        } else {
            "".to_string()
        }
    ));

    let getter_result = Branch(vec![
        Line("#[inline]".to_string()),
        Line(format!(
            "pub fn which(self) -> ::std::result::Result<{}, ::capnp::NotInSchema> {{",
            concrete_type
        )),
        Indent(Box::new(Branch(vec![
            Line(format!(
                "match self.{}.get_data_field::<u16>({}) {{",
                field_name, doffset
            )),
            Indent(Box::new(Branch(getter_interior))),
            Line("}".to_string()),
        ]))),
        Line("}".to_string()),
    ]);

    // TODO set_which() for builders?

    Ok((result, getter_result, typedef))
}

fn generate_haser(
    discriminant_offset: u32,
    styled_name: &str,
    field: &schema_capnp::field::Reader,
    is_reader: bool,
) -> ::capnp::Result<FormattedText> {
    use schema_capnp::*;

    let mut result = Vec::new();
    let mut interior = Vec::new();
    let member = if is_reader { "reader" } else { "builder" };

    let discriminant_value = field.get_discriminant_value();
    if discriminant_value != field::NO_DISCRIMINANT {
        interior.push(Line(format!(
            "if self.{}.get_data_field::<u16>({}) != {} {{ return false; }}",
            member, discriminant_offset as usize, discriminant_value as usize
        )));
    }
    match field.which() {
        Err(_) | Ok(field::Group(_)) => {}
        Ok(field::Slot(reg_field)) => match try!(try!(reg_field.get_type()).which()) {
            type_::Text(())
            | type_::Data(())
            | type_::List(_)
            | type_::Struct(_)
            | type_::AnyPointer(_) => {
                interior.push(Line(format!(
                    "!self.{}.get_pointer_field({}).is_null()",
                    member,
                    reg_field.get_offset()
                )));
                result.push(Line(format!(
                    "pub fn has_{}(&self) -> bool {{",
                    styled_name
                )));
                result.push(Indent(Box::new(Branch(interior))));
                result.push(Line("}".to_string()));
            }
            _ => {}
        },
    }

    Ok(Branch(result))
}

fn generate_pipeline_getter(
    gen: &GeneratorContext,
    field: schema_capnp::field::Reader,
) -> ::capnp::Result<FormattedText> {
    use schema_capnp::{field, type_};

    let name = try!(field.get_name());

    match try!(field.which()) {
        field::Group(group) => {
            let the_mod = gen.scope_map[&group.get_type_id()].join("::");
            Ok(Branch(vec![
                Line(format!(
                    "pub fn get_{}(&self) -> {}::Pipeline {{",
                    camel_to_snake_case(name),
                    the_mod
                )),
                Indent(Box::new(Line(
                    "::capnp::capability::FromTypelessPipeline::new(self._typeless.noop())"
                        .to_string(),
                ))),
                Line("}".to_string()),
            ]))
        }
        field::Slot(reg_field) => {
            let typ = try!(reg_field.get_type());
            match try!(typ.which()) {
                type_::Struct(_) | type_::AnyPointer(_) => {
                    Ok(Branch(vec!(
                        Line(format!("pub fn get_{}(&self) -> {} {{",
                                     camel_to_snake_case(name),
                                     try!(typ.type_string(gen, Leaf::Pipeline)))),
                        Indent(Box::new(Line(
                            format!("::capnp::capability::FromTypelessPipeline::new(self._typeless.get_pointer_field({}))",
                                    reg_field.get_offset())))),
                        Line("}".to_string()))))
                }
                type_::Interface(_) => {
                    Ok(Branch(vec!(
                        Line(format!("pub fn get_{}(&self) -> {} {{",
                                     camel_to_snake_case(name),
                                     try!(typ.type_string(gen, Leaf::Client)))),
                        Indent(Box::new(Line(
                            format!("::capnp::capability::FromClientHook::new(self._typeless.get_pointer_field({}).as_cap())",
                                    reg_field.get_offset())))),
                        Line("}".to_string()))))
                }
                _ => {
                    Ok(Branch(Vec::new()))
                }
            }
        }
    }
}

// We need this to work around the fact that Rust does not allow typedefs
// with unused type parameters.
fn get_ty_params_of_brand(
    gen: &GeneratorContext,
    brand: ::schema_capnp::brand::Reader,
) -> ::capnp::Result<String> {
    let mut acc = HashSet::new();
    try!(get_ty_params_of_brand_helper(gen, &mut acc, brand));
    let mut result = String::new();
    for (scope_id, parameter_index) in acc.into_iter() {
        let node = gen.node_map[&scope_id];
        let p = try!(node.get_parameters()).get(parameter_index as u32);
        result.push_str(try!(p.get_name()));
        result.push_str(",");
    }

    Ok(result)
}

fn get_ty_params_of_type_helper(
    gen: &GeneratorContext,
    accumulator: &mut HashSet<(u64, u16)>,
    typ: ::schema_capnp::type_::Reader,
) -> ::capnp::Result<()> {
    use schema_capnp::type_;
    match try!(typ.which()) {
        type_::Void(())
        | type_::Bool(())
        | type_::Int8(())
        | type_::Int16(())
        | type_::Int32(())
        | type_::Int64(())
        | type_::Uint8(())
        | type_::Uint16(())
        | type_::Uint32(())
        | type_::Uint64(())
        | type_::Float32(())
        | type_::Float64(())
        | type_::Text(_)
        | type_::Data(_) => {}
        type_::AnyPointer(p) => {
            match try!(p.which()) {
                type_::any_pointer::Unconstrained(_) => (),
                type_::any_pointer::Parameter(p) => {
                    accumulator.insert((p.get_scope_id(), p.get_parameter_index()));
                }
                type_::any_pointer::ImplicitMethodParameter(_) => {
                    // XXX
                }
            }
        }
        type_::List(list) => try!(get_ty_params_of_type_helper(
            gen,
            accumulator,
            try!(list.get_element_type())
        )),
        type_::Enum(e) => {
            try!(get_ty_params_of_brand_helper(
                gen,
                accumulator,
                try!(e.get_brand())
            ));
        }
        type_::Struct(s) => {
            try!(get_ty_params_of_brand_helper(
                gen,
                accumulator,
                try!(s.get_brand())
            ));
        }
        type_::Interface(interf) => {
            try!(get_ty_params_of_brand_helper(
                gen,
                accumulator,
                try!(interf.get_brand())
            ));
        }
    }
    Ok(())
}

fn get_ty_params_of_brand_helper(
    gen: &GeneratorContext,
    accumulator: &mut HashSet<(u64, u16)>,
    brand: ::schema_capnp::brand::Reader,
) -> ::capnp::Result<()> {
    for scope in try!(brand.get_scopes()).iter() {
        let scope_id = scope.get_scope_id();
        match try!(scope.which()) {
            ::schema_capnp::brand::scope::Bind(bind) => {
                for binding in try!(bind).iter() {
                    match try!(binding.which()) {
                        ::schema_capnp::brand::binding::Unbound(()) => {}
                        ::schema_capnp::brand::binding::Type(t) => {
                            try!(get_ty_params_of_type_helper(gen, accumulator, try!(t)))
                        }
                    }
                }
            }
            ::schema_capnp::brand::scope::Inherit(()) => {
                let parameters = try!(gen.node_map[&scope_id].get_parameters());
                for idx in 0..parameters.len() {
                    accumulator.insert((scope_id, idx as u16));
                }
            }
        }
    }
    Ok(())
}

fn generate_node(
    gen: &GeneratorContext,
    node_id: u64,
    node_name: &str,
    // Ugh. We need this to deal with the anonymous Params and Results
    // structs that go with RPC methods.
    parent_node_id: Option<u64>,
) -> ::capnp::Result<FormattedText> {
    use schema_capnp::*;

    let mut output: Vec<FormattedText> = Vec::new();
    let mut nested_output: Vec<FormattedText> = Vec::new();

    let node_reader = &gen.node_map[&node_id];
    let nested_nodes = try!(node_reader.get_nested_nodes());
    for nested_node in nested_nodes.iter() {
        let id = nested_node.get_id();
        nested_output.push(try!(generate_node(
            gen,
            id,
            try!(gen.get_last_name(id)),
            None
        )));
    }

    match try!(node_reader.which()) {
        node::File(()) => {
            output.push(Branch(nested_output));
        }
        node::Struct(struct_reader) => {
            let params = node_reader.parameters_texts(gen, parent_node_id);
            output.push(BlankLine);

            let is_generic = node_reader.get_is_generic();
            if is_generic {
                output.push(Line(format!(
                    "pub mod {} {{ /* {} */",
                    node_name,
                    params.expanded_list.join(",")
                )));
            } else {
                output.push(Line(format!("pub mod {} {{", node_name)));
            }
            let bracketed_params = if params.params == "" {
                "".to_string()
            } else {
                format!("<{}>", params.params)
            };

            let mut preamble = Vec::new();
            let mut builder_members = Vec::new();
            let mut reader_members = Vec::new();
            let mut union_fields = Vec::new();
            let mut which_enums = Vec::new();
            let mut pipeline_impl_interior = Vec::new();
            let mut private_mod_interior = Vec::new();

            let data_size = struct_reader.get_data_word_count();
            let pointer_size = struct_reader.get_pointer_count();
            let discriminant_count = struct_reader.get_discriminant_count();
            let discriminant_offset = struct_reader.get_discriminant_offset();

            let fields = try!(struct_reader.get_fields());
            for field in fields.iter() {
                let name = try!(field.get_name());
                let styled_name = camel_to_snake_case(name);

                let discriminant_value = field.get_discriminant_value();
                let is_union_field = discriminant_value != field::NO_DISCRIMINANT;

                if !is_union_field {
                    pipeline_impl_interior.push(try!(generate_pipeline_getter(gen, field)));
                    let (ty, get) = try!(getter_text(gen, &field, true));
                    reader_members.push(Branch(vec![
                        Line("#[inline]".to_string()),
                        Line(format!("pub fn get_{}(self) -> {} {{", styled_name, ty)),
                        Indent(Box::new(get)),
                        Line("}".to_string()),
                    ]));

                    let (ty_b, get_b) = try!(getter_text(gen, &field, false));

                    builder_members.push(Branch(vec![
                        Line("#[inline]".to_string()),
                        Line(format!("pub fn get_{}(self) -> {} {{", styled_name, ty_b)),
                        Indent(Box::new(get_b)),
                        Line("}".to_string()),
                    ]));
                } else {
                    union_fields.push(field);
                }

                builder_members.push(try!(generate_setter(
                    gen,
                    discriminant_offset,
                    &styled_name,
                    &field
                )));

                reader_members.push(try!(generate_haser(
                    discriminant_offset,
                    &styled_name,
                    &field,
                    true
                )));
                builder_members.push(try!(generate_haser(
                    discriminant_offset,
                    &styled_name,
                    &field,
                    false
                )));

                match field.which() {
                    Ok(field::Group(group)) => {
                        let id = group.get_type_id();
                        let text = try!(generate_node(gen, id, try!(gen.get_last_name(id)), None));
                        nested_output.push(text);
                    }
                    _ => {}
                }
            }

            if discriminant_count > 0 {
                let (which_enums1, union_getter, typedef) = try!(generate_union(
                    gen,
                    discriminant_offset,
                    &union_fields,
                    true,
                    &params
                ));
                which_enums.push(which_enums1);
                which_enums.push(typedef);
                reader_members.push(union_getter);

                let (_, union_getter, typedef) = try!(generate_union(
                    gen,
                    discriminant_offset,
                    &union_fields,
                    false,
                    &params
                ));
                which_enums.push(typedef);
                builder_members.push(union_getter);

                let mut reexports = String::new();
                reexports.push_str("pub use self::Which::{");
                let mut whichs = Vec::new();
                for f in union_fields.iter() {
                    whichs.push(capitalize_first_letter(try!(f.get_name())));
                }
                reexports.push_str(&whichs.join(","));
                reexports.push_str("};");
                preamble.push(Line(reexports));
                preamble.push(BlankLine);
            }

            let builder_struct_size =
                Branch(vec!(
                    Line(format!("impl <'a,{0}> ::capnp::traits::HasStructSize for Builder<'a,{0}> {1} {{",
                                 params.params, params.where_clause)),
                    Indent(Box::new(
                        Branch(vec!(Line("#[inline]".to_string()),
                                    Line("fn struct_size() -> ::capnp::private::layout::StructSize { _private::STRUCT_SIZE }".to_string()))))),
                   Line("}".to_string())));

            private_mod_interior.push(Line("use capnp::private::layout;".to_string()));
            private_mod_interior.push(
                Line(
                    format!("pub const STRUCT_SIZE: layout::StructSize = layout::StructSize {{ data: {}, pointers: {} }};",
                            data_size as usize, pointer_size as usize)));
            private_mod_interior.push(Line(format!("pub const TYPE_ID: u64 = {:#x};", node_id)));

            let from_pointer_builder_impl =
                Branch(vec![
                    Line(format!("impl <'a,{0}> ::capnp::traits::FromPointerBuilder<'a> for Builder<'a,{0}> {1} {{", params.params, params.where_clause)),
                    Indent(
                        Box::new(
                            Branch(vec!(
                                Line(format!("fn init_pointer(builder: ::capnp::private::layout::PointerBuilder<'a>, _size: u32) -> Builder<'a,{}> {{", params.params)),
                                Indent(Box::new(Line("::capnp::traits::FromStructBuilder::new(builder.init_struct(_private::STRUCT_SIZE))".to_string()))),
                                Line("}".to_string()),
                                Line(format!("fn get_from_pointer(builder: ::capnp::private::layout::PointerBuilder<'a>) -> ::capnp::Result<Builder<'a,{}>> {{", params.params)),
                                Indent(Box::new(Line("::std::result::Result::Ok(::capnp::traits::FromStructBuilder::new(try!(builder.get_struct(_private::STRUCT_SIZE, ::std::ptr::null()))))".to_string()))),
                                Line("}".to_string()))))),
                    Line("}".to_string()),
                    BlankLine]);

            let accessors = vec![
                Branch(preamble),
                (if !is_generic {
                    Branch(vec!(
                        Line("#[derive(Copy, Clone)]".into()),
                        Line("pub struct Owned;".to_string()),
                        Line("impl <'a> ::capnp::traits::Owned<'a> for Owned { type Reader = Reader<'a>; type Builder = Builder<'a>; }".to_string()),
                        Line("impl <'a> ::capnp::traits::OwnedStruct<'a> for Owned { type Reader = Reader<'a>; type Builder = Builder<'a>; }".to_string()),
                        Line("impl ::capnp::traits::Pipelined for Owned { type Pipeline = Pipeline; }".to_string())
                    ))
                } else {
                    Branch(vec!(
                        Line("#[derive(Copy, Clone)]".into()),
                        Line(format!("pub struct Owned<{}> {{", params.params)),
                            Indent(Box::new(Line(format!("_phantom: ::std::marker::PhantomData<({})>", params.params)))),
                        Line("}".to_string()),
                        Line(format!("impl <'a, {0}> ::capnp::traits::Owned<'a> for Owned <{0}> {1} {{ type Reader = Reader<'a, {0}>; type Builder = Builder<'a, {0}>; }}",
                            params.params, params.where_clause)),
                        Line(format!("impl <'a, {0}> ::capnp::traits::OwnedStruct<'a> for Owned <{0}> {1} {{ type Reader = Reader<'a, {0}>; type Builder = Builder<'a, {0}>; }}",
                            params.params, params.where_clause)),
                        Line(format!("impl <{0}> ::capnp::traits::Pipelined for Owned<{0}> {1} {{ type Pipeline = Pipeline{2}; }}",
                            params.params, params.where_clause, bracketed_params)),
                    ))
                }),
                BlankLine,
                Line("#[derive(Clone, Copy)]".to_string()),
                (if !is_generic {
                    Line("pub struct Reader<'a> { reader: ::capnp::private::layout::StructReader<'a> }".to_string())
                } else {
                    Branch(vec!(
                        Line(format!("pub struct Reader<'a,{}> {} {{", params.params, params.where_clause)),
                        Indent(Box::new(Branch(vec!(
                            Line("reader: ::capnp::private::layout::StructReader<'a>,".to_string()),
                            Line(format!("_phantom: ::std::marker::PhantomData<({})>", params.params)),
                        )))),
                        Line("}".to_string())
                    ))
                }),
                BlankLine,
                Branch(vec!(
                        Line(format!("impl <'a,{0}> ::capnp::traits::HasTypeId for Reader<'a,{0}> {1} {{",
                            params.params, params.where_clause)),
                        Indent(Box::new(Branch(vec!(Line("#[inline]".to_string()),
                                               Line("fn type_id() -> u64 { _private::TYPE_ID }".to_string()))))),
                    Line("}".to_string()))),
                Line(format!("impl <'a,{0}> ::capnp::traits::FromStructReader<'a> for Reader<'a,{0}> {1} {{",
                            params.params, params.where_clause)),
                Indent(
                    Box::new(Branch(vec!(
                        Line(format!("fn new(reader: ::capnp::private::layout::StructReader<'a>) -> Reader<'a,{}> {{", params.params)),
                        Indent(Box::new(Line(format!("Reader {{ reader: reader, {} }}", params.phantom_data)))),
                        Line("}".to_string()))))),
                Line("}".to_string()),
                BlankLine,
                Line(format!("impl <'a,{0}> ::capnp::traits::FromPointerReader<'a> for Reader<'a,{0}> {1} {{",
                    params.params, params.where_clause)),
                Indent(
                    Box::new(Branch(vec!(
                        Line(format!("fn get_from_pointer(reader: &::capnp::private::layout::PointerReader<'a>) -> ::capnp::Result<Reader<'a,{}>> {{",params.params)),
                        Indent(Box::new(Line("::std::result::Result::Ok(::capnp::traits::FromStructReader::new(try!(reader.get_struct(::std::ptr::null()))))".to_string()))),
                        Line("}".to_string()))))),
                Line("}".to_string()),
                BlankLine,
                Line(format!("impl <'a,{0}> ::capnp::traits::Imbue<'a> for Reader<'a,{0}> {1} {{",
                    params.params, params.where_clause)),
                Indent(
                    Box::new(Branch(vec!(
                        Line("fn imbue(&mut self, cap_table: &'a ::capnp::private::layout::CapTable) {".to_string()),
                        Indent(Box::new(Line("self.reader.imbue(::capnp::private::layout::CapTableReader::Plain(cap_table))".to_string()))),
                        Line("}".to_string()))))),
                Line("}".to_string()),
                BlankLine,
                Line(format!("impl <'a,{0}> Reader<'a,{0}> {1} {{", params.params, params.where_clause)),
                Indent(
                    Box::new(Branch(vec![
                        Line(format!("pub fn reborrow<'b>(&'b self) -> Reader<'b,{}> {{",params.params)),
                        Indent(Box::new(Line("Reader { .. *self }".to_string()))),
                        Line("}".to_string()),
                        BlankLine,
                        Line("pub fn total_size(&self) -> ::capnp::Result<::capnp::MessageSize> {".to_string()),
                        Indent(Box::new(Line("self.reader.total_size()".to_string()))),
                        Line("}".to_string())]))),
                Indent(Box::new(Branch(reader_members))),
                Line("}".to_string()),
                BlankLine,
                (if !is_generic {
                    Line("pub struct Builder<'a> { builder: ::capnp::private::layout::StructBuilder<'a> }".to_string())
                } else {
                    Branch(vec!(
                        Line(format!("pub struct Builder<'a,{}> {} {{",
                                     params.params, params.where_clause)),
                        Indent(Box::new(Branch(vec!(
                            Line("builder: ::capnp::private::layout::StructBuilder<'a>,".to_string()),
                            Line(format!("_phantom: ::std::marker::PhantomData<({})>", params.params)),
                        )))),
                        Line("}".to_string())
                    ))
                }),
                builder_struct_size,
                Branch(vec!(
                    Line(format!("impl <'a,{0}> ::capnp::traits::HasTypeId for Builder<'a,{0}> {1} {{",
                                 params.params, params.where_clause)),
                    Indent(Box::new(Branch(vec!(
                        Line("#[inline]".to_string()),
                        Line("fn type_id() -> u64 { _private::TYPE_ID }".to_string()))))),
                    Line("}".to_string()))),
                Line(format!(
                    "impl <'a,{0}> ::capnp::traits::FromStructBuilder<'a> for Builder<'a,{0}> {1} {{",
                    params.params, params.where_clause)),
                Indent(
                    Box::new(Branch(vec!(
                        Line(format!("fn new(builder: ::capnp::private::layout::StructBuilder<'a>) -> Builder<'a, {}> {{", params.params)),
                        Indent(Box::new(Line(format!("Builder {{ builder: builder, {} }}", params.phantom_data)))),
                        Line("}".to_string()))))),
                Line("}".to_string()),
                BlankLine,
                Line(format!("impl <'a,{0}> ::capnp::traits::ImbueMut<'a> for Builder<'a,{0}> {1} {{",
                             params.params, params.where_clause)),
                Indent(
                    Box::new(Branch(vec!(
                        Line("fn imbue_mut(&mut self, cap_table: &'a mut ::capnp::private::layout::CapTable) {".to_string()),
                        Indent(Box::new(Line("self.builder.imbue(::capnp::private::layout::CapTableBuilder::Plain(cap_table))".to_string()))),
                        Line("}".to_string()))))),
                Line("}".to_string()),
                BlankLine,

                from_pointer_builder_impl,
                Line(format!(
                    "impl <'a,{0}> ::capnp::traits::SetPointerBuilder<Builder<'a,{0}>> for Reader<'a,{0}> {1} {{",
                    params.params, params.where_clause)),
                Indent(Box::new(Line(format!("fn set_pointer_builder<'b>(pointer: ::capnp::private::layout::PointerBuilder<'b>, value: Reader<'a,{}>, canonicalize: bool) -> ::capnp::Result<()> {{ pointer.set_struct(&value.reader, canonicalize) }}", params.params)))),
                Line("}".to_string()),
                BlankLine,
                Line(format!("impl <'a,{0}> Builder<'a,{0}> {1} {{", params.params, params.where_clause)),
                Indent(
                    Box::new(Branch(vec![
                        Line(format!("pub fn as_reader(self) -> Reader<'a,{}> {{", params.params)),
                        Indent(Box::new(Line("::capnp::traits::FromStructReader::new(self.builder.as_reader())".to_string()))),
                        Line("}".to_string()),
                        Line(format!("pub fn reborrow<'b>(&'b mut self) -> Builder<'b,{}> {{", params.params)),
                        Indent(Box::new(Line("Builder { .. *self }".to_string()))),
                        Line("}".to_string()),
                        Line(format!("pub fn reborrow_as_reader<'b>(&'b self) -> Reader<'b,{}> {{", params.params)),
                        Indent(Box::new(Line("::capnp::traits::FromStructReader::new(self.builder.as_reader())".to_string()))),
                        Line("}".to_string()),

                        BlankLine,
                        Line("pub fn total_size(&self) -> ::capnp::Result<::capnp::MessageSize> {".to_string()),
                        Indent(Box::new(Line("self.builder.as_reader().total_size()".to_string()))),
                        Line("}".to_string())
                        ]))),
                Indent(Box::new(Branch(builder_members))),
                Line("}".to_string()),
                BlankLine,
                (if is_generic {
                    Branch(vec![
                        Line(format!("pub struct Pipeline{} {{", bracketed_params)),
                        Indent(Box::new(Branch(vec![
                            Line("_typeless: ::capnp::any_pointer::Pipeline,".to_string()),
                            Line(format!("_phantom: ::std::marker::PhantomData<({})>", params.params)),
                        ]))),
                        Line("}".to_string())
                    ])
                } else {
                    Line("pub struct Pipeline { _typeless: ::capnp::any_pointer::Pipeline }".to_string())
                }),
                Line(format!("impl{} ::capnp::capability::FromTypelessPipeline for Pipeline{} {{", bracketed_params, bracketed_params)),
                Indent(
                    Box::new(Branch(vec!(
                        Line(format!("fn new(typeless: ::capnp::any_pointer::Pipeline) -> Pipeline{} {{", bracketed_params)),
                        Indent(Box::new(Line(format!("Pipeline {{ _typeless: typeless, {} }}", params.phantom_data)))),
                        Line("}".to_string()))))),
                Line("}".to_string()),
                Line(format!("impl{0} Pipeline{0} {1} {{", bracketed_params,
                             params.pipeline_where_clause)),
                Indent(Box::new(Branch(pipeline_impl_interior))),
                Line("}".to_string()),
                Line("mod _private {".to_string()),
                Indent(Box::new(Branch(private_mod_interior))),
                Line("}".to_string()),
            ];

            output.push(Indent(Box::new(Branch(vec![
                Branch(accessors),
                Branch(which_enums),
                Branch(nested_output),
            ]))));
            output.push(Line("}".to_string()));
        }

        node::Enum(enum_reader) => {
            let last_name = try!(gen.get_last_name(node_id));
            output.push(BlankLine);

            let mut members = Vec::new();
            let mut match_branches = Vec::new();
            let enumerants = try!(enum_reader.get_enumerants());
            for ii in 0..enumerants.len() {
                let enumerant = capitalize_first_letter(try!(enumerants.get(ii).get_name()));
                members.push(Line(format!("{} = {},", enumerant, ii)));
                match_branches.push(Line(format!(
                    "{} => ::std::result::Result::Ok({}::{}),",
                    ii, last_name, enumerant
                )));
            }
            match_branches.push(Line(
                "n => ::std::result::Result::Err(::capnp::NotInSchema(n)),".to_string(),
            ));

            output.push(Branch(vec![
                Line("#[repr(u16)]".to_string()),
                Line("#[derive(Clone, Copy, PartialEq)]".to_string()),
                Line(format!("pub enum {} {{", last_name)),
                Indent(Box::new(Branch(members))),
                Line("}".to_string()),
            ]));

            output.push(
                Branch(vec!(
                    Line(format!("impl ::capnp::traits::FromU16 for {} {{", last_name)),
                    Indent(Box::new(Line("#[inline]".to_string()))),
                    Indent(
                        Box::new(Branch(vec![
                            Line(format!(
                                "fn from_u16(value: u16) -> ::std::result::Result<{}, ::capnp::NotInSchema> {{",
                                last_name)),
                            Indent(
                                Box::new(Branch(vec![
                                    Line("match value {".to_string()),
                                    Indent(Box::new(Branch(match_branches))),
                                    Line("}".to_string())
                                        ]))),
                            Line("}".to_string())]))),
                    Line("}".to_string()),
                    Line(format!("impl ::capnp::traits::ToU16 for {} {{", last_name)),
                    Indent(Box::new(Line("#[inline]".to_string()))),
                    Indent(
                        Box::new(Line("fn to_u16(self) -> u16 { self as u16 }".to_string()))),
                    Line("}".to_string()))));

            output.push(Branch(vec![
                Line(format!(
                    "impl ::capnp::traits::HasTypeId for {} {{",
                    last_name
                )),
                Indent(Box::new(Line("#[inline]".to_string()))),
                Indent(Box::new(Line(
                    format!("fn type_id() -> u64 {{ {:#x}u64 }}", node_id).to_string(),
                ))),
                Line("}".to_string()),
            ]));
        }

        node::Interface(interface) => {
            let params = node_reader.parameters_texts(gen, parent_node_id);
            output.push(BlankLine);

            let is_generic = node_reader.get_is_generic();

            let names = &gen.scope_map[&node_id];
            let mut client_impl_interior = Vec::new();
            let mut server_interior = Vec::new();
            let mut mod_interior = Vec::new();
            let mut dispatch_arms = Vec::new();
            let mut private_mod_interior = Vec::new();

            let bracketed_params = if params.params == "" {
                "".to_string()
            } else {
                format!("<{}>", params.params)
            };

            private_mod_interior.push(Line(format!("pub const TYPE_ID: u64 = {:#x};", node_id)));

            mod_interior.push(Line("#![allow(unused_variables)]".to_string()));

            let methods = try!(interface.get_methods());
            for ordinal in 0..methods.len() {
                let method = methods.get(ordinal);
                let name = try!(method.get_name());

                method.get_code_order();
                let param_id = method.get_param_struct_type();
                let param_node = &gen.node_map[&param_id];
                let (param_scopes, params_ty_params) = if param_node.get_scope_id() == 0 {
                    let mut names = names.clone();
                    let local_name = module_name(&format!("{}Params", name));
                    nested_output.push(try!(generate_node(
                        gen,
                        param_id,
                        &*local_name,
                        Some(node_id)
                    )));
                    names.push(local_name);
                    (names, params.params.clone())
                } else {
                    (
                        gen.scope_map[&param_node.get_id()].clone(),
                        try!(get_ty_params_of_brand(gen, try!(method.get_param_brand()))),
                    )
                };
                let param_type = try!(do_branding(
                    &gen,
                    param_id,
                    try!(method.get_param_brand()),
                    Leaf::Owned,
                    param_scopes.join("::"),
                    Some(node_id)
                ));

                let result_id = method.get_result_struct_type();
                let result_node = &gen.node_map[&result_id];
                let (result_scopes, results_ty_params) = if result_node.get_scope_id() == 0 {
                    let mut names = names.clone();
                    let local_name = module_name(&format!("{}Results", name));
                    nested_output.push(try!(generate_node(
                        gen,
                        result_id,
                        &*local_name,
                        Some(node_id)
                    )));
                    names.push(local_name);
                    (names, params.params.clone())
                } else {
                    (
                        gen.scope_map[&result_node.get_id()].clone(),
                        try!(get_ty_params_of_brand(gen, try!(method.get_result_brand()))),
                    )
                };
                let result_type = try!(do_branding(
                    &gen,
                    result_id,
                    try!(method.get_result_brand()),
                    Leaf::Owned,
                    result_scopes.join("::"),
                    Some(node_id)
                ));

                dispatch_arms.push(
                    Line(format!(
                        "{} => server.{}(::capnp::private::capability::internal_get_typed_params(params), ::capnp::private::capability::internal_get_typed_results(results)),",
                        ordinal, module_name(name))));
                mod_interior.push(Line(format!(
                    "pub type {}Params<{}> = ::capnp::capability::Params<{}>;",
                    capitalize_first_letter(name),
                    params_ty_params,
                    param_type
                )));
                mod_interior.push(Line(format!(
                    "pub type {}Results<{}> = ::capnp::capability::Results<{}>;",
                    capitalize_first_letter(name),
                    results_ty_params,
                    result_type
                )));
                server_interior.push(
                    Line(format!(
                        "fn {}(&mut self, _: {}Params<{}>, _: {}Results<{}>) -> ::capnp::capability::Promise<(), ::capnp::Error> {{ ::capnp::capability::Promise::err(::capnp::Error::unimplemented(\"method not implemented\".to_string())) }}",
                        module_name(name),
                        capitalize_first_letter(name), params_ty_params,
                        capitalize_first_letter(name), results_ty_params
                    )));

                client_impl_interior.push(Line(format!(
                    "pub fn {}_request(&self) -> ::capnp::capability::Request<{},{}> {{",
                    camel_to_snake_case(name),
                    param_type,
                    result_type
                )));

                client_impl_interior.push(Indent(Box::new(Line(format!(
                    "self.client.new_call(_private::TYPE_ID, {}, None)",
                    ordinal
                )))));
                client_impl_interior.push(Line("}".to_string()));

                try!(method.get_annotations());
            }

            let mut base_dispatch_arms = Vec::new();
            let server_base = {
                let mut base_traits = Vec::new();
                let extends = try!(interface.get_superclasses());
                for ii in 0..extends.len() {
                    let type_id = extends.get(ii).get_id();
                    let brand = try!(extends.get(ii).get_brand());
                    let the_mod = gen.scope_map[&type_id].join("::");

                    base_dispatch_arms.push(Line(format!(
                        "0x{:x} => {}::dispatch_call_internal(&mut *self.server, method_id, params, results),",
                        type_id,
                        try!(do_branding(
                            gen, type_id, brand, Leaf::ServerDispatch, the_mod.clone(), None)))));
                    base_traits.push(try!(do_branding(
                        gen,
                        type_id,
                        brand,
                        Leaf::Server,
                        the_mod,
                        None
                    )));
                }
                if extends.len() > 0 {
                    format!(": {}", base_traits.join(" + "))
                } else {
                    "".to_string()
                }
            };

            mod_interior.push(BlankLine);
            mod_interior.push(Line(format!("pub struct Client{} {{", bracketed_params)));
            mod_interior.push(Indent(Box::new(Line(
                "pub client: ::capnp::capability::Client,".to_string(),
            ))));
            if is_generic {
                mod_interior.push(Indent(Box::new(Line(format!(
                    "_phantom: ::std::marker::PhantomData<({})>",
                    params.params
                )))))
            }
            mod_interior.push(Line("}".to_string()));
            mod_interior.push(Branch(vec![
                Line(format!(
                    "impl {} ::capnp::capability::FromClientHook for Client{} {{",
                    bracketed_params, bracketed_params
                )),
                Indent(Box::new(Line(format!(
                    "fn new(hook: Box<::capnp::private::capability::ClientHook>) -> Client{} {{",
                    bracketed_params
                )))),
                Indent(Box::new(Indent(Box::new(Line(format!(
                    "Client {{ client: ::capnp::capability::Client::new(hook), {} }}",
                    params.phantom_data
                )))))),
                Indent(Box::new(Line("}".to_string()))),
                Line("}".to_string()),
            ]));

            mod_interior.push(if !is_generic {
                Branch(vec!(
                    Line("#[derive(Copy, Clone)]".into()),
                    Line("pub struct Owned;".to_string()),
                    Line("impl <'a> ::capnp::traits::Owned<'a> for Owned { type Reader = Client; type Builder = Client; }".to_string()),
                    Line("impl ::capnp::traits::Pipelined for Owned { type Pipeline = Client; }".to_string())))
            } else {
                Branch(vec!(
                    Line("#[derive(Copy, Clone)]".into()),
                    Line(format!("pub struct Owned<{}> {} {{", params.params, params.where_clause)),
                    Indent(Box::new(Line(format!("_phantom: ::std::marker::PhantomData<({})>", params.params)))),
                    Line("}".to_string()),
                    Line(format!(
                        "impl <'a, {0}> ::capnp::traits::Owned<'a> for Owned <{0}> {1} {{ type Reader = Client<{0}>; type Builder = Client<{0}>; }}",
                        params.params, params.where_clause)),
                    Line(format!(
                        "impl <{0}> ::capnp::traits::Pipelined for Owned <{0}> {1} {{ type Pipeline = Client{2}; }}",
                        params.params, params.where_clause, bracketed_params))))
            });

            mod_interior.push(Branch(vec!(
                Line(format!("impl <'a,{0}> ::capnp::traits::FromPointerReader<'a> for Client<{0}> {1} {{",
                    params.params, params.where_clause)),
                Indent(
                    Box::new(Branch(vec![
                        Line(format!("fn get_from_pointer(reader: &::capnp::private::layout::PointerReader<'a>) -> ::capnp::Result<Client<{}>> {{",params.params)),
                        Indent(Box::new(Line(format!("::std::result::Result::Ok(::capnp::capability::FromClientHook::new(try!(reader.get_capability())))")))),
                        Line("}".to_string())]))),
                Line("}".to_string()))));

            mod_interior.push(Branch(vec![
                Line(format!("impl <'a,{0}> ::capnp::traits::FromPointerBuilder<'a> for Client<{0}> {1} {{",
                             params.params, params.where_clause)),
                Indent(
                    Box::new(
                        Branch(vec![
                            Line(format!("fn init_pointer(_builder: ::capnp::private::layout::PointerBuilder<'a>, _size: u32) -> Client<{}> {{", params.params)),
                            Indent(Box::new(Line("unimplemented!()".to_string()))),
                            Line("}".to_string()),
                            Line(format!("fn get_from_pointer(builder: ::capnp::private::layout::PointerBuilder<'a>) -> ::capnp::Result<Client<{}>> {{", params.params)),
                            Indent(Box::new(Line("::std::result::Result::Ok(::capnp::capability::FromClientHook::new(try!(builder.get_capability())))".to_string()))),
                            Line("}".to_string())]))),
                Line("}".to_string()),
                BlankLine]));

            mod_interior.push(Branch(vec![
                Line(format!(
                    "impl <{0}> ::capnp::traits::SetPointerBuilder<Client<{0}>> for Client<{0}> {1} {{",
                    params.params, params.where_clause)),
                Indent(
                    Box::new(
                        Branch(vec![
                            Line(format!("fn set_pointer_builder<'a>(pointer: ::capnp::private::layout::PointerBuilder<'a>, from: Client<{}>, _canonicalize: bool) -> ::capnp::Result<()> {{",
                                         params.params)),
                            Indent(Box::new(Line(
                                "::std::result::Result::Ok(pointer.set_capability(from.client.hook))".to_string()))),
                            Line("}".to_string())]))),
                Line("}".to_string())]));

            mod_interior.push(
                Branch(vec!(
                    (if is_generic {
                        Branch(vec!(
                            Line(format!("pub struct ToClient<U,{}> {{", params.params)),
                            Indent(Box::new(Branch(vec!(
                                Line("pub u: U,".to_string()),
                                Line(format!("_phantom: ::std::marker::PhantomData<({})>", params.params))
                            )))),
                            Line("}".to_string()),
                            Line(format!("impl <{0}, U: Server<{0}> + 'static> ToClient<U,{0}> {1} {{",
                                         params.params, params.where_clause_with_send)),
                            Indent(Box::new(Line(format!(
                                "pub fn new(u: U) -> ToClient<U, {}> {{ ToClient {{u: u, _phantom: ::std::marker::PhantomData}} }}",
                                params.params)))),
                        ))
                    } else {
                        Branch(vec!(
                            Line("pub struct ToClient<U>{pub u: U}".to_string()),
                            Line("impl <U: Server + 'static> ToClient<U> {".to_string()),
                            Line("pub fn new(u: U) -> ToClient<U> { ToClient {u: u} }".to_string()),
                        ))
                    }),
                    Indent(Box::new(Branch( vec!(
                        Line(format!("pub fn from_server<_T: ::capnp::private::capability::ServerHook>(self) -> Client{} {{", bracketed_params)),
                        Indent(
                            Box::new(Line(format!("Client {{ client: _T::new_client(::std::boxed::Box::new(ServerDispatch {{ server: ::std::boxed::Box::new(self.u), {} }})), {} }}", params.phantom_data, params.phantom_data)))),
                        Line("}".to_string()))))),
                    Line("}".to_string()))));

            mod_interior.push(Branch(vec![
                Line(format!(
                    "impl {0} ::capnp::traits::HasTypeId for Client{0} {{",
                    bracketed_params
                )),
                Indent(Box::new(Line("#[inline]".to_string()))),
                Indent(Box::new(Line(
                    "fn type_id() -> u64 { _private::TYPE_ID }".to_string(),
                ))),
                Line("}".to_string()),
            ]));

            mod_interior.push(
                Branch(vec!(
                    Line(format!("impl {0} Clone for Client{0} {{", bracketed_params)),
                    Indent(Box::new(Line(format!("fn clone(&self) -> Client{} {{", bracketed_params)))),
                    Indent(Box::new(Indent(Box::new(Line(format!("Client {{ client: ::capnp::capability::Client::new(self.client.hook.add_ref()), {} }}", params.phantom_data)))))),
                    Indent(Box::new(Line("}".to_string()))),
                    Line("}".to_string()))));

            mod_interior.push(Branch(vec![
                Line(format!("impl {0} Client{0} {{", bracketed_params)),
                Indent(Box::new(Branch(client_impl_interior))),
                Line("}".to_string()),
            ]));

            mod_interior.push(Branch(vec![
                Line(format!(
                    "pub trait Server<{}> {} {{",
                    params.params, server_base
                )),
                Indent(Box::new(Branch(server_interior))),
                Line("}".to_string()),
            ]));

            mod_interior.push(Branch(vec![
                Line(format!(
                    "pub struct ServerDispatch<_T,{}> {{",
                    params.params
                )),
                Indent(Box::new(Line("pub server: Box<_T>,".to_string()))),
                Indent(Box::new(Branch(if is_generic {
                    vec![Line(format!(
                        "_phantom: ::std::marker::PhantomData<({})>",
                        params.params
                    ))]
                } else {
                    vec![]
                }))),
                Line("}".to_string()),
            ]));

            mod_interior.push(
                Branch(vec!(
                    (if is_generic {
                        Line(format!("impl <{}, _T: Server{}> ::capnp::capability::Server for ServerDispatch<_T,{}> {{", params.params, bracketed_params, params.params))
                    } else {
                        Line("impl <_T: Server> ::capnp::capability::Server for ServerDispatch<_T> {".to_string())
                    }),
                    Indent(Box::new(Line("fn dispatch_call(&mut self, interface_id: u64, method_id: u16, params: ::capnp::capability::Params<::capnp::any_pointer::Owned>, results: ::capnp::capability::Results<::capnp::any_pointer::Owned>) -> ::capnp::capability::Promise<(), ::capnp::Error> {".to_string()))),
                    Indent(Box::new(Indent(Box::new(Line("match interface_id {".to_string()))))),
                    Indent(Box::new(Indent(Box::new(Indent(
                        Box::new(Line(format!("_private::TYPE_ID => ServerDispatch::<_T, {}>::dispatch_call_internal(&mut *self.server, method_id, params, results),",params.params)))))))),
                    Indent(Box::new(Indent(Box::new(Indent(Box::new(Branch(base_dispatch_arms))))))),
                    Indent(Box::new(Indent(Box::new(Indent(Box::new(Line("_ => { ::capnp::capability::Promise::err(::capnp::Error::unimplemented(\"Method not implemented.\".to_string())) }".to_string()))))))),
                    Indent(Box::new(Indent(Box::new(Line("}".to_string()))))),
                    Indent(Box::new(Line("}".to_string()))),
                    Line("}".to_string()))));

            mod_interior.push(
                Branch(vec!(
                    (if is_generic {
                        Line(format!("impl <{}, _T: Server{}> ServerDispatch<_T,{}> {{", params.params, bracketed_params, params.params))
                    } else {
                        Line("impl <_T :Server> ServerDispatch<_T> {".to_string())
                    }),
                    Indent(Box::new(Line("pub fn dispatch_call_internal(server: &mut _T, method_id: u16, params: ::capnp::capability::Params<::capnp::any_pointer::Owned>, results: ::capnp::capability::Results<::capnp::any_pointer::Owned>) -> ::capnp::capability::Promise<(), ::capnp::Error> {".to_string()))),
                    Indent(Box::new(Indent(Box::new(Line("match method_id {".to_string()))))),
                    Indent(Box::new(Indent(Box::new(Indent(Box::new(Branch(dispatch_arms))))))),
                    Indent(Box::new(Indent(Box::new(Indent(Box::new(Line("_ => { ::capnp::capability::Promise::err(::capnp::Error::unimplemented(\"Method not implemented.\".to_string())) }".to_string()))))))),
                    Indent(Box::new(Indent(Box::new(Line("}".to_string()))))),
                    Indent(Box::new(Line("}".to_string()))),
                    Line("}".to_string()))));

            mod_interior.push(Branch(vec![
                Line("pub mod _private {".to_string()),
                Indent(Box::new(Branch(private_mod_interior))),
                Line("}".to_string()),
            ]));

            mod_interior.push(Branch(vec![Branch(nested_output)]));

            output.push(BlankLine);
            if is_generic {
                output.push(Line(format!(
                    "pub mod {} {{ /* ({}) */",
                    node_name,
                    params.expanded_list.join(",")
                )));
            } else {
                output.push(Line(format!("pub mod {} {{", node_name)));
            }
            output.push(Indent(Box::new(Branch(mod_interior))));
            output.push(Line("}".to_string()));
        }

        node::Const(c) => {
            let styled_name = snake_to_upper_case(try!(gen.get_last_name(node_id)));

            let typ = try!(c.get_type());
            let formatted_text = match (try!(typ.which()), try!(try!(c.get_value()).which())) {
                (type_::Void(()), value::Void(())) => {
                    Line(format!("pub const {}: () = ();", styled_name))
                }
                (type_::Bool(()), value::Bool(b)) => {
                    Line(format!("pub const {}: bool = {};", styled_name, b))
                }
                (type_::Int8(()), value::Int8(i)) => {
                    Line(format!("pub const {}: i8 = {};", styled_name, i))
                }
                (type_::Int16(()), value::Int16(i)) => {
                    Line(format!("pub const {}: i16 = {};", styled_name, i))
                }
                (type_::Int32(()), value::Int32(i)) => {
                    Line(format!("pub const {}: i32 = {};", styled_name, i))
                }
                (type_::Int64(()), value::Int64(i)) => {
                    Line(format!("pub const {}: i64 = {};", styled_name, i))
                }
                (type_::Uint8(()), value::Uint8(i)) => {
                    Line(format!("pub const {}: u8 = {};", styled_name, i))
                }
                (type_::Uint16(()), value::Uint16(i)) => {
                    Line(format!("pub const {}: u16 = {};", styled_name, i))
                }
                (type_::Uint32(()), value::Uint32(i)) => {
                    Line(format!("pub const {}: u32 = {};", styled_name, i))
                }
                (type_::Uint64(()), value::Uint64(i)) => {
                    Line(format!("pub const {}: u64 = {};", styled_name, i))
                }

                (type_::Float32(()), value::Float32(f)) => {
                    Line(format!("pub const {}: f32 = {:e}f32;", styled_name, f))
                }

                (type_::Float64(()), value::Float64(f)) => {
                    Line(format!("pub const {}: f64 = {:e}f64;", styled_name, f))
                }

                (type_::Enum(e), value::Enum(v)) => {
                    if let Some(node) = gen.node_map.get(&e.get_type_id()) {
                        match try!(node.which()) {
                            node::Enum(e) => {
                                let enumerants = try!(e.get_enumerants());
                                if (v as u32) < enumerants.len() {
                                    let variant = capitalize_first_letter(try!(
                                        enumerants.get(v as u32).get_name()
                                    ));
                                    let type_string = try!(typ.type_string(gen, Leaf::Owned));
                                    Line(format!(
                                        "pub const {}: {} = {}::{};",
                                        styled_name, &type_string, &type_string, variant
                                    ))
                                } else {
                                    return Err(Error::failed(format!(
                                        "enumerant out of range: {}",
                                        v
                                    )));
                                }
                            }
                            _ => {
                                return Err(Error::failed(format!(
                                    "bad enum type ID: {}",
                                    e.get_type_id()
                                )));
                            }
                        }
                    } else {
                        return Err(Error::failed(format!(
                            "bad enum type ID: {}",
                            e.get_type_id()
                        )));
                    }
                }

                (type_::Text(()), value::Text(t)) => Line(format!(
                    "pub const {}: &'static str = {:?};",
                    styled_name,
                    try!(t)
                )),
                (type_::Data(()), value::Data(d)) => Line(format!(
                    "pub const {}: &'static [u8] = &{:?};",
                    styled_name,
                    try!(d)
                )),

                (type_::List(_), value::List(v)) => {
                    try!(generate_pointer_constant(gen, &styled_name, typ, v))
                }
                (type_::Struct(_), value::Struct(v)) => {
                    try!(generate_pointer_constant(gen, &styled_name, typ, v))
                }

                (type_::Interface(_t), value::Interface(())) => {
                    return Err(Error::unimplemented(format!("interface constants")));
                }
                (type_::AnyPointer(_), value::AnyPointer(_pr)) => {
                    return Err(Error::unimplemented(format!("anypointer constants")));
                }

                _ => {
                    return Err(Error::failed(format!("type does not match value")));
                }
            };

            output.push(formatted_text);
        }

        node::Annotation(annotation_reader) => {
            println!("  annotation node:");
            if annotation_reader.get_targets_file() {
                println!("  targets file");
            }
            if annotation_reader.get_targets_const() {
                println!("  targets const");
            }
            // ...
            if annotation_reader.get_targets_annotation() {
                println!("  targets annotation");
            }
        }
    }

    Ok(Branch(output))
}

/// Generates Rust code according to a `schema_capnp::code_generator_request` read from `inp`.
pub fn main<T>(mut inp: T, out_dir: &::std::path::Path) -> ::capnp::Result<()>
where
    T: ::std::io::Read,
{
    use capnp::serialize;
    use std::io::Write;

    let message = try!(serialize::read_message(
        &mut inp,
        capnp::message::ReaderOptions::new()
    ));

    let gen = try!(GeneratorContext::new(&message));

    for requested_file in try!(gen.request.get_requested_files()).iter() {
        let id = requested_file.get_id();
        let mut filepath = out_dir.to_path_buf();
        let requested = ::std::path::PathBuf::from(try!(requested_file.get_filename()));
        filepath.push(requested);
        if let Some(parent) = filepath.parent() {
            try!(::std::fs::create_dir_all(parent));
        }

        let root_name = try!(path_to_stem_string(&filepath)).replace("-", "_");
        filepath.set_file_name(&format!("{}_capnp.rs", root_name));

        let lines = Branch(vec![
            Line(
                "// Generated by the capnpc-rust plugin to the Cap'n Proto schema compiler."
                    .to_string(),
            ),
            Line("// DO NOT EDIT.".to_string()),
            Line(format!(
                "// source: {}",
                try!(requested_file.get_filename())
            )),
            BlankLine,
            try!(generate_node(&gen, id, &root_name, None)),
        ]);

        let text = stringify(&lines);

        // It would be simpler to use try! instead of a pattern match, but then the error message
        // would not include `filepath`.
        match ::std::fs::File::create(&filepath) {
            Ok(ref mut writer) => {
                try!(writer.write_all(text.as_bytes()));
            }
            Err(e) => {
                let _ = writeln!(
                    &mut ::std::io::stderr(),
                    "could not open file {:?} for writing: {}",
                    filepath,
                    e
                );
                return Err(e.into());
            }
        }
    }
    Ok(())
}
