// use syn::{parse_macro_input, Data, DeriveInput, Fields, Ident, Index};
use syn;

use std::collections::HashMap;

use proc_macro2::TokenStream;
use quote::quote;

/// A shim for converting usize to u32
pub fn usize_to_u32_shim() -> TokenStream {
    quote! {
        pub fn usize_to_u32(num: usize) -> Option<u32> {
            if num > 0xffffffff as usize {
                None
            } else {
                Some(num as u32)
            }
        }
    }
}

/// Is a primitive type?
pub fn is_primitive(path: &syn::Path) -> bool {
    path.is_ident("u8")
        || path.is_ident("u16")
        || path.is_ident("u32")
        || path.is_ident("u64")
        || path.is_ident("i8")
        || path.is_ident("i16")
        || path.is_ident("i32")
        || path.is_ident("i64")
        || path.is_ident("f32")
        || path.is_ident("f64")
        || path.is_ident("bool")
}

/// Check if the path represents a Vec<u8>
pub fn is_data(path: &syn::Path) -> bool {
    let last_segment = match path.segments.last().unwrap() {
        syn::punctuated::Pair::End(last_segment) => last_segment,
        _ => unreachable!(),
    };
    if &last_segment.ident.to_string() != "Vec" {
        return false;
    }
    let angle = match &last_segment.arguments {
        syn::PathArguments::AngleBracketed(angle) => {
            if angle.args.len() > 1 {
                unreachable!("Too many arguments for Vec!");
            }
            angle
        }
        _ => unreachable!("Vec with arguments that are not angle bracketed!"),
    };
    let last_arg = match angle.args.last().unwrap() {
        syn::punctuated::Pair::End(last_arg) => last_arg,
        _ => return false,
    };

    let arg_ty = match last_arg {
        syn::GenericArgument::Type(arg_ty) => arg_ty,
        _ => return false,
    };

    let arg_ty_path = match arg_ty {
        syn::Type::Path(arg_ty_path) => arg_ty_path,
        _ => return false,
    };

    if arg_ty_path.qself.is_some() {
        return false;
    }

    if !arg_ty_path.path.is_ident("u8") {
        return false;
    }

    true
}

/// Check if the path represents a Vec<SomeStruct>, where SomeStruct != u8
pub fn get_vec(path: &syn::Path) -> Option<syn::Path> {
    let last_segment = match path.segments.last().unwrap() {
        syn::punctuated::Pair::End(last_segment) => last_segment,
        _ => unreachable!(),
    };
    if &last_segment.ident.to_string() != "Vec" {
        return None;
    }
    let angle = match &last_segment.arguments {
        syn::PathArguments::AngleBracketed(angle) => {
            if angle.args.len() > 1 {
                unreachable!("Too many arguments for Vec!");
            }
            angle
        }
        _ => unreachable!("Vec with arguments that are not angle bracketed!"),
    };
    let last_arg = match angle.args.last().unwrap() {
        syn::punctuated::Pair::End(last_arg) => last_arg,
        _ => return None,
    };

    let arg_ty = match last_arg {
        syn::GenericArgument::Type(arg_ty) => arg_ty,
        _ => return None,
    };

    let arg_ty_path = match arg_ty {
        syn::Type::Path(arg_ty_path) => arg_ty_path,
        _ => return None,
    };

    if arg_ty_path.qself.is_some() {
        return None;
    }

    // Make sure that we don't deal with Vec<u8>:
    if arg_ty_path.path.is_ident("u8") {
        return None;
    }

    Some(arg_ty_path.path.clone())
}

pub fn gen_list_write_iter(path: &syn::Path) -> TokenStream {
    if is_primitive(path) || path.is_ident("String") || is_data(path) {
        // A primitive list:
        quote! {
            list_builder
                .reborrow()
                .set(usize_to_u32(index).unwrap(), item.clone().into());
        }
    } else {
        // Not a primitive list:
        quote! {
            let mut item_builder = list_builder
                .reborrow()
                .get(usize_to_u32(index).unwrap());

            item.clone().write_capnp(&mut item_builder);
        }
    }
    // TODO: It seems like we do not support List(List(...)) at the moment.
    // How to support it?
}

