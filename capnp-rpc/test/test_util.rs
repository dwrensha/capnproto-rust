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

use test_capnp::{test_all_types, TestEnum};

pub fn init_test_message(mut builder: test_all_types::Builder) {
    builder.set_void_field(());
    builder.set_bool_field(true);
    builder.set_int8_field(-123);
    builder.set_int16_field(-12345);
    builder.set_int32_field(-12345678);
    builder.set_int64_field(-123456789012345);
    builder.set_u_int8_field(234);
    builder.set_u_int16_field(45678);
    builder.set_u_int32_field(3456789012);
    builder.set_u_int64_field(12345678901234567890);
    builder.set_float32_field(1234.5);
    builder.set_float64_field(-123e45);
    builder.set_text_field("foo");
    builder.set_data_field(b"bar");
    {
        let mut sub_builder = builder.reborrow().init_struct_field();
        sub_builder.set_void_field(());
        sub_builder.set_bool_field(true);
        sub_builder.set_int8_field(-12);
        sub_builder.set_int16_field(3456);
        sub_builder.set_int32_field(-78901234);
        sub_builder.set_int64_field(56789012345678);
        sub_builder.set_u_int8_field(90);
        sub_builder.set_u_int16_field(1234);
        sub_builder.set_u_int32_field(56789012);
        sub_builder.set_u_int64_field(345678901234567890);
        sub_builder.set_float32_field(-1.25e-10);
        sub_builder.set_float64_field(345f64);
        sub_builder.set_text_field("baz");
        sub_builder.set_data_field(b"qux");
        {
            let mut sub_sub_builder = sub_builder.reborrow().init_struct_field();
            sub_sub_builder.set_text_field("nested");
            sub_sub_builder
                .init_struct_field()
                .set_text_field("really nested");
        }
        sub_builder.set_enum_field(TestEnum::Baz);

        sub_builder.reborrow().init_void_list(3);
        {
            let mut bool_list = sub_builder.reborrow().init_bool_list(5);
            bool_list.set(0, false);
            bool_list.set(1, true);
            bool_list.set(2, false);
            bool_list.set(3, true);
            bool_list.set(4, true);
        }
        {
            let mut int8_list = sub_builder.reborrow().init_int8_list(4);
            int8_list.set(0, 12);
            int8_list.set(1, -34);
            int8_list.set(2, -0x80);
            int8_list.set(3, 0x7f);
        }
        {
            let mut int16_list = sub_builder.reborrow().init_int16_list(4);
            int16_list.set(0, 1234);
            int16_list.set(1, -5678);
            int16_list.set(2, -0x8000);
            int16_list.set(3, 0x7fff);
        }
        {
            let mut int32_list = sub_builder.reborrow().init_int32_list(4);
            int32_list.set(0, 12345678);
            int32_list.set(1, -90123456);
            int32_list.set(2, -0x80000000);
            int32_list.set(3, 0x7fffffff);
        }
        {
            let mut int64_list = sub_builder.reborrow().init_int64_list(4);
            int64_list.set(0, 123456789012345);
            int64_list.set(1, -678901234567890);
            int64_list.set(2, -0x8000000000000000);
            int64_list.set(3, 0x7fffffffffffffff);
        }

        // ...
        {
            let mut struct_list = sub_builder.reborrow().init_struct_list(3);
            struct_list
                .reborrow()
                .get(0)
                .set_text_field("x structlist 1");
            struct_list
                .reborrow()
                .get(1)
                .set_text_field("x structlist 2");
            struct_list
                .reborrow()
                .get(2)
                .set_text_field("x structlist 3");
        }

        let mut enum_list = sub_builder.reborrow().init_enum_list(3);
        enum_list.set(0, TestEnum::Qux);
        enum_list.set(1, TestEnum::Bar);
        enum_list.set(2, TestEnum::Grault);
    }
    builder.set_enum_field(TestEnum::Corge);

    builder.init_void_list(6);

    // ...
}

pub trait CheckTestMessage {
    fn check_test_message(Self);
}

