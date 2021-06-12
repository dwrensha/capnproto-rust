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

// Enable this lint to catch violations in the generated code.
#![warn(elided_lifetimes_in_paths)]

extern crate core;

extern crate capnp;

pub mod test_capnp {
    include!(concat!(env!("OUT_DIR"), "/test_capnp.rs"));
}

pub mod foo {
    pub mod bar {
        pub mod in_submodule_capnp {
            include!(concat!(env!("OUT_DIR"), "/in_submodule_capnp.rs"));
        }
    }
}

pub mod baz {
    pub mod in_other_submodule_capnp {
        include!(concat!(env!("OUT_DIR"), "/in_other_submodule_capnp.rs"));
    }
}

pub mod test_default_parent_module {
    pub mod test_default_parent_module_inner {
        // In build.rs we specify this is the default parent module.
        pub mod test_default_parent_module_capnp {
            include!(concat!(env!("OUT_DIR"), "/test_default_parent_module_capnp.rs"));
        }
    }

    // Put this in somewhere other than the default parent module, to test whether the `parentModule`
    // annotation successfully overrides the default.
    pub mod test_default_parent_module_override_capnp {
        include!(concat!(env!("OUT_DIR"), "/test_default_parent_module_override_capnp.rs"));
    }

}

pub mod test_in_dir_capnp {
    include!(concat!(env!("OUT_DIR"), "/schema/test_in_dir_capnp.rs"));
}

pub mod test_in_src_prefix_dir_capnp {
  // The src_prefix gets stripped away, so the generated code ends up directly in OUT_DIR.
  include!(concat!(env!("OUT_DIR"), "/test_in_src_prefix_dir_capnp.rs"));
}

#[cfg(test)]
mod test_util;

#[cfg(test)]
mod tests {
    use capnp::message;
    use capnp::message::{ReaderOptions};

    #[test]
    fn test_prim_list () {
        use test_capnp::test_prim_list;

        // Make the first segment small to force allocation of a second segment.
        let mut message = message::Builder::new(message::HeapAllocator::new().first_segment_words(50));

        let mut test_prim_list = message.init_root::<test_prim_list::Builder<'_>>();
        assert_eq!(test_prim_list.has_bool_list(), false);
        assert_eq!(test_prim_list.has_void_list(), false);
        {
            {
                let mut uint8_list = test_prim_list.reborrow().init_uint8_list(100);
                for i in 0..uint8_list.len() {
                    uint8_list.set(i, i as u8);
                }
            }

            {
                let mut uint64_list = test_prim_list.reborrow().init_uint64_list(20);
                for i in 0..uint64_list.len() {
                    uint64_list.set(i, i as u64);
                }
            }

            {
                let mut bool_list = test_prim_list.reborrow().init_bool_list(65);

                bool_list.set(0, true);
                bool_list.set(1, true);
                bool_list.set(2, true);
                bool_list.set(3, true);
                bool_list.set(5, true);
                bool_list.set(8, true);
                bool_list.set(13, true);
                bool_list.set(64, true);

                assert!(bool_list.get(0));
                assert!(!bool_list.get(4));
                assert!(!bool_list.get(63));
                assert!(bool_list.get(64));
            }

            let mut void_list = test_prim_list.reborrow().init_void_list(1025);
            void_list.set(257, ());
        }
        assert_eq!(test_prim_list.has_bool_list(), true);
        assert_eq!(test_prim_list.has_void_list(), true);

        let test_prim_list_reader = test_prim_list.into_reader();
        let uint8_list = test_prim_list_reader.get_uint8_list().unwrap();
        for i in 0..uint8_list.len() {
            assert_eq!(uint8_list.get(i), i as u8);
        }
        let uint64_list = test_prim_list_reader.get_uint64_list().unwrap();
        for i in 0..uint64_list.len() {
            assert_eq!(uint64_list.get(i), i as u64);
        }

        assert_eq!(test_prim_list_reader.has_bool_list(), true);
        let bool_list = test_prim_list_reader.get_bool_list().unwrap();
        assert!(bool_list.get(0));
        assert!(bool_list.get(1));
        assert!(bool_list.get(2));
        assert!(bool_list.get(3));
        assert!(!bool_list.get(4));
        assert!(bool_list.get(5));
        assert!(!bool_list.get(6));
        assert!(!bool_list.get(7));
        assert!(bool_list.get(8));
        assert!(!bool_list.get(9));
        assert!(!bool_list.get(10));
        assert!(!bool_list.get(11));
        assert!(!bool_list.get(12));
        assert!(bool_list.get(13));
        assert!(!bool_list.get(63));
        assert!(bool_list.get(64));

        assert_eq!(test_prim_list_reader.get_void_list().unwrap().len(), 1025);
    }

    #[test]
    fn test_struct_list () {
        use test_capnp::test_struct_list;

        let mut message = message::Builder::new(message::HeapAllocator::new());

        let mut test_struct_list = message.init_root::<test_struct_list::Builder<'_>>();

        test_struct_list.reborrow().init_struct_list(4);
        {
            let struct_list = test_struct_list.reborrow().get_struct_list().unwrap();
            struct_list.get(0).init_uint8_list(1).set(0, 5u8);
        }

        {
            let reader = test_struct_list.into_reader();
            assert_eq!(reader.get_struct_list().unwrap().get(0).get_uint8_list().unwrap().get(0), 5u8);
        }
    }

    #[test]
    fn test_blob () {
        use test_capnp::test_blob;

        let mut message = message::Builder::new(message::HeapAllocator::new());
        let mut test_blob = message.init_root::<test_blob::Builder<'_>>();

        assert_eq!(test_blob.has_text_field(), false);
        test_blob.set_text_field("abcdefghi");
        assert_eq!(test_blob.has_text_field(), true);

        assert_eq!(test_blob.has_data_field(), false);
        test_blob.set_data_field(&[0u8, 1u8, 2u8, 3u8, 4u8]);
        assert_eq!(test_blob.has_data_field(), true);

        {
            let test_blob_reader = test_blob.reborrow_as_reader();

            assert_eq!(test_blob_reader.has_text_field(), true);
            assert_eq!(test_blob_reader.has_data_field(), true);

            assert_eq!(test_blob_reader.get_text_field().unwrap(), "abcdefghi");
            assert!(test_blob_reader.get_data_field().unwrap() == [0u8, 1u8, 2u8, 3u8, 4u8]);
        }

        {
            let mut text = test_blob.reborrow().init_text_field(10);
            assert_eq!(&*text,"\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00");
            text.push_str("aabbccddee");
        }

        test_blob.reborrow().init_data_field(7);
        assert!(test_blob.reborrow().into_reader().get_data_field().unwrap() ==
                [0u8,0u8,0u8,0u8,0u8,0u8,0u8]);
        {
            let data_builder = test_blob.reborrow().get_data_field().unwrap();
            for c in data_builder.iter_mut() {
                *c = 5;
            }
            data_builder[0] = 4u8;
        }

        assert_eq!(test_blob.reborrow().into_reader().get_text_field().unwrap(), "aabbccddee");
        assert!(test_blob.reborrow().into_reader().get_data_field().unwrap() == [4u8,5u8,5u8,5u8,5u8,5u8,5u8]);

        {
            test_blob.reborrow().get_data_field().unwrap()[2] = 10;
        }
        assert!(test_blob.into_reader().get_data_field().unwrap() == [4u8,5u8,10u8,5u8,5u8,5u8,5u8]);
    }