pub fn gen_list_read_iter(path: &syn::Path) -> TokenStream {
    if is_primitive(path) || path.is_ident("String") || is_data(path) {
        // A primitive list:
        quote! {
            res_vec.push(item_reader.into());
        }
    } else {
        // Not a primitive list:
        quote! {
            res_vec.push(<#path>::read_capnp(&item_reader)?);
        }
    }
    // TODO: It seems like we do not support List(List(...)) at the moment.
    // How to support it?
}

/// A shim allowing to merge cases where either
/// Result<T,Into<CapnoConvError>> or a T is returned.
pub fn capnp_result_shim() -> TokenStream {
    quote! {
        pub enum CapnpResult<T> {
            Ok(T),
            Err(CapnpConvError),
        }

        impl<T> CapnpResult<T> {
            pub fn into_result(self) -> Result<T, CapnpConvError> {
                match self {
                    CapnpResult::Ok(t) => Ok(t),
                    CapnpResult::Err(e) => Err(e),
                }
            }
        }

        impl<T> From<T> for CapnpResult<T> {
            fn from(input: T) -> Self {
                CapnpResult::Ok(input)
            }
        }

        impl<T, E> From<Result<T, E>> for CapnpResult<T>
        where
            E: Into<CapnpConvError>,
        {
            fn from(input: Result<T, E>) -> Self {
                match input {
                    Ok(t) => CapnpResult::Ok(t),
                    Err(e) => CapnpResult::Err(e.into()),
                }
            }
        }
    }
}

/// Obtain a map of default values from generics.
/// Example:
///
/// ```text
/// struct MyStruct<A = u32, B = u64> { ... }
/// ```
///
/// We expect to get a map, mapping A -> u32, B -> u64.
///
pub fn extract_defaults(generics: &syn::Generics) -> HashMap<syn::Ident, syn::Path> {
    let mut defaults = HashMap::new();
    for param in &generics.params {
        let type_param = match *param {
            syn::GenericParam::Type(ref type_param) => type_param,
            _ => continue,
        };

        if type_param.eq_token.is_none() {
            continue;
        };

        let default_type = match &type_param.default {
            Some(default_type) => default_type,
            None => continue,
        };

        let default_type_path = match default_type {
            syn::Type::Path(default_type_path) => default_type_path,
            _ => unimplemented!("Only paths default params are supported"),
        };

        if default_type_path.qself.is_some() {
            unimplemented!("qself is not implemented!");
        }

        defaults.insert(type_param.ident.clone(), default_type_path.path.clone());
    }
    defaults
}

/// For every generic along a path, assign a default value if possible
pub fn assign_defaults_path(path: &mut syn::Path, defaults: &HashMap<syn::Ident, syn::Path>) {
    // Deal with the case of a single Ident: `T`

    if path.segments.len() == 1 {
        let last_segment = match path.segments.last_mut().unwrap() {
            syn::punctuated::Pair::End(last_segment) => last_segment,
            _ => unreachable!(),
        };

        if let syn::PathArguments::None = last_segment.arguments {
            if let Some(default_path) = defaults.get(&last_segment.ident) {
                let _ = std::mem::replace(path, default_path.clone());
                return;
            }
        }
    }

    // Deal with the more general case of a Path with various arguments
    // that should be assigned their default value

    for segment in path.segments.iter_mut() {
        let args = match &mut segment.arguments {
            syn::PathArguments::None => continue,
            syn::PathArguments::AngleBracketed(angle_bracketed) => &mut angle_bracketed.args,
            _ => unimplemented!("Only angle bracketed arguments are supported!"),
        };

        for generic_arg in args.iter_mut() {
            let ty = match generic_arg {
                syn::GenericArgument::Type(ty) => ty,
                _ => unimplemented!(),
            };

            let type_path = match ty {
                syn::Type::Path(type_path) => type_path,
                _ => unimplemented!(),
            };

            if type_path.qself.is_some() {
                unimplemented!();
            }

            // Recursively replace default arguments:
            assign_defaults_path(&mut type_path.path, defaults);
        }
    }
}

/// Remove all of our `#[capnp_conv(with = ...)]` attributes
pub fn remove_with_attributes(input: &mut syn::DeriveInput) {
    match input.data {
        syn::Data::Struct(ref mut data) => match data.fields {
            syn::Fields::Named(ref mut fields_named) => {
                for field in fields_named.named.iter_mut() {
                    // Remove all the attributes that look like: `capnp_conv(...)`
                    field.attrs.retain(|attr| !attr.path.is_ident("capnp_conv"));
                }
            }
            syn::Fields::Unnamed(_) | syn::Fields::Unit => unimplemented!(),
        },
        syn::Data::Enum(ref mut data_enum) => {
            for variant in data_enum.variants.iter_mut() {
                // Remove all the attributes that look like: `capnp_conv(...)`
                variant
                    .attrs
                    .retain(|attr| !attr.path.is_ident("capnp_conv"));
            }
        }

        syn::Data::Union(_) => unimplemented!(),
    };
}

#[derive(Debug)]
pub struct CapnpWithAttribute {
    #[allow(dead_code)]
    pub paren_token: syn::token::Paren,
    pub with_ident: syn::Ident,
    pub eq_token: syn::Token![=],
    pub path: syn::Path,
}

impl syn::parse::Parse for CapnpWithAttribute {
    fn parse(input: syn::parse::ParseStream) -> syn::parse::Result<Self> {
        let content;
        let paren_token = syn::parenthesized!(content in input);
        Ok(Self {
            paren_token,
            with_ident: content.parse()?,
            eq_token: content.parse()?,
            path: content.parse()?,
        })
    }
}
