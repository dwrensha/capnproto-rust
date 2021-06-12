use capnp_conv::{capnp_conv, CapnpConvError, FromCapnpBytes, ReadCapnp, ToCapnpBytes, WriteCapnp};

#[allow(unused)]
mod test_capnp {
    include!(concat!(env!("OUT_DIR"), "/capnp/test_capnp.rs"));
}

#[capnp_conv(test_capnp::test_struct_inner)]
#[derive(Debug, Clone, PartialEq)]
struct TestStructInner {
    inner_u8: u8,
}

#[capnp_conv(test_capnp::test_struct::inline_union)]
#[derive(Debug, Clone, PartialEq)]
enum InlineUnion {
    FirstVariant(u64),
    SecondVariant(TestStructInner),
    ThirdVariant,
}

#[capnp_conv(test_capnp::test_union)]
#[derive(Debug, Clone, PartialEq)]
enum TestUnion {
    VariantOne(u64),
    VariantTwo(TestStructInner),
    VariantThree,
    VariantFour(String),
}

#[capnp_conv(test_capnp::list_union::inline_inner_union)]
#[derive(Debug, Clone, PartialEq)]
enum InlineInnerUnion {
    Ab(u32),
    Cd(u64),
}

#[capnp_conv(test_capnp::list_union)]
#[derive(Debug, Clone, PartialEq)]
enum ListUnion {
    Empty,
    WithList(Vec<TestStructInner>),
    WithData(Vec<u8>),
    TestUnion(TestUnion),
    InlineInnerUnion(InlineInnerUnion),
}

#[capnp_conv(test_capnp::test_struct)]
#[derive(Debug, Clone, PartialEq)]
struct TestStruct {
    my_bool: bool,
    my_int8: i8,
    my_int16: i16,
    my_int32: i32,
    my_int64: i64,
    my_uint8: u8,
    my_uint16: u16,
    my_uint32: u32,
    my_uint64: u64,
    my_text: String,
    my_data: Vec<u8>,
    struct_inner: TestStructInner,
    my_primitive_list: Vec<u16>,
    my_list: Vec<TestStructInner>,
    inline_union: InlineUnion,
    external_union: TestUnion,
    list_union: ListUnion,
}

#[test]
fn capnp_serialize_basic_struct() {
    let test_struct = TestStruct {
        my_bool: true,
        my_int8: -1i8,
        my_int16: 1i16,
        my_int32: -1i32,
        my_int64: 1i64,
        my_uint8: 1u8,
        my_uint16: 2u16,
        my_uint32: 3u32,
        my_uint64: 4u64,
        my_text: "my_text".to_owned(),
        my_data: vec![1, 2, 3, 4, 5u8],
        struct_inner: TestStructInner { inner_u8: 1u8 },
        my_primitive_list: vec![10, 11, 12, 13, 14u16],
        my_list: vec![
            TestStructInner { inner_u8: 2u8 },
            TestStructInner { inner_u8: 3u8 },
            TestStructInner { inner_u8: 4u8 },
        ],
        inline_union: InlineUnion::SecondVariant(TestStructInner { inner_u8: 5u8 }),
        external_union: TestUnion::VariantOne(6u64),
        list_union: ListUnion::WithList(vec![
            TestStructInner { inner_u8: 10u8 },
            TestStructInner { inner_u8: 11u8 },
        ]),
    };

    let data = test_struct.to_capnp_bytes();
    let test_struct2 = TestStruct::from_capnp_bytes(&data).unwrap();

    assert_eq!(test_struct, test_struct2);
}

#[capnp_conv(test_capnp::float_struct)]
#[derive(Debug, Clone)]
struct FloatStruct {
    my_float32: f32,
    my_float64: f64,
}

/// We test floats separately, because in Rust floats to not implement PartialEq
#[test]
fn capnp_serialize_floats() {
    let float_struct = FloatStruct {
        my_float32: -0.5f32,
        my_float64: 0.5f64,
    };

    let data = float_struct.to_capnp_bytes();
    let float_struct2 = FloatStruct::from_capnp_bytes(&data).unwrap();

    // Sloppily check that the floats are close enough (We can't compare them directly, as they
    // don't implement PartialEq)
    assert_eq!(
        (float_struct.my_float32 * 10000.0).trunc(),
        (float_struct2.my_float32 * 10000.0).trunc()
    );

    assert_eq!(
        (float_struct.my_float64 * 10000.0).trunc(),
        (float_struct2.my_float64 * 10000.0).trunc()
    );
}

#[capnp_conv(test_capnp::generic_struct)]
#[derive(Debug, Clone, PartialEq)]
struct GenericStruct<A = u32, B = u64, D = u8, E = TestStructInner> {
    a: A,
    pub b: B,
    c: u8,
    pub d: Vec<D>,
    e: Vec<E>,
    f: E,
}

#[test]
fn capnp_serialize_generic_struct() {
    let generic_struct = GenericStruct {
        a: 1u32,
        b: 2u64,
        c: 3u8,
        d: vec![1, 2, 3, 4u8],
        e: vec![
            TestStructInner { inner_u8: 2u8 },
            TestStructInner { inner_u8: 3u8 },
            TestStructInner { inner_u8: 4u8 },
        ],
        f: TestStructInner { inner_u8: 5u8 },
    };

    let data = generic_struct.to_capnp_bytes();
    let generic_struct2 = GenericStruct::from_capnp_bytes(&data).unwrap();

    assert_eq!(generic_struct, generic_struct2);
}

#[capnp_conv(test_capnp::generic_enum)]
#[derive(Debug, Clone, PartialEq)]
enum GenericEnum<A = u32, B = TestStructInner, V = Vec<u8>> {
    VarA(A),
    VarB(B),
    VarC(u64),
    VarD(V),
}

#[test]
fn capnp_serialize_generic_enum() {
    for generic_enum in &[
        GenericEnum::VarA(1u32),
        GenericEnum::VarB(TestStructInner { inner_u8: 2u8 }),
        GenericEnum::VarC(3u64),
        GenericEnum::VarD(vec![1, 2, 3, 4u8]),
    ] {
        let data = generic_enum.to_capnp_bytes();
        let generic_enum2 = GenericEnum::from_capnp_bytes(&data).unwrap();
        assert_eq!(generic_enum.clone(), generic_enum2);
    }
}

#[capnp_conv(test_capnp::inner_generic)]
#[derive(Debug, Clone, PartialEq)]
struct InnerGeneric<A = u32> {
    a: A,
}

#[capnp_conv(test_capnp::list_generic)]
#[derive(Debug, Clone, PartialEq)]
struct ListGeneric<A = u32> {
    list: Vec<InnerGeneric<A>>,
}

#[test]
fn capnp_serialize_generic_list() {
    let list_generic = ListGeneric {
        list: vec![
            InnerGeneric { a: 1u32 },
            InnerGeneric { a: 2u32 },
            InnerGeneric { a: 3u32 },
        ],
    };

    let data = list_generic.to_capnp_bytes();
    let list_generic2 = ListGeneric::from_capnp_bytes(&data).unwrap();

    assert_eq!(list_generic, list_generic2);
}
