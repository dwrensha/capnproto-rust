// Copyright (c) 2013-2015 Sandstorm Development Group, Inc. and contributors
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

#![allow(dead_code)]
// This is experimental for now...

use capnp::Word;
use std::cell::RefCell;

pub struct Arena {
    allocations: RefCell<Vec<Vec<Word>>>,
}

impl Arena {
    pub fn new() -> Arena {
        Arena {
            allocations: RefCell::new(Vec::new()),
        }
    }

    pub fn alloc<'a>(&'a self, length: usize) -> &'a mut [Word] {
        let mut v = ::capnp::Word::allocate_zeroed_vec(length);
        let result = unsafe { ::std::slice::from_raw_parts_mut(v.as_mut_ptr(), v.len()) };
        self.allocations.borrow_mut().push(v);
        result
    }
}

pub struct SchemaLoader<'a> {
    arena: &'a Arena,
    schemas: ::std::collections::hash_map::HashMap<u64, &'a [Word]>,
}

impl<'a> SchemaLoader<'a> {
    pub fn new(arena: &'a Arena) -> SchemaLoader<'a> {
        SchemaLoader {
            arena: arena,
            schemas: ::std::collections::hash_map::HashMap::new(),
        }
    }

    pub fn load(&mut self, reader: ::schema_capnp::node::Reader) {
        let id = reader.get_id();
        let num_words = reader.total_size().unwrap().word_count;
        self.schemas.get(&id);
        let _words = self.arena.alloc(num_words as usize);
        //ScratchSpaceMallocMessageBuilder::new
    }
}
