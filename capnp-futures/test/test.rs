// Copyright (c) 2015 Sandstorm Development Group, Inc. and contributors
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
extern crate capnp_futures;
extern crate futures;
extern crate mio_uds;
extern crate tokio_core;

pub mod addressbook_capnp {
    include!(concat!(env!("OUT_DIR"), "/addressbook_capnp.rs"));
}

#[cfg(test)]
mod tests {
    use addressbook_capnp::{address_book, person};

    fn populate_address_book(address_book: address_book::Builder) {
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

    fn read_address_book(address_book: address_book::Reader) {
        let people = address_book.get_people().unwrap();
        assert_eq!(people.len(), 2);
        let alice = people.get(0);
        assert_eq!(alice.get_id(), 123);
        assert_eq!(alice.get_name().unwrap(), "Alice");
        assert_eq!(alice.get_email().unwrap(), "alice@example.com");

        let bob = people.get(1);
        assert_eq!(bob.get_id(), 456);
        assert_eq!(bob.get_name().unwrap(), "Bob");
    }

    #[test]
    fn foo() {
        use capnp;
        use capnp_futures;
        use futures::future::Future;
        use futures::stream::Stream;
        use mio_uds::UnixStream;
        use tokio_core::reactor;

        use std::cell::Cell;
        use std::rc::Rc;

        let mut l = reactor::Core::new().unwrap();
        let handle = l.handle();
        let (s1, s2) = UnixStream::pair().unwrap();
        let s1 = reactor::PollEvented::new(s1, &handle).unwrap();
        let s2 = reactor::PollEvented::new(s2, &handle).unwrap();

        let (mut sender, write_queue) = capnp_futures::write_queue(s1);

        let read_stream = capnp_futures::ReadStream::new(s2, Default::default());

        let messages_read = Rc::new(Cell::new(0u32));
        let messages_read1 = messages_read.clone();

        let done_reading = read_stream.for_each(|m| {
            let address_book = m.get_root::<address_book::Reader>().unwrap();
            read_address_book(address_book);
            messages_read.set(messages_read.get() + 1);
            Ok(())
        });

        let io = done_reading.join(write_queue.map(|_| ()));

        let mut m = capnp::message::Builder::new_default();
        populate_address_book(m.init_root());
        handle.spawn(sender.send(m).map_err(|_| panic!("cancelled")).map(|_| {
            println!("SENT");
            ()
        }));
        drop(sender);

        l.run(io).expect("running");

        assert_eq!(messages_read1.get(), 1);
    }
    /*
    fn fill_and_send_message(mut message: message::Builder<message::HeapAllocator>) {
        {
            let mut address_book = message.init_root::<address_book::Builder>();
            populate_address_book(address_book.borrow());
            read_address_book(address_book.borrow_as_reader());
        }

        gj::EventLoop::top_level(move |wait_scope| -> Result<(), ::std::io::Error> {
            let mut event_port = try!(::gjio::EventPort::new());
            let network = event_port.get_network();
            let (stream0, stream1) = try!(network.new_socket_pair());

            let promise0 = serialize::write_message(stream0, message).map(|_| Ok(()));
            let promise1 =
                serialize::read_message(stream1, message::ReaderOptions::new()).then(|(_, message_reader)| {
                    let address_book = message_reader.get_root::<address_book::Reader>().unwrap();
                    read_address_book(address_book);
                    gj::Promise::ok(())
                });

            gj::Promise::all(vec![promise0, promise1].into_iter()).wait(wait_scope, &mut event_port).unwrap();
            Ok(())
        }).unwrap();

    }

    #[test]
    fn single_segment() {
        fill_and_send_message(message::Builder::new_default());
    }

    #[test]
    fn multi_segment() {
        let builder_options = message::HeapAllocator::new()
            .first_segment_words(1).allocation_strategy(::capnp::message::AllocationStrategy::FixedSize);
        fill_and_send_message(message::Builder::new(builder_options));
    }
*/
}