    #[test]
    fn test_big_struct() {
        use test_capnp::test_big_struct;

        // Make the first segment small to force allocation of a second segment.
        let mut message = message::Builder::new(message::HeapAllocator::new().first_segment_words(5));

        let mut big_struct = message.init_root::<test_big_struct::Builder<'_>>();

        big_struct.set_bool_field(false);
        big_struct.set_int8_field(-128);
        big_struct.set_int16_field(0);
        big_struct.set_int32_field(1009);

        assert_eq!(big_struct.has_struct_field(), false);
        big_struct.reborrow().init_struct_field();
        assert_eq!(big_struct.has_struct_field(), true);
        {
            let mut inner = big_struct.reborrow().get_struct_field().unwrap();
            inner.set_float64_field(0.1234567);
            inner.set_bool_field_b(true);
        }

        big_struct.set_bool_field(true);


        let big_struct_reader = big_struct.into_reader();
        assert_eq!(big_struct_reader.has_struct_field(), true);
        assert_eq!(big_struct_reader.get_int8_field(), -128);
        assert_eq!(big_struct_reader.get_int32_field(), 1009);

        let inner_reader = big_struct_reader.get_struct_field().unwrap();
        assert!(!inner_reader.get_bool_field_a());
        assert!(inner_reader.get_bool_field_b());
        assert_eq!(inner_reader.get_float64_field(), 0.1234567);
    }

    #[test]
    fn test_complex_list () {
        use test_capnp::{test_complex_list, AnEnum};

        let mut message = message::Builder::new_default();

        let mut test_complex_list = message.init_root::<test_complex_list::Builder<'_>>();

        {
            {
                let mut enum_list = test_complex_list.reborrow().init_enum_list(100);
                for i in 0..10 {
                    enum_list.set(i, AnEnum::Qux);
                }
                for i in 10..20 {
                    enum_list.set(i, AnEnum::Bar);
                }
            }

            {
                let mut text_list = test_complex_list.reborrow().init_text_list(2);
                text_list.set(0, "garply");
                text_list.set(1, "foo");
            }

            {
                let mut data_list = test_complex_list.reborrow().init_data_list(2);
                data_list.set(0, &[0u8, 1u8, 2u8]);
                data_list.set(1, &[255u8, 254u8, 253u8]);
            }

            {
                let mut prim_list_list = test_complex_list.reborrow().init_prim_list_list(2);
                {
                    let mut prim_list = prim_list_list.reborrow().init(0, 3);
                    prim_list.set(0, 5);
                    prim_list.set(1, 6);
                    prim_list.set(2, 7);
                    assert_eq!(prim_list.len(), 3);
                }
                let mut prim_list = prim_list_list.init(1, 1);
                prim_list.set(0,-1);
            }

            {
                let mut prim_list_list_list = test_complex_list.reborrow().init_prim_list_list_list(2);
                {
                    let mut prim_list_list = prim_list_list_list.reborrow().init(0, 2);
                    {
                        let mut prim_list = prim_list_list.reborrow().init(0, 2);
                        prim_list.set(0, 0);
                        prim_list.set(1, 1);
                    }
                    let mut prim_list = prim_list_list.init(1, 1);
                    prim_list.set(0, 255);
                }
                let prim_list_list = prim_list_list_list.init(1, 1);
                let mut prim_list = prim_list_list.init(0, 3);
                prim_list.set(0, 10);
                prim_list.set(1, 9);
                prim_list.set(2, 8);
            }

            {
                let mut enum_list_list = test_complex_list.reborrow().init_enum_list_list(2);
                {
                    let mut enum_list = enum_list_list.reborrow().init(0, 1);
                    enum_list.set(0, AnEnum::Bar);
                }
                let mut enum_list = enum_list_list.init(1, 2);
                enum_list.set(0, AnEnum::Foo);
                enum_list.set(1, AnEnum::Qux);
            }

            {
                let text_list_list = test_complex_list.reborrow().init_text_list_list(1);
                text_list_list.init(0,1).set(0, "abc");
            }

            {
                let data_list_list = test_complex_list.reborrow().init_data_list_list(1);
                data_list_list.init(0,1).set(0, &[255, 254, 253]);
            }

            {
                let struct_list_list = test_complex_list.reborrow().init_struct_list_list(1);
                struct_list_list.init(0,1).get(0).set_int8_field(-1);
            }
        }

        let complex_list_reader = test_complex_list.into_reader();
        let enum_list_reader = complex_list_reader.get_enum_list().unwrap();
        for i in 0..10 {
            assert!(enum_list_reader.get(i) == Ok(AnEnum::Qux));
        }
        for i in 10..20 {
            assert!(enum_list_reader.get(i) == Ok(AnEnum::Bar));
        }

        let text_list = complex_list_reader.get_text_list().unwrap();
        assert_eq!(text_list.len(), 2);
        assert_eq!(text_list.get(0).unwrap(), "garply");
        assert_eq!(text_list.get(1).unwrap(), "foo");

        let data_list = complex_list_reader.get_data_list().unwrap();
        assert_eq!(data_list.len(), 2);
        assert!(data_list.get(0).unwrap() == [0u8, 1u8, 2u8]);
        assert!(data_list.get(1).unwrap() == [255u8, 254u8, 253u8]);

        let prim_list_list = complex_list_reader.get_prim_list_list().unwrap();
        assert_eq!(prim_list_list.len(), 2);
        assert_eq!(prim_list_list.get(0).unwrap().len(), 3);
        assert!(prim_list_list.get(0).unwrap().get(0) == 5);
        assert!(prim_list_list.get(0).unwrap().get(1) == 6);
        assert!(prim_list_list.get(0).unwrap().get(2) == 7);
        assert!(prim_list_list.get(1).unwrap().get(0) == -1);

        let prim_list_list_list = complex_list_reader.get_prim_list_list_list().unwrap();
        assert!(prim_list_list_list.get(0).unwrap().get(0).unwrap().get(0) == 0);
        assert!(prim_list_list_list.get(0).unwrap().get(0).unwrap().get(1) == 1);
        assert!(prim_list_list_list.get(0).unwrap().get(1).unwrap().get(0) == 255);
        assert!(prim_list_list_list.get(1).unwrap().get(0).unwrap().get(0) == 10);
        assert!(prim_list_list_list.get(1).unwrap().get(0).unwrap().get(1) == 9);
        assert!(prim_list_list_list.get(1).unwrap().get(0).unwrap().get(2) == 8);

        let enum_list_list = complex_list_reader.get_enum_list_list().unwrap();
        assert!(enum_list_list.get(0).unwrap().get(0) == Ok(AnEnum::Bar));
        assert!(enum_list_list.get(1).unwrap().get(0) == Ok(AnEnum::Foo));
        assert!(enum_list_list.get(1).unwrap().get(1) == Ok(AnEnum::Qux));

        assert!(complex_list_reader.get_text_list_list().unwrap().get(0).unwrap().get(0).unwrap() == "abc");
        assert!(complex_list_reader.get_data_list_list().unwrap().get(0).unwrap().get(0).unwrap() == [255, 254, 253]);

        assert!(complex_list_reader.get_struct_list_list().unwrap().get(0).unwrap().get(0).get_int8_field() == -1);
    }

