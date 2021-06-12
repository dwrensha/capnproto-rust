#![crate_type = "lib"]
#![deny(trivial_numeric_casts, warnings)]
#![allow(intra_doc_link_resolution_failure)]
#![allow(
    clippy::too_many_arguments,
    clippy::implicit_hasher,
    clippy::module_inception,
    clippy::new_without_default
)]

use std::io;

use capnp;

use derive_more::From;

pub use capnp_conv_derive::capnp_conv;

#[derive(Debug, From)]
pub enum CapnpConvError {
    CapnpError(capnp::Error),
    NotInSchema(capnp::NotInSchema),
    IoError(io::Error),
}

/// Convert Rust struct to Capnp.
pub trait WriteCapnp<'a> {
    /// The corresponding Capnp writer type.
    // type WriterType: capnp::traits::FromPointerBuilder<'a>;
    type WriterType: capnp::traits::FromPointerBuilder<'a>;

    /// Converts a Rust struct to corresponding Capnp struct. This should not fail.
    fn write_capnp(&self, writer: &mut Self::WriterType);
}

/// Convert Capnp struct to Rust.
pub trait ReadCapnp<'a>: Sized {
    /// The corresponding Capnp reader type.
    type ReaderType: capnp::traits::FromPointerReader<'a>;

    /// Converts a Capnp struct to corresponding Rust struct.     
    fn read_capnp(reader: &Self::ReaderType) -> Result<Self, CapnpConvError>;
}

pub trait ToCapnpBytes {
    /// Serialize a Rust struct into bytes using Capnp
    fn to_capnp_bytes(&self) -> Vec<u8>;
}

pub trait FromCapnpBytes: Sized {
    /// Deserialize a Rust struct from bytes using Capnp
    fn from_capnp_bytes(bytes: &[u8]) -> Result<Self, CapnpConvError>;
}

impl<T> ToCapnpBytes for T
where
    T: for<'a> WriteCapnp<'a>,
{
    fn to_capnp_bytes(&self) -> Vec<u8> {
        let mut builder = capnp::message::Builder::new_default();

        // A trick to avoid borrow checker issues:
        {
            let mut struct_builder = builder.init_root::<T::WriterType>();
            self.write_capnp(&mut struct_builder);
        }

        let mut data = Vec::new();
        // Should never really fail:
        capnp::serialize_packed::write_message(&mut data, &builder).unwrap();
        data
    }
}

impl<T> FromCapnpBytes for T
where
    T: for<'a> ReadCapnp<'a>,
{
    fn from_capnp_bytes(bytes: &[u8]) -> Result<Self, CapnpConvError> {
        let mut cursor = io::Cursor::new(&bytes);
        let reader = capnp::serialize_packed::read_message(
            &mut cursor,
            capnp::message::ReaderOptions::new(),
        )?;
        let struct_reader = reader.get_root::<T::ReaderType>()?;
        Ok(Self::read_capnp(&struct_reader)?)
    }
}
