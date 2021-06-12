#![crate_type = "lib"]
#![recursion_limit = "128"]
#![deny(trivial_numeric_casts, warnings)]
#![allow(intra_doc_link_resolution_failure)]
#![allow(
    clippy::too_many_arguments,
    clippy::implicit_hasher,
    clippy::module_inception,
    clippy::new_without_default
)]
#![allow(unreachable_code)]

extern crate proc_macro;

mod derive_enum;
mod derive_struct;
mod util;

use quote::quote;
use syn::{parse_macro_input, Data, DeriveInput, Fields, Path};

use self::derive_enum::{gen_read_capnp_enum, gen_write_capnp_enum};
use self::derive_struct::{gen_read_capnp_named_struct, gen_write_capnp_named_struct};
use self::util::{assign_defaults_path, extract_defaults, remove_with_attributes};

/// Generate code for conversion between Rust and capnp structs.
#[proc_macro_attribute]
pub fn capnp_conv(
    args: proc_macro::TokenStream,
    input: proc_macro::TokenStream,
) -> proc_macro::TokenStream {
    // See: https://github.com/dtolnay/syn/issues/86
    // for information about arguments.

    // Name of capnp struct:
    let capnp_struct = parse_macro_input!(args as Path);
    let input = parse_macro_input!(input as DeriveInput);

    let defaults = extract_defaults(&input.generics);
    let assign_defaults = |path: &mut syn::Path| assign_defaults_path(path, &defaults);

    // Name of local struct:
    let rust_struct = &input.ident;

    let conversion = match input.data {
        Data::Struct(ref data) => match data.fields {
            Fields::Named(ref fields_named) => {
                // Example:
                // struct Point {
                //     x: u32,
                //     y: u32,
                // }

                let write_capnp = gen_write_capnp_named_struct(
                    fields_named,
                    rust_struct,
                    &capnp_struct,
                    &assign_defaults,
                );
                let read_capnp = gen_read_capnp_named_struct(
                    fields_named,
                    rust_struct,
                    &capnp_struct,
                    &assign_defaults,
                );

                quote! {
                    #[allow(clippy::all)]
                    #write_capnp
                    #[allow(clippy::all)]
                    #read_capnp
                }
            }
            Fields::Unnamed(_) | Fields::Unit => unimplemented!(),
        },
        Data::Enum(ref data_enum) => {
            // Example:
            // enum MyEnum {
            //     Type1(u32),
            //     Type2,
            //     Type3(MyStruct),
            // }
            let write_capnp =
                gen_write_capnp_enum(data_enum, rust_struct, &capnp_struct, &assign_defaults);
            let read_capnp =
                gen_read_capnp_enum(data_enum, rust_struct, &capnp_struct, &assign_defaults);

            quote! {
                #[allow(clippy::all)]
                #write_capnp
                #[allow(clippy::all)]
                #read_capnp
            }
        }
        Data::Union(_) => unimplemented!(),
    };

    // Remove all of our `#[capnp_conv(with = ... )]` attributes from the input:
    let mut input = input;
    remove_with_attributes(&mut input);

    let expanded = quote! {
        // Original structure
        #input
        // Generated mutual From conversion code:
        #conversion
    };

    proc_macro::TokenStream::from(expanded)
}