    #[test]
    fn test_defaults() {
        use test_capnp::test_defaults;

        {
            let message = message::Builder::new_default();
            let test_defaults = message.get_root_as_reader::<test_defaults::Reader<'_>>()
                .expect("get_root_as_reader()");
            ::test_util::CheckTestMessage::check_test_message(test_defaults);
        }

        {
            let mut message = message::Builder::new_default();
            let test_defaults = message.init_root::<test_defaults::Builder<'_>>();
            ::test_util::CheckTestMessage::check_test_message(test_defaults);
        }

        {
            let mut message = message::Builder::new_default();
            let mut test_defaults = message.get_root::<test_defaults::Builder<'_>>()
                .expect("get_root()");
            test_defaults.set_bool_field(false);
            test_defaults.set_int8_field(63);
            test_defaults.set_int16_field(-1123);
            test_defaults.set_int32_field(445678);
            test_defaults.set_int64_field(-990123456789);
            test_defaults.set_u_int8_field(234);
            test_defaults.set_u_int16_field(56789);
            test_defaults.set_u_int32_field(123456789);
            test_defaults.set_u_int64_field(123456789012345);
            test_defaults.set_float32_field(7890.123);
            test_defaults.set_float64_field(5e55);

            {
                let mut sub_builder = test_defaults.reborrow().get_struct_field().unwrap();
                sub_builder.set_text_field("garply");
            }

            assert_eq!(test_defaults.reborrow().get_bool_field(), false);
            assert_eq!(test_defaults.reborrow().get_int8_field(), 63);
            assert_eq!(test_defaults.reborrow().get_int16_field(), -1123);
            assert_eq!(test_defaults.reborrow().get_int32_field(),  445678);
            assert_eq!(test_defaults.reborrow().get_int64_field(), -990123456789);
            assert_eq!(test_defaults.reborrow().get_u_int8_field(), 234);
            assert_eq!(test_defaults.reborrow().get_u_int16_field(), 56789);
            assert_eq!(test_defaults.reborrow().get_u_int32_field(),  123456789);
            assert_eq!(test_defaults.reborrow().get_u_int64_field(),  123456789012345);
            assert_eq!(test_defaults.reborrow().get_float32_field(), 7890.123);
            assert_eq!(test_defaults.reborrow().get_float64_field(), 5e55);

            {
                let sub_builder = test_defaults.reborrow().get_struct_field().unwrap();
                assert_eq!("garply", &*sub_builder.get_text_field().unwrap());
            }
        }
    }

    #[test]
    fn test_default_initialization_multi_segment() {
        use test_capnp::test_defaults;
        let builder_options = message::HeapAllocator::new()
            .first_segment_words(1).allocation_strategy(::capnp::message::AllocationStrategy::FixedSize);
        let mut message = message::Builder::new(builder_options);
        let test_defaults = message.init_root::<test_defaults::Builder<'_>>();
        ::test_util::CheckTestMessage::check_test_message(test_defaults);
    }

