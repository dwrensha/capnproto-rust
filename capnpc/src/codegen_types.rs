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

use capnp::Error;
use codegen;
use codegen::GeneratorContext;
use schema_capnp::{brand, node, type_};
use std::collections::hash_map::HashMap;

#[derive(Copy, Clone, PartialEq)]
pub enum Leaf {
    Reader(&'static str),
    Builder(&'static str),
    Owned,
    Client,
    Server,
    ServerDispatch,
    Pipeline,
}

impl ::std::fmt::Display for Leaf {
    fn fmt(&self, fmt: &mut ::std::fmt::Formatter) -> Result<(), ::std::fmt::Error> {
        let display_string = match self {
            &Leaf::Reader(lt) => format!("Reader<{}>", lt),
            &Leaf::Builder(lt) => format!("Builder<{}>", lt),
            &Leaf::Owned => "Owned".to_string(),
            &Leaf::Client => "Client".to_string(),
            &Leaf::Server => "Server".to_string(),
            &Leaf::ServerDispatch => "ServerDispatch".to_string(),
            &Leaf::Pipeline => "Pipeline".to_string(),
        };
        ::std::fmt::Display::fmt(&display_string, fmt)
    }
}

impl Leaf {
    fn bare_name(&self) -> &'static str {
        match self {
            &Leaf::Reader(_) => "Reader",
            &Leaf::Builder(_) => "Builder",
            &Leaf::Owned => "Owned",
            &Leaf::Client => "Client",
            &Leaf::Server => "Server",
            &Leaf::ServerDispatch => "ServerDispatch",
            &Leaf::Pipeline => "Pipeline",
        }
    }

    fn _have_lifetime(&self) -> bool {
        match self {
            &Leaf::Reader(_) | &Leaf::Builder(_) => true,
            &Leaf::Owned
            | &Leaf::Client
            | &Leaf::Server
            | &Leaf::ServerDispatch
            | &Leaf::Pipeline => false,
        }
    }
}

pub struct TypeParameterTexts {
    pub expanded_list: Vec<String>,
    pub params: String,
    pub where_clause: String,
    pub where_clause_with_send: String,
    pub pipeline_where_clause: String,
    pub phantom_data: String,
}

// this is a collection of helpers acting on a "Node" (most of them are Type definitions)
pub trait RustNodeInfo {
    fn parameters_texts(
        &self,
        gen: &::codegen::GeneratorContext,
        parent_node_id: Option<u64>,
    ) -> TypeParameterTexts;
}

// this is a collection of helpers acting on a "Type" (someplace where a Type is used, not defined)
pub trait RustTypeInfo {
    fn is_prim(&self) -> Result<bool, Error>;
    fn is_parameter(&self) -> Result<bool, Error>;
    fn is_branded(&self) -> Result<bool, Error>;
    fn type_string(&self, gen: &codegen::GeneratorContext, module: Leaf) -> Result<String, Error>;
}

impl<'a> RustNodeInfo for node::Reader<'a> {
    fn parameters_texts(
        &self,
        gen: &::codegen::GeneratorContext,
        parent_node_id: Option<u64>,
    ) -> TypeParameterTexts {
        if self.get_is_generic() {
            let params = get_type_parameters(&gen, self.get_id(), parent_node_id);
            let type_parameters = params
                .iter()
                .map(|param| format!("{}", param))
                .collect::<Vec<String>>()
                .join(",");
            let where_clause = "where ".to_string() + &*(params
                .iter()
                .map(|param| format!("{}: for<'c> ::capnp::traits::Owned<'c>", param))
                .collect::<Vec<String>>()
                .join(", ")
                + " ");
            let where_clause_with_send = "where ".to_string() + &*(params
                .iter()
                .map(|param| format!("{}:'static", param))
                .collect::<Vec<String>>()
                .join(", ")
                + " ");
            let pipeline_where_clause = "where ".to_string()
                + &*(params
                    .iter()
                    .map(|param| {
                        format!("{}: ::capnp::traits::Pipelined, <{} as ::capnp::traits::Pipelined>::Pipeline: ::capnp::capability::FromTypelessPipeline", param, param)
                    })
                    .collect::<Vec<String>>()
                    .join(", ") + " ");
            let phantom_data = "_phantom: ::std::marker::PhantomData,".to_string();

            TypeParameterTexts {
                expanded_list: params,
                params: type_parameters,
                where_clause: where_clause,
                where_clause_with_send: where_clause_with_send,
                pipeline_where_clause: pipeline_where_clause,
                phantom_data: phantom_data,
            }
        } else {
            TypeParameterTexts {
                expanded_list: vec![],
                params: "".to_string(),
                where_clause: "".to_string(),
                where_clause_with_send: "".to_string(),
                pipeline_where_clause: "".to_string(),
                phantom_data: "".to_string(),
            }
        }
    }
}

impl<'a> RustTypeInfo for type_::Reader<'a> {
    fn type_string(
        &self,
        gen: &codegen::GeneratorContext,
        module: Leaf,
    ) -> Result<String, ::capnp::Error> {
        let local_lifetime = match module {
            Leaf::Reader(lt) => lt,
            Leaf::Builder(lt) => lt,
            _ => "",
        };

        let lifetime_comma = if local_lifetime == "" {
            "".to_string()
        } else {
            format!("{},", local_lifetime)
        };

        match try!(self.which()) {
            type_::Void(()) => Ok("()".to_string()),
            type_::Bool(()) => Ok("bool".to_string()),
            type_::Int8(()) => Ok("i8".to_string()),
            type_::Int16(()) => Ok("i16".to_string()),
            type_::Int32(()) => Ok("i32".to_string()),
            type_::Int64(()) => Ok("i64".to_string()),
            type_::Uint8(()) => Ok("u8".to_string()),
            type_::Uint16(()) => Ok("u16".to_string()),
            type_::Uint32(()) => Ok("u32".to_string()),
            type_::Uint64(()) => Ok("u64".to_string()),
            type_::Float32(()) => Ok("f32".to_string()),
            type_::Float64(()) => Ok("f64".to_string()),
            type_::Text(()) => Ok(format!("::capnp::text::{}", module)),
            type_::Data(()) => Ok(format!("::capnp::data::{}", module)),
            type_::Struct(st) => do_branding(
                gen,
                st.get_type_id(),
                try!(st.get_brand()),
                module,
                gen.scope_map[&st.get_type_id()].join("::"),
                None,
            ),
            type_::Interface(interface) => do_branding(
                gen,
                interface.get_type_id(),
                try!(interface.get_brand()),
                module,
                gen.scope_map[&interface.get_type_id()].join("::"),
                None,
            ),
            type_::List(ot1) => {
                let element_type = try!(ot1.get_element_type());
                match try!(element_type.which()) {
                    type_::Struct(_) => {
                        let inner = try!(element_type.type_string(gen, Leaf::Owned));
                        Ok(format!(
                            "::capnp::struct_list::{}<{}{}>",
                            module.bare_name(),
                            lifetime_comma,
                            inner
                        ))
                    }
                    type_::Enum(_) => {
                        let inner = try!(element_type.type_string(gen, Leaf::Owned));
                        Ok(format!(
                            "::capnp::enum_list::{}<{}{}>",
                            module.bare_name(),
                            lifetime_comma,
                            inner
                        ))
                    }
                    type_::List(_) => {
                        let inner = try!(element_type.type_string(gen, Leaf::Owned));
                        Ok(format!(
                            "::capnp::list_list::{}<{}{}>",
                            module.bare_name(),
                            lifetime_comma,
                            inner
                        ))
                    }
                    type_::Text(()) => Ok(format!("::capnp::text_list::{}", module)),
                    type_::Data(()) => Ok(format!("::capnp::data_list::{}", module)),
                    type_::Interface(_) => {
                        let inner = try!(element_type.type_string(gen, Leaf::Client));
                        Ok(format!(
                            "::capnp::capability_list::{}<{}{}>",
                            module.bare_name(),
                            lifetime_comma,
                            inner
                        ))
                    }
                    type_::AnyPointer(_) => {
                        Err(Error::failed("List(AnyPointer) is unsupported".to_string()))
                    }
                    _ => {
                        let inner = try!(element_type.type_string(gen, Leaf::Owned));
                        Ok(format!(
                            "::capnp::primitive_list::{}<{}{}>",
                            module.bare_name(),
                            lifetime_comma,
                            inner
                        ))
                    }
                }
            }
            type_::Enum(en) => {
                let scope = &gen.scope_map[&en.get_type_id()];
                Ok(scope.join("::").to_string())
            }
            type_::AnyPointer(pointer) => match try!(pointer.which()) {
                type_::any_pointer::Parameter(def) => {
                    let the_struct = &gen.node_map[&def.get_scope_id()];
                    let parameters = try!(the_struct.get_parameters());
                    let parameter = parameters.get(def.get_parameter_index() as u32);
                    let parameter_name = try!(parameter.get_name());
                    match module {
                        Leaf::Owned => Ok(parameter_name.to_string()),
                        Leaf::Reader(lifetime) => Ok(format!(
                            "<{} as ::capnp::traits::Owned<{}>>::Reader",
                            parameter_name, lifetime
                        )),
                        Leaf::Builder(lifetime) => Ok(format!(
                            "<{} as ::capnp::traits::Owned<{}>>::Builder",
                            parameter_name, lifetime
                        )),
                        Leaf::Pipeline => Ok(format!(
                            "<{} as ::capnp::traits::Pipelined>::Pipeline",
                            parameter_name
                        )),
                        _ => Err(Error::unimplemented(
                            "unimplemented any_pointer leaf".to_string(),
                        )),
                    }
                }
                _ => match module {
                    Leaf::Reader(lifetime) => {
                        Ok(format!("::capnp::any_pointer::Reader<{}>", lifetime))
                    }
                    Leaf::Builder(lifetime) => {
                        Ok(format!("::capnp::any_pointer::Builder<{}>", lifetime))
                    }
                    _ => Ok(format!("::capnp::any_pointer::{}", module)),
                },
            },
        }
    }

