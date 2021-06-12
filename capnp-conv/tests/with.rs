use byteorder::{BigEndian, ByteOrder, ReadBytesExt, WriteBytesExt};
use capnp_conv::{capnp_conv, CapnpConvError, FromCapnpBytes, ReadCapnp, ToCapnpBytes, WriteCapnp};

#[allow(unused)]
mod test_capnp {
    include!(concat!(env!("OUT_DIR"), "/capnp/test_capnp.rs"));
}

#[derive(derive_more::Constructor, Debug, Clone, Copy, PartialEq, Eq)]
pub struct Wrapper<T>(T);

impl<T> From<T> for Wrapper<T> {
    fn from(t: T) -> Self {
        Wrapper(t)
    }
}

impl Into<u128> for Wrapper<u128> {
    fn into(self) -> u128 {
        self.0
    }
}

impl<'a> WriteCapnp<'a> for Wrapper<u128> {
    type WriterType = crate::test_capnp::custom_u_int128::Builder<'a>;

    fn write_capnp(&self, writer: &mut Self::WriterType) {
        let mut inner = writer.reborrow().get_inner().unwrap();

        let mut data_bytes = Vec::new();

        data_bytes
            .write_u128::<BigEndian>(self.clone().into())
            .unwrap();
        let mut cursor = std::io::Cursor::new(AsRef::<[u8]>::as_ref(&data_bytes));

        inner.set_x0(cursor.read_u64::<BigEndian>().unwrap());
        inner.set_x1(cursor.read_u64::<BigEndian>().unwrap());
    }
}

impl<'a> ReadCapnp<'a> for Wrapper<u128> {
    type ReaderType = crate::test_capnp::custom_u_int128::Reader<'a>;

    fn read_capnp(reader: &Self::ReaderType) -> Result<Self, CapnpConvError> {
        let inner = reader.get_inner()?;
        let mut vec = Vec::new();
        vec.write_u64::<BigEndian>(inner.get_x0())?;
        vec.write_u64::<BigEndian>(inner.get_x1())?;
        Ok(Wrapper::new(BigEndian::read_u128(&vec[..])))
    }
}

#[capnp_conv(test_capnp::test_with_struct)]
#[derive(Debug, Clone, PartialEq)]
struct TestWithStruct {
    #[capnp_conv(with = Wrapper<u128>)]
    a: u128,
    b: u64,
}

#[test]
fn capnp_serialize_with_struct() {
    let test_with_struct = TestWithStruct { a: 1u128, b: 2u64 };

    let data = test_with_struct.to_capnp_bytes();
    let test_with_struct2 = TestWithStruct::from_capnp_bytes(&data).unwrap();

    assert_eq!(test_with_struct, test_with_struct2);
}

#[capnp_conv(test_capnp::test_with_enum)]
#[derive(Debug, Clone, PartialEq)]
enum TestWithEnum {
    #[capnp_conv(with = Wrapper<u128>)]
    VarA(u128),
    VarB(u64),
    VarC,
}

#[test]
fn capnp_serialize_with_enum() {
    let test_with_enum = TestWithEnum::VarA(1u128);

    let data = test_with_enum.to_capnp_bytes();
    let test_with_enum2 = TestWithEnum::from_capnp_bytes(&data).unwrap();

    assert_eq!(test_with_enum, test_with_enum2);
}