    #[test]
    fn test_any_pointer() {
        use test_capnp::{test_any_pointer, test_empty_struct, test_big_struct};

        let mut message = message::Builder::new_default();
        let mut test_any_pointer = message.init_root::<test_any_pointer::Builder<'_>>();

        test_any_pointer.reborrow().init_any_pointer_field().set_as("xyzzy").unwrap();

        {
            let reader = test_any_pointer.reborrow().into_reader();
            assert_eq!(reader.get_any_pointer_field().get_as::<::capnp::text::Reader<'_>>().unwrap(), "xyzzy");
        }

        test_any_pointer.reborrow().get_any_pointer_field().init_as::<test_empty_struct::Builder<'_>>();
        test_any_pointer.reborrow().get_any_pointer_field().get_as::<test_empty_struct::Builder<'_>>().unwrap();

        {
            let reader = test_any_pointer.reborrow().into_reader();
            reader.get_any_pointer_field().get_as::<test_empty_struct::Reader<'_>>().unwrap();
        }

        {
            let mut message = message::Builder::new_default();
            let mut test_big_struct = message.init_root::<test_big_struct::Builder<'_>>();
            test_big_struct.set_int32_field(-12345);
            test_any_pointer.get_any_pointer_field().set_as(test_big_struct.reborrow().into_reader()).unwrap();
        }

        fn _test_lifetimes(body: test_big_struct::Reader<'_>) {
            let mut message = message::Builder::new_default();
            message.set_root(body).unwrap();
        }
    }

    #[test]
    fn test_writable_struct_pointer() {
        use test_capnp::test_big_struct;

        let mut message = message::Builder::new_default();
        let mut big_struct = message.init_root::<test_big_struct::Builder<'_>>();


        let neg_seven : u64 = (-7i64) as u64;
        {
            let mut struct_field = big_struct.reborrow().init_struct_field();
            assert_eq!(struct_field.reborrow().get_uint64_field(), 0);

            struct_field.set_uint64_field(neg_seven);
            assert_eq!(struct_field.get_uint64_field(), neg_seven);
        }
        assert_eq!(big_struct.reborrow().get_struct_field().unwrap().get_uint64_field(), neg_seven);
        {
            let mut struct_field = big_struct.reborrow().init_struct_field();
            assert_eq!(struct_field.reborrow().get_uint64_field(), 0);
            assert_eq!(struct_field.get_uint32_field(), 0);
        }

        {
            // getting before init is the same as init
            assert_eq!(big_struct.reborrow().get_another_struct_field().unwrap().get_uint64_field(), 0);
            big_struct.reborrow().get_another_struct_field().unwrap().set_uint32_field(4294967265);

            // Alas, we need to make a copy to appease the reborrow checker.
            let mut other_message = message::Builder::new_default();
            other_message.set_root(big_struct.reborrow().get_another_struct_field().unwrap().into_reader()).unwrap();
            big_struct.set_struct_field(
                other_message.get_root::<test_big_struct::inner::Builder<'_>>().unwrap().into_reader()).unwrap();
        }

        assert_eq!(big_struct.reborrow().get_struct_field().unwrap().get_uint32_field(), 4294967265);
        {
            let mut other_struct_field = big_struct.reborrow().get_another_struct_field().unwrap();
            assert_eq!(other_struct_field.reborrow().get_uint32_field(), 4294967265);
            other_struct_field.set_uint32_field(42);
            assert_eq!(other_struct_field.get_uint32_field(), 42);
        }
        assert_eq!(big_struct.reborrow().get_struct_field().unwrap().get_uint32_field(), 4294967265);
        assert_eq!(big_struct.get_another_struct_field().unwrap().get_uint32_field(), 42);
    }

    #[test]
    fn test_generic_one_parameter() {
        use test_capnp::brand_once;

        let mut message_for_brand = message::Builder::new_default();
        let mut branded = message_for_brand.init_root::<brand_once::Builder<'_>>();
        {
            let branded_field = branded.reborrow().init_branded_field();
            let mut foo = branded_field.init_generic_field();
            foo.set_text_field("blah");
        }

        let reader = branded.into_reader();
        assert_eq!("blah", reader.get_branded_field().unwrap().get_generic_field().unwrap().get_text_field().unwrap());
    }

    #[test]
    fn test_generic_two_parameter() {
        use test_capnp::brand_twice;

        let mut message_for_brand = message::Builder::new_default();
        let mut branded = message_for_brand.init_root::<brand_twice::Builder<'_>>();
        {
            let mut baz = branded.reborrow().init_baz_field();
            baz.set_foo_field("blah").unwrap();
            let mut bar = baz.init_bar_field();
            bar.set_text_field("some text");
            bar.set_data_field("some data".as_bytes());
        }

        let reader = branded.into_reader();
        assert_eq!("blah", reader.get_baz_field().unwrap().get_foo_field().unwrap());
        assert_eq!("some text", reader.get_baz_field().unwrap().get_bar_field().unwrap().get_text_field().unwrap());
        assert_eq!("some data".as_bytes(), reader.get_baz_field().unwrap().get_bar_field().unwrap().get_data_field().unwrap());
    }

    #[test]
    fn test_generics() {
        use capnp::text;
        use test_capnp::{test_generics, test_all_types};
        let mut message = message::Builder::new_default();
        let mut root: test_generics::Builder<'_, test_all_types::Owned, text::Owned> = message.init_root();
        ::test_util::init_test_message(root.reborrow().get_foo().unwrap());
        root.reborrow().get_dub().unwrap().set_foo("Hello").unwrap();
        {
            let mut bar: ::capnp::primitive_list::Builder<'_,u8> = root.reborrow().get_dub().unwrap().initn_bar(1);
            bar.set(0, 11);
        }
        {
            let mut rev_bar = root.reborrow().get_rev().unwrap().get_bar().unwrap();
            rev_bar.set_int8_field(111);
            let mut bool_list = rev_bar.init_bool_list(2);
            bool_list.set(0, false);
            bool_list.set(1, true);
        }

        ::test_util::CheckTestMessage::check_test_message(root.reborrow().get_foo().unwrap());
        let root_reader = root.into_reader();
        ::test_util::CheckTestMessage::check_test_message(root_reader.reborrow().get_foo().unwrap());
        let dub_reader = root_reader.get_dub().unwrap();
        assert_eq!("Hello", dub_reader.get_foo().unwrap());
        let bar_reader = dub_reader.get_bar().unwrap();
        assert_eq!(bar_reader.len(), 1);
        assert_eq!(bar_reader.get(0), 11);
    }

    #[test]
    fn test_generic_union() {
        use capnp::primitive_list;
        use test_capnp::{test_generics_union, test_all_types};
        let mut message = message::Builder::new_default();
        {
            let mut root: test_generics_union::Builder<'_, test_all_types::Owned, primitive_list::Owned<u32>>
                = message.init_root();
            {
                let mut bar = root.reborrow().initn_bar1(10);
                bar.set(5, 100);
            }
            assert!(!root.has_foo1());
            assert!(root.has_bar1());
            assert!(!root.has_foo2());

            match root.reborrow().which().unwrap() {
                test_generics_union::Bar1(Ok(bar)) => {
                    assert_eq!(bar.len(), 10);
                    assert_eq!(bar.get(0), 0);
                    assert_eq!(bar.get(5), 100);
                    assert_eq!(bar.get(9), 0);
                }
                _ => panic!("expected Bar1"),
            }

            {
                let mut foo = root.reborrow().init_foo2();
                foo.set_int32_field(37);
            }

            assert!(!root.has_foo1());
            assert!(!root.has_bar1());
            assert!(root.has_foo2());

            match root.reborrow().which().unwrap() {
                test_generics_union::Foo2(Ok(foo)) => {
                    assert_eq!(foo.get_int32_field(), 37);
                }
                _ => panic!("expected Foo2"),
            }
        }
    }

    #[test]
    fn test_union() {
        use test_capnp::test_union;

        let mut message = message::Builder::new_default();
        let mut union_struct = message.init_root::<test_union::Builder<'_>>();

        union_struct.reborrow().get_union0().set_u0f0s0(());
        match union_struct.reborrow().get_union0().which() {
            Ok(test_union::union0::U0f0s0(())) => {}
            _ => panic!()
        }
        union_struct.reborrow().init_union0().set_u0f0s1(true);
        match union_struct.reborrow().get_union0().which() {
            Ok(test_union::union0::U0f0s1(true)) => {}
            _ => panic!()
        }
        union_struct.reborrow().init_union0().set_u0f0s8(127);
        match union_struct.reborrow().get_union0().which() {
            Ok(test_union::union0::U0f0s8(127)) => {}
            _ => panic!()
        }

        assert_eq!(union_struct.reborrow().get_union0().has_u0f0sp(), false);
        union_struct.reborrow().init_union0().set_u0f0sp("abcdef");
        assert_eq!(union_struct.get_union0().has_u0f0sp(), true);
    }

    #[test]
    fn test_union_defaults() {
        use test_capnp::{test_union, test_union_defaults};

        {
            let message = message::Builder::new_default();
            let reader = message.get_root_as_reader::<test_union_defaults::Reader<'_>>()
                .expect("get_root_as_reader()");
            let field = reader.get_s16s8s64s8_set().unwrap();
            if let test_union::union0::U0f0s16(_) = field.get_union0().which().unwrap() {} else {
                panic!("expected U0f0s16");
            }

            if let test_union_defaults::inner1::A(17) = reader.get_inner1().which().unwrap() {} else {
                panic!("")
            }

            if let test_union_defaults::inner2::C(Ok("grault")) = reader.get_inner2().which().unwrap() {} else {
                panic!("")
            }
        }
    }

    #[test]
    fn test_constants() {
        use test_capnp::{test_constants, TestEnum};
        assert_eq!(test_constants::VOID_CONST, ());
        assert_eq!(test_constants::BOOL_CONST, true);
        assert_eq!(test_constants::INT8_CONST, -123);
        assert_eq!(test_constants::INT16_CONST, -12345);
        assert_eq!(test_constants::INT32_CONST, -12345678);
        assert_eq!(test_constants::INT64_CONST, -123456789012345);
        assert_eq!(test_constants::UINT8_CONST, 234);
        assert_eq!(test_constants::UINT16_CONST, 45678);
        assert_eq!(test_constants::UINT32_CONST, 3456789012);
        assert_eq!(test_constants::UINT64_CONST, 12345678901234567890);
        assert_eq!(test_constants::FLOAT32_CONST, 1234.5);
        assert_eq!(test_constants::FLOAT64_CONST, -123e45);
        assert_eq!(test_constants::TEXT_CONST, "foo");
        assert_eq!(test_constants::COMPLEX_TEXT_CONST, "foo\"☺\'$$$");
        assert_eq!(test_constants::DATA_CONST, b"bar");
        {
            let struct_const_root = test_constants::STRUCT_CONST.get().unwrap();
            assert_eq!(struct_const_root.get_bool_field(), true);
            assert_eq!(struct_const_root.get_int8_field(), -12);
            assert_eq!(struct_const_root.get_int16_field(), 3456);
            assert_eq!(struct_const_root.get_int32_field(), -78901234);
            // ...
            assert_eq!(struct_const_root.get_text_field().unwrap(), "baz");
            assert_eq!(struct_const_root.get_data_field().unwrap(), b"qux");
            {
                let sub_reader = struct_const_root.get_struct_field().unwrap();
                assert_eq!(sub_reader.get_text_field().unwrap(), "nested");
                assert_eq!(sub_reader.get_struct_field().unwrap().get_text_field().unwrap(), "really nested");
            }
            // ...
        }

        assert!(test_constants::ENUM_CONST == TestEnum::Corge);

        let void_list = test_constants::VOID_LIST_CONST;
        assert_eq!(void_list.get().unwrap().len(), 6);

        let bool_list_const = test_constants::BOOL_LIST_CONST;
        let bool_list = bool_list_const.get().unwrap();
        assert_eq!(bool_list.len(), 4);
        assert_eq!(bool_list.get(0), true);
        assert_eq!(bool_list.get(1), false);
        assert_eq!(bool_list.get(2), false);
        assert_eq!(bool_list.get(3), true);

        let int8_list_const = test_constants::INT8_LIST_CONST;
        let int8_list = int8_list_const.get().unwrap();
        assert_eq!(int8_list.len(), 2);
        assert_eq!(int8_list.get(0), 111);
        assert_eq!(int8_list.get(1), -111);

        // ...

        let text_list_const = test_constants::TEXT_LIST_CONST;
        let text_list = text_list_const.get().unwrap();
        assert_eq!(text_list.len(), 3);
        assert_eq!(text_list.get(0).unwrap(), "plugh");
        assert_eq!(text_list.get(1).unwrap(), "xyzzy");
        assert_eq!(text_list.get(2).unwrap(), "thud");

        // TODO: DATA_LIST_CONST

        let struct_list_const = test_constants::STRUCT_LIST_CONST;
        let struct_list = struct_list_const.get().unwrap();
        assert_eq!(struct_list.len(), 3);
        assert_eq!(struct_list.get(0).get_text_field().unwrap(), "structlist 1");
        assert_eq!(struct_list.get(1).get_text_field().unwrap(), "structlist 2");
        assert_eq!(struct_list.get(2).get_text_field().unwrap(), "structlist 3");
    }

    #[test]
    fn test_set_root() {
        use test_capnp::test_big_struct;

        let mut message1 = message::Builder::new_default();
        let mut message2 = message::Builder::new_default();
        let mut struct1 = message1.init_root::<test_big_struct::Builder<'_>>();
        struct1.set_uint8_field(3);
        message2.set_root(struct1.into_reader()).unwrap();
        let struct2 = message2.get_root::<test_big_struct::Builder<'_>>().unwrap();

        assert_eq!(struct2.get_uint8_field(), 3u8);
    }

    #[test]
    fn upgrade_struct() {
        use test_capnp::{test_old_version, test_new_version};

        let mut message = message::Builder::new_default();
        {
            let mut old_version = message.init_root::<test_old_version::Builder<'_>>();
            old_version.set_old1(123);
        }
        {
            let mut new_version = message.get_root::<test_new_version::Builder<'_>>().unwrap();
            new_version.reborrow().get_new2().unwrap();
            assert_eq!(new_version.get_new3().unwrap().get_int8_field(), -123);
        }
    }

    #[test]
    fn upgrade_union() {
        use test_capnp::{test_old_union_version, test_new_union_version};
        // This tests for a specific case that was broken originally.
        let mut message = message::Builder::new_default();
        {
            let mut old_version = message.init_root::<test_old_union_version::Builder<'_>>();
            old_version.set_b(123);
        }

        {
            let new_version = message.get_root::<test_new_union_version::Builder<'_>>().unwrap();
            match new_version.which().unwrap() {
                test_new_union_version::B(n) => assert_eq!(n, 123),
                _ => panic!("expected B"),
            }
        }
    }

    #[test]
    fn upgrade_list() {
        use test_capnp::{test_any_pointer, test_lists};

        {
            let mut builder = message::Builder::new_default();
            let mut root = builder.init_root::<test_any_pointer::Builder<'_>>();
            {
                let mut list = root.reborrow()
                    .get_any_pointer_field().initn_as::<::capnp::primitive_list::Builder<'_,u8>>(3);
                list.set(0, 12);
                list.set(1, 34);
                list.set(2, 56);
            }
            {
                let mut l = root.get_any_pointer_field()
                    .get_as::<::capnp::struct_list::Builder<'_,test_lists::struct8::Owned>>().unwrap();
                assert_eq!(3, l.len());
                assert_eq!(12, l.reborrow().get(0).get_f());
                assert_eq!(34, l.reborrow().get(1).get_f());
                assert_eq!(56, l.reborrow().get(2).get_f());
            }
        }

        {
            let mut builder = message::Builder::new_default();
            let mut root = builder.init_root::<test_any_pointer::Builder<'_>>();
            {
                let mut list = root.reborrow()
                    .get_any_pointer_field().initn_as::<::capnp::text_list::Builder<'_>>(3);
                list.set(0, "foo");
                list.set(1, "bar");
                list.set(2, "baz");
            }
            {
                let mut l = root.get_any_pointer_field()
                    .get_as::<::capnp::struct_list::Builder<'_,test_lists::struct_p::Owned>>().unwrap();
                assert_eq!(3, l.len());
                assert_eq!("foo", &*l.reborrow().get(0).get_f().unwrap());
                assert_eq!("bar", &*l.reborrow().get(1).get_f().unwrap());
                assert_eq!("baz", &*l.reborrow().get(2).get_f().unwrap());
            }
        }
    }

    #[test]
    fn upgrade_struct_list() {
        use capnp::struct_list;
        use test_capnp::{test_old_version, test_new_version};

        let segment0: &[capnp::Word] = &[
            capnp::word(1,0,0,0,0x1f,0,0,0), // list, inline composite, 3 words
            capnp::word(4, 0, 0, 0, 1, 0, 2, 0), // struct tag. 1 element, 1 word data, 2 pointers.
            capnp::word(0xab,0,0,0,0,0,0,0),
            capnp::word(0x05,0,0,0, 0x42,0,0,0), // list pointer, offset 1, type = BYTE, length 8.
            capnp::word(0,0,0,0,0,0,0,0),
            capnp::word(0x68,0x65,0x6c,0x6c,0x6f,0x21,0x21,0), // "hello!!"
        ];

        let segment_array = &[capnp::Word::words_to_bytes(segment0)];
        let message_reader =
            message::Reader::new(message::SegmentArray::new(segment_array), ReaderOptions::new());

        let old_version: struct_list::Reader<'_,test_old_version::Owned> = message_reader.get_root().unwrap();
        assert_eq!(old_version.len(), 1);
        assert_eq!(old_version.get(0).get_old1(), 0xab);
        assert_eq!(old_version.get(0).get_old2().unwrap(), "hello!!");

        // Make the first segment exactly large enough to fit the original message.
        // This leaves no room for a far pointer landing pad in the first segment.
        let allocator = message::HeapAllocator::new().first_segment_words(6);

        let mut message = message::Builder::new(allocator);
        message.set_root(old_version).unwrap();
        {
            let segments = message.get_segments_for_output();
            assert_eq!(segments.len(), 1);
            assert_eq!(segments[0].len(), 6 * 8);
        }

        {
            let mut new_version: struct_list::Builder<'_,test_new_version::Owned> = message.get_root().unwrap();
            assert_eq!(new_version.len(), 1);
            assert_eq!(new_version.reborrow().get(0).get_old1(), 0xab);
            assert_eq!(&*new_version.reborrow().get(0).get_old2().unwrap(), "hello!!");
        }

        {
            let segments = message.get_segments_for_output();
            // Check the old list, including the tag, was zeroed.
            assert_eq!(&segments[0][8..40], &[0; 32][..]);
        }

    }

    #[test]
    fn all_types() {
        use test_capnp::{test_all_types};

        let mut message = message::Builder::new_default();
        ::test_util::init_test_message(message.init_root());
        ::test_util::CheckTestMessage::check_test_message(message.get_root::<test_all_types::Builder<'_>>().unwrap());
        ::test_util::CheckTestMessage::check_test_message(
            message.get_root::<test_all_types::Builder<'_>>().unwrap().into_reader());
    }

    #[test]
    fn all_types_multi_segment() {
        use test_capnp::{test_all_types};

        let builder_options = message::HeapAllocator::new()
            .first_segment_words(1).allocation_strategy(::capnp::message::AllocationStrategy::FixedSize);
        let mut message = message::Builder::new(builder_options);
        ::test_util::init_test_message(message.init_root());
        ::test_util::CheckTestMessage::check_test_message(message.get_root::<test_all_types::Builder<'_>>().unwrap());
        ::test_util::CheckTestMessage::check_test_message(
            message.get_root::<test_all_types::Builder<'_>>().unwrap().into_reader());
    }

    #[test]
    fn setters() {
        use test_capnp::{test_all_types};

        {
            let mut message = message::Builder::new_default();

            ::test_util::init_test_message(message.init_root::<test_all_types::Builder<'_>>());

            let mut message2 = message::Builder::new_default();
            let mut all_types2 = message2.init_root::<test_all_types::Builder<'_>>();

            all_types2.set_struct_field(message.get_root::<test_all_types::Builder<'_>>().unwrap().into_reader()).unwrap();
            ::test_util::CheckTestMessage::check_test_message(all_types2.reborrow().get_struct_field().unwrap());

            let reader = all_types2.into_reader().get_struct_field().unwrap();
            ::test_util::CheckTestMessage::check_test_message(reader);
        }

        {
            let builder_options = message::HeapAllocator::new()
                .first_segment_words(1)
                .allocation_strategy(::capnp::message::AllocationStrategy::FixedSize);
            let mut message = message::Builder::new(builder_options);

            ::test_util::init_test_message(message.init_root::<test_all_types::Builder<'_>>());

            let builder_options = message::HeapAllocator::new()
                .first_segment_words(1)
                .allocation_strategy(::capnp::message::AllocationStrategy::FixedSize);
            let mut message2 = message::Builder::new(builder_options);
            let mut all_types2 = message2.init_root::<test_all_types::Builder<'_>>();

            all_types2.set_struct_field(message.get_root_as_reader().unwrap()).unwrap();
            ::test_util::CheckTestMessage::check_test_message(all_types2.reborrow().get_struct_field().unwrap());

            let reader = all_types2.into_reader().get_struct_field().unwrap();
            ::test_util::CheckTestMessage::check_test_message(reader);
        }
    }

    #[test]
    fn double_far_pointer() {
        let segment0: &[capnp::Word] = &[
            capnp::word(0,0,0,0,0,0,1,0),
            // struct pointer, zero offset, zero data words, one pointer.

            capnp::word(6,0,0,0,1,0,0,0),
            // far pointer, two-word landing pad, offset 0, segment 1.
        ];

        let segment1: &[capnp::Word] = &[
            capnp::word(2,0,0,0,2,0,0,0),
            // landing pad start. offset 0, segment 2

            capnp::word(0,0,0,0,1,0,1,0),
            // landing pad tag. struct pointer. One data word. One pointer.
        ];

        let segment2: &[capnp::Word] = &[
            capnp::word(0x1f,0x1f,0x1f,0x1f,0x1f,0x1f,0x1f,0x1f),
            // Data word.

            capnp::word(1,0,0,0,0x42,0,0,0),
            // text pointer. offset zero. 1-byte elements. 8 total elements.

            capnp::word('h' as u8, 'e' as u8, 'l' as u8, 'l' as u8, 'o' as u8, '.' as u8, '\n' as u8, 0),
        ];

        let segment_array = &[capnp::Word::words_to_bytes(segment0),
                              capnp::Word::words_to_bytes(segment1),
                              capnp::Word::words_to_bytes(segment2)];

        let message =
            message::Reader::new(message::SegmentArray::new(segment_array), ReaderOptions::new());

        let root: ::test_capnp::test_any_pointer::Reader<'_> = message.get_root().unwrap();
        let s: ::test_capnp::test_all_types::Reader<'_> = root.get_any_pointer_field().get_as().unwrap();
        assert_eq!(s.get_int8_field(), 0x1f);
        assert_eq!(s.get_int16_field(), 0x1f1f);
        assert_eq!(s.get_text_field().unwrap(), "hello.\n");
    }

    #[test]
    fn double_far_pointer_truncated_pad() {
        let segment0: &[capnp::Word] = &[
            capnp::word(6,0,0,0,1,0,0,0),
            // far pointer, two-word landing pad, offset 0, segment 1.
        ];

        let segment1: &[capnp::Word] = &[
            capnp::word(2,0,0,0,2,0,0,0),
            // landing pad start. offset 0, segment 2

            // For this message to be valid, there would need to be another word here.
        ];
        let segment2: &[capnp::Word] = &[
            capnp::word(0,0,0,0,0,0,0,0),
        ];

        let segment_array = &[capnp::Word::words_to_bytes(segment0),
                              capnp::Word::words_to_bytes(segment1),
                              capnp::Word::words_to_bytes(segment2)];
        let message =
            message::Reader::new(message::SegmentArray::new(segment_array), ReaderOptions::new());

        match message.get_root::<::test_capnp::test_all_types::Reader<'_>>() {
            Ok(_) => panic!("expected out-of-bounds error"),
            Err(e) => {
                assert_eq!(e.description, "message contained out-of-bounds pointer")
            }
        }
    }

    #[test]
    fn double_far_pointer_out_of_bounds() {
        let segment0: &[capnp::Word] = &[
            capnp::word(6,0,0,0,1,0,0,0),
            // far pointer, two-word landing pad, offset 0, segment 1.
        ];

        let segment1: &[capnp::Word] = &[
            capnp::word(0xa,0,0,0,2,0,0,0),
            // landing pad start. offset 1, segment 2

            capnp::word(0,0,0,0,1,0,1,0),
            // landing pad tag. struct pointer. One data word. One pointer.
        ];
        let segment2: &[capnp::Word] = &[
            capnp::word(0,0,0,0,0,0,0,0),
        ];

        let segment_array = &[capnp::Word::words_to_bytes(segment0),
                              capnp::Word::words_to_bytes(segment1),
                              capnp::Word::words_to_bytes(segment2)];
        let message =
            message::Reader::new(message::SegmentArray::new(segment_array), ReaderOptions::new());

        match message.get_root::<::test_capnp::test_all_types::Reader<'_>>() {
            Ok(_) => panic!("expected out-of-bounds error"),
            Err(e) => {
                assert_eq!(e.description, "message contained out-of-bounds pointer")
            }
        }
    }

    #[test]
    fn far_pointer_pointing_at_self() {
        use test_capnp::test_all_types;

        let words: &[capnp::Word] =
            &[capnp::word(0,0,0,0,0,0,1,0), // struct, one pointer
              capnp::word(0xa,0,0,0,0,0,0,0)]; // far pointer, points to self
        let segment_array = &[capnp::Word::words_to_bytes(words)];

        let message_reader =
            message::Reader::new(message::SegmentArray::new(segment_array), ReaderOptions::new());

        let reader = message_reader.get_root::<test_all_types::Reader<'_>>().unwrap();
        assert!(reader.total_size().is_err());
        let mut builder = ::capnp::message::Builder::new_default();
        assert!(builder.set_root(reader).is_err());
    }


    #[test]
    fn text_builder_int_underflow() {
        use test_capnp::{test_any_pointer};

        let mut message = message::Builder::new_default();
        {
            let mut root = message.init_root::<test_any_pointer::Builder<'_>>();
            let _: ::capnp::data::Builder<'_> = root.reborrow().get_any_pointer_field().initn_as(0);

            // No NUL terminator!
            let result = root.get_any_pointer_field().get_as::<::capnp::text::Builder<'_>>();
            assert!(result.is_err());
        }
    }

    #[test]
    fn inline_composite_list_int_overflow() {
        let words: &[capnp::Word] = &[
            capnp::word(0,0,0,0,0,0,1,0),
            capnp::word(1,0,0,0,0x17,0,0,0),
            capnp::word(0,0,0,128,16,0,0,0),
            capnp::word(0,0,0,0,0,0,0,0),
            capnp::word(0,0,0,0,0,0,0,0)];
        let segment_array = &[capnp::Word::words_to_bytes(words)];

        let message =
            message::Reader::new(message::SegmentArray::new(segment_array), ReaderOptions::new());

        let root: ::test_capnp::test_any_pointer::Reader<'_> = message.get_root().unwrap();
        match root.total_size() {
            Err(e) =>
                assert_eq!("InlineComposite list's elements overrun its word count.", e.description),
            _ => panic!("did not get expected error")
        }

        {
            let result = root.get_any_pointer_field()
                .get_as::<::capnp::struct_list::Reader<'_,::test_capnp::test_all_types::Owned>>();

            assert!(result.is_err());
        }

        let mut message_builder = message::Builder::new_default();
        let builder_root = message_builder.init_root::<::test_capnp::test_any_pointer::Builder<'_>>();
        match builder_root.get_any_pointer_field().set_as(root) {
            Err(e) =>
                assert_eq!("InlineComposite list's elements overrun its word count.", e.description),
            _ => panic!("did not get expected error"),
        }
    }

    #[test]
    fn long_u64_list() {
        use test_capnp::{test_all_types};

        let length: u32 = 1 << 27;
        let step_exponent = 18;

        let mut message = message::Builder::new_default();
        {
            let root: test_all_types::Builder<'_> = message.init_root();
            let mut list = root.init_u_int64_list(length);
            for ii in 0..(length >> step_exponent) {
                let jj = ii << step_exponent;
                list.set(jj, jj as u64);
            }
            for ii in 0..(length >> step_exponent) {
                let jj = ii << step_exponent;
                assert_eq!(list.get(jj), jj as u64);
            }
        }

        let root: test_all_types::Reader<'_> = message.get_root_as_reader().unwrap();
        let list = root.get_u_int64_list().unwrap();
        for ii in 0..(length >> step_exponent) {
            let jj = ii << step_exponent;
            assert_eq!(list.get(jj), jj as u64);
        }
    }

    #[test]
    fn long_struct_list() {
        use test_capnp::{test_lists};

        let length: u32 = 1 << 27;
        let step_exponent = 18;

        let mut message = message::Builder::new_default();
        {
            let root: test_lists::Builder<'_> = message.init_root();
            let mut list = root.init_list64(length);
            for ii in 0..(length >> step_exponent) {
                let jj = ii << step_exponent;
                list.reborrow().get(jj).set_f(jj as u64);
            }
            for ii in 0..(length >> step_exponent) {
                let jj = ii << step_exponent;
                assert_eq!(list.reborrow().get(jj).get_f(), jj as u64);
            }
        }

        let root: test_lists::Reader<'_> = message.get_root_as_reader().unwrap();
        let list = root.get_list64().unwrap();
        for ii in 0..(length >> step_exponent) {
            let jj = ii << step_exponent;
            assert_eq!(list.get(jj).get_f(), jj as u64);
        }
    }

    #[test]
    fn long_list_list() {
        use test_capnp::{test_lists};

        let length: u32 = 1 << 27;
        let step_exponent = 18;

        let mut message = message::Builder::new_default();
        {
            let root: test_lists::Builder<'_> = message.init_root();
            let mut list = root.init_int32_list_list(length);
            for ii in 0..(length >> step_exponent) {
                let jj = ii << step_exponent;
                list.reborrow().init(jj, 1).set(0, jj as i32);
            }
            for ii in 0..(length >> step_exponent) {
                let jj = ii << step_exponent;
                let elem = list.reborrow().get(jj).unwrap();
                assert_eq!(elem.len(), 1);
                assert_eq!(elem.get(0), jj as i32);
            }
        }

        let root: test_lists::Reader<'_> = message.get_root_as_reader().unwrap();
        let list = root.get_int32_list_list().unwrap();
        for ii in 0..(length >> step_exponent) {
            let jj = ii << step_exponent;
            let elem = list.get(jj).unwrap();
            assert_eq!(elem.len(), 1);
            assert_eq!(elem.get(0), jj as i32);
        }
    }

    #[test]
    fn traversal_limit_exceeded() {
        use test_capnp::{test_all_types};

        let mut message = message::Builder::new_default();
        ::test_util::init_test_message(message.init_root());

        let segments = message.get_segments_for_output();
        let reader = message::Reader::new(message::SegmentArray::new(&segments),
                                          *ReaderOptions::new().traversal_limit_in_words(Some(2)));
        match reader.get_root::<test_all_types::Reader<'_>>() {
            Err(e) => assert_eq!(e.description, "read limit exceeded"),
            Ok(_) => panic!("expected error"),
        }
    }

    #[test]
    fn void_list_amplification() {
        use test_capnp::{test_any_pointer, test_all_types};

        let mut message = message::Builder::new_default();
        {
            let root = message.init_root::<test_any_pointer::Builder<'_>>();
            let _: ::capnp::primitive_list::Builder<'_,()> =
                root.get_any_pointer_field().initn_as((1 << 29) - 1);
        }
        let segments = message.get_segments_for_output();
        assert_eq!(segments.len(), 1);
        assert_eq!(segments[0].len(), 16); // 2 words

        let reader = message::Reader::new(message::SegmentArray::new(&segments),
                                          ReaderOptions::new());
        let root = reader.get_root::<test_any_pointer::Reader<'_>>().unwrap();
        let result = root.get_any_pointer_field().get_as::<::capnp::struct_list::Reader<'_,test_all_types::Owned>>();
        assert!(result.is_err());
    }

    #[test]
    fn empty_struct_list_amplification() {
        use test_capnp::{test_any_pointer, test_empty_struct, test_all_types};

        let mut message = message::Builder::new_default();
        {
            let root = message.init_root::<test_any_pointer::Builder<'_>>();
            let _ : ::capnp::struct_list::Builder<'_, test_empty_struct::Owned> =
                root.get_any_pointer_field().initn_as((1 << 29) - 1);
        }
        {
            let segments = message.get_segments_for_output();
            assert_eq!(segments.len(), 1);
            assert_eq!(segments[0].len(), 3 * 8); // 3 words

            let reader =
                message::Reader::new(message::SegmentArray::new(&segments),
                                     ReaderOptions::new());
            let root = reader.get_root::<test_any_pointer::Reader<'_>>().unwrap();
            let result = root.get_any_pointer_field().get_as::<::capnp::struct_list::Reader<'_, test_all_types::Owned>>();
            assert!(result.is_err());
        }

        // At one point this took a long time because zero_object_helper() would iterate through
        // the whole list, even though its elements were void.
        message.init_root::<test_any_pointer::Builder<'_>>();
    }

    #[test]
    fn total_size_struct_list_amplification() {
        use test_capnp::test_any_pointer;

        let words: &[capnp::Word] =
            &[capnp::word(0,0,0,0, 0,0,1,0), // struct, one pointers
              capnp::word(1,0,0,0, 0xf,0,0,0), // list, inline composite, one word
              capnp::word(0,0x80,0xc2,0xff, 0,0,0,0), // large struct, but zero of them
              capnp::word(0,0,0x20,0, 0,0,0x22,0),
            ];
        let segment_array = &[capnp::Word::words_to_bytes(words)];

        let message_reader =
            message::Reader::new(message::SegmentArray::new(segment_array), ReaderOptions::new());

        let reader = message_reader.get_root::<test_any_pointer::Reader<'_>>().unwrap();
        reader.total_size().unwrap();

        let mut builder = ::capnp::message::Builder::new_default();
        assert!(builder.set_root(reader).is_err()); // read limit exceeded
    }

    #[test]
    fn null_struct_fields() {
        use test_capnp::{test_all_types};
        let mut message = message::Builder::new_default();
        {
	    let mut test = message.init_root::<test_all_types::Builder<'_>>();
	    test.set_text_field("Hello");
        }
        let reader = message.get_root::<test_all_types::Builder<'_>>().unwrap().into_reader();
        assert_eq!(reader.get_text_field().unwrap(), "Hello");
        assert_eq!(reader.has_struct_field(), false);
        let nested = reader.get_struct_field().unwrap();
        assert_eq!(nested.get_int8_field(), 0);
        assert_eq!(nested.get_u_int64_field(), 0);
        assert_eq!(nested.get_void_list().unwrap().len(), 0);
        assert_eq!(nested.get_float64_list().unwrap().len(), 0);
        assert_eq!(nested.get_struct_list().unwrap().len(), 0);
        assert_eq!(nested.get_text_field().unwrap(), "");
        let empty_slice: &[u8] = &[];
        assert_eq!(nested.get_data_field().unwrap(), empty_slice);
    }

    // At one point this failed to typecheck, giving the error:
    // "no method named `get_any_pointer_field` found for type `test_capnp::test_any_pointer::Pipeline`"
    #[allow(unused)]
    fn pipeline_any_pointer(foo: ::test_capnp::test_any_pointer::Pipeline) {
        let _ = foo.get_any_pointer_field();
    }

    #[test]
    fn set_with_caveats() {
        use test_capnp::test_all_types;
        let mut message = message::Builder::new_default();
        let root: test_all_types::Builder<'_> = message.init_root();
        let list = root.init_struct_list(2);
        {
            let mut message1 = message::Builder::new_default();
            let mut root1: test_all_types::Builder<'_> = message1.init_root();
            root1.set_int8_field(11);
            list.set_with_caveats(0, root1.into_reader()).unwrap();
        }
        {
            let mut message2 = message::Builder::new_default();
            let mut root2: test_all_types::Builder<'_> = message2.init_root();
            ::test_util::init_test_message(root2.reborrow());
            list.set_with_caveats(1, root2.into_reader()).unwrap();
        }

        let list_reader = list.into_reader();
        assert_eq!(11, list_reader.get(0).get_int8_field());
        ::test_util::CheckTestMessage::check_test_message(list_reader.get(1));
    }

    #[test]
    fn get_raw_struct_data() {
        use capnp::traits::HasStructSize;
        use test_capnp::test_all_types;
        let mut message = message::Builder::new_default();
        let mut root: test_all_types::Builder<'_> = message.init_root();
        root.set_int8_field(3);
        root.set_int16_field(0x0abb);
        let struct_size = <test_all_types::Builder<'_> as HasStructSize>::STRUCT_SIZE;
        {
            let raw_bytes =
                ::capnp::raw::get_struct_data_section(root.reborrow().into_reader());
            assert_eq!(raw_bytes.len(), (struct_size.data * 8) as usize);
            assert_eq!(raw_bytes[0], 0); // boolField
            assert_eq!(raw_bytes[1], 3); // int8Field
            assert_eq!(raw_bytes[2], 0xbb); // int16Field less significant byte
            assert_eq!(raw_bytes[3], 0x0a); // int16Field more significant byte
        }
    }

    #[test]
    fn get_raw_list_data() {
        use test_capnp::test_all_types;
        let mut message = message::Builder::new_default();
        let mut root: test_all_types::Builder<'_> = message.init_root();
        {
            let mut uint16_list = root.reborrow().init_u_int16_list(5);
            uint16_list.set(0, 10);
            uint16_list.set(1, 11);
            uint16_list.set(2, 12);
            uint16_list.set(3, 13);
            uint16_list.set(4, 14);
            assert_eq!(
                ::capnp::raw::get_list_element_size(uint16_list.reborrow().into_reader()),
                ::capnp::private::layout::ElementSize::TwoBytes);

            assert_eq!(
                ::capnp::raw::get_list_step_size_in_bits(uint16_list.reborrow().into_reader()),
                16);

            assert_eq!(
                ::capnp::raw::get_list_bytes(uint16_list.reborrow().into_reader()),
                &[10, 0, 11, 0, 12, 0, 13, 0, 14, 0]);
        }
    }

    #[test]
    fn get_struct_pointer_section() {
        use test_capnp::test_all_types;
        let mut message = message::Builder::new_default();
        let mut root: test_all_types::Builder<'_> = message.init_root();
        ::test_util::init_test_message(root.reborrow().init_struct_field());
        let pointers = ::capnp::raw::get_struct_pointer_section(root.into_reader());
        let substruct: test_all_types::Reader<'_> = pointers.get(2).get_as().unwrap();
        ::test_util::CheckTestMessage::check_test_message(substruct);
    }

    #[test]
    fn struct_list_iterator() {
        use test_capnp::test_all_types;
        let mut message = message::Builder::new_default();
        {
            let root: test_all_types::Builder<'_> = message.init_root();
            let mut struct_list = root.init_struct_list(6);
            let mut t0 = struct_list.reborrow().get(0);
            t0.set_u_int32_field(0);

            let mut t1 = struct_list.reborrow().get(1);
            t1.set_u_int32_field(1);

            let mut t2 = struct_list.reborrow().get(2);
            t2.set_u_int32_field(2);

            let mut t3 = struct_list.reborrow().get(3);
            t3.set_u_int32_field(3);

            let mut t4 = struct_list.reborrow().get(4);
            t4.set_u_int32_field(4);

            let mut t5 = struct_list.reborrow().get(5);
            t5.set_u_int32_field(5);
        }

        let reader = message.get_root_as_reader::<test_all_types::Reader<'_>>().unwrap();
        let structs = reader.get_struct_list().unwrap();

        let mut iter = structs.iter();
        assert_eq!(3, iter.nth(3).unwrap().get_u_int32_field());
        assert_eq!(4, iter.nth(0).unwrap().get_u_int32_field());
        assert_eq!(5, iter.nth(0).unwrap().get_u_int32_field());

        let mut c = 2;
        for s in structs.iter().skip(2) {
            assert_eq!(c, s.get_u_int32_field());
            c += 1;
        }

        {
            let mut overflow_iter = structs.iter();
            assert!(overflow_iter.nth(4).is_some());

            // The first four elements have been consumed, so going another 4 should overflow.
            assert!(overflow_iter.nth(4).is_none());

            // The previous call pushed us to the end, even though it returned None.
            assert!(overflow_iter.next().is_none());
        }
    }

    #[test]
    fn name_annotation() {
        use test_capnp::renamed_struct;
        let mut message = message::Builder::new_default();
        {
            let mut root: renamed_struct::Builder<'_> = message.init_root();
            root.set_good_field_name(true);
            root.set_another_good_field_name(renamed_struct::RenamedEnum::Bar);

            let renamed_union = root.get_renamed_union();
            renamed_union.init_qux();
        }
        {
            let root: renamed_struct::Reader<'_> = message.get_root_as_reader().unwrap();
            match root.which().unwrap() {
                renamed_struct::GoodFieldName(true) => (),
                _ => panic!("expected GoodFieldName(true)"),
            }
            assert!(renamed_struct::RenamedEnum::Bar == root.get_another_good_field_name().unwrap());


            match root.get_renamed_union().which().unwrap() {
                renamed_struct::renamed_union::Qux(_) => (),
                _ => panic!("expected Qux"),
            }
        }
    }
}
