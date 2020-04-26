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
                alice_phones.reborrow().get(0).set_type(person::phone_number::Type::Mobile);
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
                bob_phones.reborrow().get(0).set_type(person::phone_number::Type::Home);
                bob_phones.reborrow().get(1).set_number("555-7654");
                bob_phones.reborrow().get(1).set_type(person::phone_number::Type::Work);
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
    fn write_stream_and_read_queue() {
        use capnp;
        use capnp_futures;
        use futures::future::{FutureExt};
        use futures::stream::{StreamExt};
        use futures::task::LocalSpawn;

        use std::cell::Cell;
        use std::rc::Rc;

        let (s1, s2) = async_std::os::unix::net::UnixStream::pair().expect("socket pair");

        let (mut sender, write_queue) = capnp_futures::write_queue(s1);

        let read_stream = capnp_futures::ReadStream::new(s2, Default::default());

        let messages_read = Rc::new(Cell::new(0u32));
        let messages_read1 = messages_read.clone();

        let done_reading = read_stream.for_each(|m| {
            match m {
                Err(e) => panic!("read error: {:?}", e),
                Ok(msg) => {
                    let address_book = msg.get_root::<address_book::Reader>().unwrap();
                    read_address_book(address_book);
                    messages_read.set(messages_read.get() + 1);
                    futures::future::ready(())
                }
            }
        });

        let io = futures::future::join(done_reading, write_queue.map(|_| ()));

        let mut m = capnp::message::Builder::new_default();
        populate_address_book(m.init_root());
        let mut exec = futures::executor::LocalPool::new();
        let spawner = exec.spawner();
        spawner.spawn_local_obj(Box::new(sender.send(m).map(|_|())).into()).expect("spawing write task");
        drop(sender);

        exec.run_until(io);

        assert_eq!(messages_read1.get(), 1);
    }

    fn fill_and_send_message(mut message: capnp::message::Builder<capnp::message::HeapAllocator>) {
        use capnp_futures::serialize;
        use futures::{FutureExt, TryFutureExt};
        use futures::task::LocalSpawn;

        {
            let mut address_book = message.init_root::<address_book::Builder>();
            populate_address_book(address_book.reborrow());
            read_address_book(address_book.reborrow_as_reader());
        }

        let (stream0, stream1) = async_std::os::unix::net::UnixStream::pair().expect("socket pair");

        let f0 = serialize::write_message(stream0, message).map_err(|e| panic!("write error {:?}", e)).map(|_|());
        let f1 =
            serialize::read_message(stream1, capnp::message::ReaderOptions::new()).and_then(|maybe_message_reader| {
                match maybe_message_reader {
                    None => panic!("did not get message"),
                    Some(m) => {
                        let address_book = m.get_root::<address_book::Reader>().unwrap();
                        read_address_book(address_book);
                        futures::future::ready(Ok::<(),capnp::Error>(()))
                    }
                }
            });

        let mut exec = futures::executor::LocalPool::new();
        let spawner = exec.spawner();
        spawner.spawn_local_obj(Box::new(f0).into()).expect("spawing write task");

        exec.run_until(f1).expect("read task");
    }

    #[test]
    fn single_segment() {
        fill_and_send_message(capnp::message::Builder::new_default());
    }

    #[test]
    fn multi_segment() {
        let builder_options = capnp::message::HeapAllocator::new()
            .first_segment_words(1).allocation_strategy(capnp::message::AllocationStrategy::FixedSize);
        fill_and_send_message(capnp::message::Builder::new(builder_options));
    }

    #[test]
    fn static_lifetime_not_required_funcs() {
        use capnp::message;
        use capnp_futures::serialize;

        let (mut write, mut read) =
            async_std::os::unix::net::UnixStream::pair().expect("socket pair");
        let _ = serialize::read_message(&mut read, message::ReaderOptions::default());
        let _ = serialize::write_message(&mut write, message::Builder::new_default());
        drop(write);
        drop(read);
    }

    #[test]
    fn static_lifetime_not_required_on_highlevel() {
        use capnp::message;
        use capnp_futures;

        let (mut write, mut read) =
            async_std::os::unix::net::UnixStream::pair().expect("socket pair");
        let _ = capnp_futures::ReadStream::new(&mut read, message::ReaderOptions::default());
        let _ = capnp_futures::write_queue::<_, message::Builder<message::HeapAllocator>>(&mut write);
        drop(write);
        drop(read);
    }
}