    fn is_parameter(&self) -> Result<bool, Error> {
        match try!(self.which()) {
            type_::AnyPointer(pointer) => match try!(pointer.which()) {
                type_::any_pointer::Parameter(_) => Ok(true),
                _ => Ok(false),
            },
            _ => Ok(false),
        }
    }

    fn is_branded(&self) -> Result<bool, Error> {
        match try!(self.which()) {
            type_::Struct(st) => {
                let brand = try!(st.get_brand());
                let scopes = try!(brand.get_scopes());
                Ok(scopes.len() > 0)
            }
            _ => Ok(false),
        }
    }

    #[inline(always)]
    fn is_prim(&self) -> Result<bool, Error> {
        match try!(self.which()) {
            type_::Int8(())
            | type_::Int16(())
            | type_::Int32(())
            | type_::Int64(())
            | type_::Uint8(())
            | type_::Uint16(())
            | type_::Uint32(())
            | type_::Uint64(())
            | type_::Float32(())
            | type_::Float64(())
            | type_::Void(())
            | type_::Bool(()) => Ok(true),
            _ => Ok(false),
        }
    }
}

///
///
pub fn do_branding(
    gen: &GeneratorContext,
    node_id: u64,
    brand: brand::Reader,
    leaf: Leaf,
    the_mod: String,
    mut parent_scope_id: Option<u64>,
) -> Result<String, Error> {
    let scopes = try!(brand.get_scopes());
    let mut brand_scopes = HashMap::new();
    for scope in scopes.iter() {
        brand_scopes.insert(scope.get_scope_id(), scope);
    }
    let brand_scopes = brand_scopes; // freeze
    let mut current_node_id = node_id;
    let mut accumulator: Vec<Vec<String>> = Vec::new();
    loop {
        let current_node = match gen.node_map.get(&current_node_id) {
            None => break,
            Some(node) => node,
        };
        let params = try!(current_node.get_parameters());
        let mut arguments: Vec<String> = Vec::new();
        match brand_scopes.get(&current_node_id) {
            None => {
                for _ in params.iter() {
                    arguments.push("::capnp::any_pointer::Owned".to_string());
                }
            }
            Some(scope) => match try!(scope.which()) {
                brand::scope::Inherit(()) => {
                    for param in params.iter() {
                        arguments.push(try!(param.get_name()).to_string());
                    }
                }
                brand::scope::Bind(bindings_list_opt) => {
                    let bindings_list = try!(bindings_list_opt);
                    assert_eq!(bindings_list.len(), params.len());
                    for binding in bindings_list.iter() {
                        match try!(binding.which()) {
                            brand::binding::Unbound(()) => {
                                arguments.push("::capnp::any_pointer::Owned".to_string());
                            }
                            brand::binding::Type(t) => {
                                arguments.push(try!(try!(t).type_string(gen, Leaf::Owned)));
                            }
                        }
                    }
                }
            },
        }
        accumulator.push(arguments);
        current_node_id = current_node.get_scope_id();
        match (current_node_id, parent_scope_id) {
            (0, Some(id)) => current_node_id = id,
            _ => (),
        }
        parent_scope_id = None; // Only consider on the first time around.
    }

    // Now add a lifetime parameter if the leaf has one.
    match leaf {
        Leaf::Reader(lt) => accumulator.push(vec![lt.to_string()]),
        Leaf::Builder(lt) => accumulator.push(vec![lt.to_string()]),
        Leaf::ServerDispatch => accumulator.push(vec!["_T".to_string()]), // HACK
        _ => (),
    }

    accumulator.reverse();
    let accumulated = accumulator.concat();

    let arguments = if accumulated.len() > 0 {
        format!("<{}>", accumulated.join(","))
    } else {
        "".to_string()
    };

    Ok(format!(
        "{mod}::{leaf}{maybe_colons}{arguments}",
        mod = the_mod,
        leaf = leaf.bare_name().to_string(),
        maybe_colons = if leaf == Leaf::ServerDispatch { "::" } else { "" }, // HACK
        arguments = arguments))
}

pub fn get_type_parameters(
    gen: &GeneratorContext,
    node_id: u64,
    mut parent_scope_id: Option<u64>,
) -> Vec<String> {
    let mut current_node_id = node_id;
    let mut accumulator: Vec<Vec<String>> = Vec::new();
    loop {
        let current_node = match gen.node_map.get(&current_node_id) {
            None => break,
            Some(node) => node,
        };
        let mut params = Vec::new();
        for param in current_node.get_parameters().unwrap().iter() {
            params.push(param.get_name().unwrap().to_string());
        }

        accumulator.push(params);
        current_node_id = current_node.get_scope_id();
        match (current_node_id, parent_scope_id) {
            (0, Some(id)) => current_node_id = id,
            _ => (),
        }
        parent_scope_id = None; // Only consider on the first time around.
    }

    accumulator.reverse();
    accumulator.concat()
}