macro_rules!
check_test_message_impl(($typ:ident) => (
    impl <'a> CheckTestMessage for test_all_types::$typ<'a> {
        fn check_test_message(mut reader : test_all_types::$typ<'a>) {
            #![allow(unused_mut)]
            reader.reborrow().get_void_field();
            assert_eq!(true, reader.reborrow().get_bool_field());
            assert_eq!(-123, reader.reborrow().get_int8_field());
            assert_eq!(-12345, reader.reborrow().get_int16_field());
            assert_eq!(-12345678, reader.reborrow().get_int32_field());
            assert_eq!(-123456789012345, reader.reborrow().get_int64_field());
            assert_eq!(234, reader.reborrow().get_u_int8_field());
            assert_eq!(45678, reader.reborrow().get_u_int16_field());
            assert_eq!(3456789012, reader.reborrow().get_u_int32_field());
            assert_eq!(12345678901234567890, reader.reborrow().get_u_int64_field());
            assert_eq!(1234.5, reader.reborrow().get_float32_field());
            assert_eq!(-123e45, reader.reborrow().get_float64_field());
            assert_eq!("foo", &*reader.reborrow().get_text_field().unwrap());
            assert_eq!(b"bar", &*reader.reborrow().get_data_field().unwrap());
            {
                let mut sub_reader = reader.get_struct_field().unwrap();
                assert_eq!((), sub_reader.reborrow().get_void_field());
                assert_eq!(true, sub_reader.reborrow().get_bool_field());
                assert_eq!(-12, sub_reader.reborrow().get_int8_field());
                assert_eq!(3456, sub_reader.reborrow().get_int16_field());
                assert_eq!(-78901234, sub_reader.reborrow().get_int32_field());
                assert_eq!(56789012345678, sub_reader.reborrow().get_int64_field());
                assert_eq!(90, sub_reader.reborrow().get_u_int8_field());
                assert_eq!(1234, sub_reader.reborrow().get_u_int16_field());
                assert_eq!(56789012, sub_reader.reborrow().get_u_int32_field());
                assert_eq!(345678901234567890, sub_reader.reborrow().get_u_int64_field());
                assert_eq!(-1.25e-10, sub_reader.reborrow().get_float32_field());
                assert_eq!(345f64, sub_reader.reborrow().get_float64_field());
                assert_eq!("baz", &*sub_reader.reborrow().get_text_field().unwrap());
                assert_eq!(b"qux", &*sub_reader.reborrow().get_data_field().unwrap());
                {
                    let mut sub_sub_reader = sub_reader.reborrow().get_struct_field().unwrap();
                    assert_eq!("nested", &*sub_sub_reader.reborrow().get_text_field().unwrap());
                    assert_eq!("really nested", &*sub_sub_reader.get_struct_field().unwrap()
                                                                .get_text_field().unwrap());
                }
                assert!(Ok(TestEnum::Baz) == sub_reader.reborrow().get_enum_field());
                assert_eq!(3, sub_reader.reborrow().get_void_list().unwrap().len());

                {
                    let bool_list = sub_reader.reborrow().get_bool_list().unwrap();
                    assert_eq!(5, bool_list.len());
                    assert_eq!(false, bool_list.get(0));
                    assert_eq!(true, bool_list.get(1));
                    assert_eq!(false, bool_list.get(2));
                    assert_eq!(true, bool_list.get(3));
                    assert_eq!(true, bool_list.get(4));
                }

                {
                    let int8_list = sub_reader.reborrow().get_int8_list().unwrap();
                    assert_eq!(4, int8_list.len());
                    assert_eq!(12, int8_list.get(0));
                    assert_eq!(-34, int8_list.get(1));
                    assert_eq!(-0x80, int8_list.get(2));
                    assert_eq!(0x7f, int8_list.get(3));
                }

                {
                    let int16_list = sub_reader.reborrow().get_int16_list().unwrap();
                    assert_eq!(4, int16_list.len());
                    assert_eq!(1234, int16_list.get(0));
                    assert_eq!(-5678, int16_list.get(1));
                    assert_eq!(-0x8000, int16_list.get(2));
                    assert_eq!(0x7fff, int16_list.get(3));
                }

                {
                    let int32_list = sub_reader.reborrow().get_int32_list().unwrap();
                    assert_eq!(4, int32_list.len());
                    assert_eq!(12345678, int32_list.get(0));
                    assert_eq!(-90123456, int32_list.get(1));
                    assert_eq!(-0x80000000, int32_list.get(2));
                    assert_eq!(0x7fffffff, int32_list.get(3));
                }

                // ...

                {
                    let mut struct_list = sub_reader.reborrow().get_struct_list().unwrap();
                    assert_eq!(3, struct_list.len());
                    assert_eq!("x structlist 1", &*struct_list.reborrow().get(0).get_text_field().unwrap());
                    assert_eq!("x structlist 2", &*struct_list.reborrow().get(1).get_text_field().unwrap());
                    assert_eq!("x structlist 3", &*struct_list.reborrow().get(2).get_text_field().unwrap());
                }

                {
                    let enum_list = sub_reader.get_enum_list().unwrap();
                    assert_eq!(3, enum_list.len());
                    assert!(Ok(TestEnum::Qux) == enum_list.get(0));
                    assert!(Ok(TestEnum::Bar) == enum_list.get(1));
                    assert!(Ok(TestEnum::Grault) == enum_list.get(2));
                }
            }
        }
    }
));

check_test_message_impl!(Reader);

check_test_message_impl!(Builder);
