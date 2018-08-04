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

extern crate capnp;
pub mod addressbook_capnp {
    include!(concat!(env!("OUT_DIR"), "/addressbook_capnp.rs"));
}

use capnp::message::{Builder, HeapAllocator, TypedReader};
use std::sync::mpsc;
use std::thread;

pub mod addressbook {
    use addressbook_capnp::{address_book, person};
    use capnp::message::{Builder, HeapAllocator, TypedReader};

    pub fn build_address_book() -> TypedReader<Builder<HeapAllocator>, address_book::Owned> {
        let mut message = Builder::new_default();
        {
            let address_book = message.init_root::<address_book::Builder>();

            let mut people = address_book.init_people(2);

            {
                let mut alice = people.reborrow().get(0);
                alice.set_id(123);
                alice.set_name("Alice");
                alice.set_email("alice@example.com");
                {
                    let mut alice_phones = alice.reborrow().init_phones(1);
                    alice_phones.reborrow().get(0).set_number("555-1212");
                    alice_phones
                        .reborrow()
                        .get(0)
                        .set_type(person::phone_number::Type::Mobile);
                }
                alice.get_employment().set_school("MIT");
            }

            {
                let mut bob = people.get(1);
                bob.set_id(456);
                bob.set_name("Bob");
                bob.set_email("bob@example.com");
                {
                    let mut bob_phones = bob.reborrow().init_phones(2);
                    bob_phones.reborrow().get(0).set_number("555-4567");
                    bob_phones
                        .reborrow()
                        .get(0)
                        .set_type(person::phone_number::Type::Home);
                    bob_phones.reborrow().get(1).set_number("555-7654");
                    bob_phones
                        .reborrow()
                        .get(1)
                        .set_type(person::phone_number::Type::Work);
                }
                bob.get_employment().set_unemployed(());
            }
        }

        // There are two ways to get a TypedReader from our `message`:
        //
        // Option 1: Go through the full process manually
        //  message.into_reader().into_typed()
        //
        // Option 2: Use the "Into" trait defined on the builder
        //   message.into()
        //
        // Option 3: Use the "From" trait defined on the builder
        TypedReader::from(message)
    }
}

pub fn main() {
    let book = addressbook::build_address_book();

    let (tx_book, rx_book) = mpsc::channel::<
        TypedReader<Builder<HeapAllocator>, addressbook_capnp::address_book::Owned>,
    >();
    let (tx_id, rx_id) = mpsc::channel::<u32>();

    thread::spawn(move || {
        let addressbook_reader = rx_book.recv().unwrap();
        let addressbook = addressbook_reader.get().unwrap();
        let first_person = addressbook.get_people().unwrap().get(0);
        let first_id = first_person.get_id();
        tx_id.send(first_id)
    });

    tx_book.send(book).unwrap();
    let first_id = rx_id.recv().unwrap();
    assert_eq!(first_id, 123);
}
