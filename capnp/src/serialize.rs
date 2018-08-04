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

//! Reading and writing of messages using the
//! [standard stream framing](https://capnproto.org/encoding.html#serialization-over-a-stream).

use std::io::{Read, Write};

use message;
use util::read_exact;
use {Error, Result, Word};

use byteorder::{ByteOrder, LittleEndian};

/// Segments read from a single flat slice of words.
pub struct SliceSegments<'a> {
    words: &'a [Word],
    segment_slices: Vec<(usize, usize)>,
}

impl<'a> message::ReaderSegments for SliceSegments<'a> {
    fn get_segment<'b>(&'b self, id: u32) -> Option<&'b [Word]> {
        if id < self.segment_slices.len() as u32 {
            let (a, b) = self.segment_slices[id as usize];
            Some(&self.words[a..b])
        } else {
            None
        }
    }

    fn len(&self) -> usize {
        self.segment_slices.len()
    }
}

/// Reads a serialized message from a slice of words.
pub fn read_message_from_words<'a>(
    slice: &'a [Word],
    options: message::ReaderOptions,
) -> Result<message::Reader<SliceSegments<'a>>> {
    let mut bytes = ::Word::words_to_bytes(slice);
    let (num_words, offsets) = try!(read_segment_table(&mut bytes, options));
    let words = unsafe { ::Word::bytes_to_words(bytes) };
    if num_words != words.len() {
        Err(Error::failed(format!(
            "Wrong number of words. Header claimed {} words, but message has {} words",
            num_words,
            words.len()
        )))
    } else {
        Ok(message::Reader::new(
            SliceSegments {
                words: words,
                segment_slices: offsets,
            },
            options,
        ))
    }
}

pub struct OwnedSegments {
    segment_slices: Vec<(usize, usize)>,
    owned_space: Vec<Word>,
}

impl ::message::ReaderSegments for OwnedSegments {
    fn get_segment<'a>(&'a self, id: u32) -> Option<&'a [Word]> {
        if id < self.segment_slices.len() as u32 {
            let (a, b) = self.segment_slices[id as usize];
            Some(&self.owned_space[a..b])
        } else {
            None
        }
    }

    fn len(&self) -> usize {
        self.segment_slices.len()
    }
}

/// Reads a serialized message from a stream with the provided options.
///
/// For optimal performance, `read` should be a buffered reader type.
pub fn read_message<R>(
    read: &mut R,
    options: message::ReaderOptions,
) -> Result<message::Reader<OwnedSegments>>
where
    R: Read,
{
    let (total_words, segment_slices) = try!(read_segment_table(read, options));
    read_segments(read, total_words, segment_slices, options)
}

/// Reads a segment table from `read` and returns the total number of words across all
/// segments, as well as the segment offsets.
///
/// The segment table format for streams is defined in the Cap'n Proto
/// [encoding spec](https://capnproto.org/encoding.html)
fn read_segment_table<R>(
    read: &mut R,
    options: message::ReaderOptions,
) -> Result<(usize, Vec<(usize, usize)>)>
where
    R: Read,
{
    let mut buf: [u8; 8] = [0; 8];

    // read the first Word, which contains segment_count and the 1st segment length
    try!(read_exact(read, &mut buf));
    let segment_count = <LittleEndian as ByteOrder>::read_u32(&buf[0..4]).wrapping_add(1) as usize;

    if segment_count >= 512 {
        return Err(Error::failed(format!(
            "Too many segments: {}",
            segment_count
        )));
    } else if segment_count == 0 {
        return Err(Error::failed(format!(
            "Too few segments: {}",
            segment_count
        )));
    }

    let mut segment_slices = Vec::with_capacity(segment_count);
    let mut total_words = <LittleEndian as ByteOrder>::read_u32(&buf[4..8]) as usize;
    segment_slices.push((0, total_words));

    if segment_count > 1 {
        if segment_count < 4 {
            try!(read_exact(read, &mut buf));
            for idx in 0..(segment_count - 1) {
                let segment_len =
                    <LittleEndian as ByteOrder>::read_u32(&buf[(idx * 4)..(idx + 1) * 4]) as usize;

                segment_slices.push((total_words, total_words + segment_len));
                total_words += segment_len;
            }
        } else {
            let mut segment_sizes = vec![0u8; (segment_count & !1) * 4];
            try!(read_exact(read, &mut segment_sizes[..]));
            for idx in 0..(segment_count - 1) {
                let segment_len =
                    <LittleEndian as ByteOrder>::read_u32(&segment_sizes[(idx * 4)..(idx + 1) * 4])
                        as usize;

                segment_slices.push((total_words, total_words + segment_len));
                total_words += segment_len;
            }
        }
    }

    // Don't accept a message which the receiver couldn't possibly traverse without hitting the
    // traversal limit. Without this check, a malicious client could transmit a very large segment
    // size to make the receiver allocate excessive space and possibly crash.
    if total_words as u64 > options.traversal_limit_in_words {
        return Err(Error::failed(format!(
            "Message has {} words, which is too large. To increase the limit on the \
             receiving end, see capnp::message::ReaderOptions.",
            total_words
        )));
    }

    Ok((total_words, segment_slices))
}

/// Reads segments from `read`.
fn read_segments<R>(
    read: &mut R,
    total_words: usize,
    segment_slices: Vec<(usize, usize)>,
    options: message::ReaderOptions,
) -> Result<message::Reader<OwnedSegments>>
where
    R: Read,
{
    let mut owned_space: Vec<Word> = Word::allocate_zeroed_vec(total_words);
    try!(read_exact(
        read,
        Word::words_to_bytes_mut(&mut owned_space[..])
    ));
    let segments = OwnedSegments {
        segment_slices,
        owned_space,
    };
    Ok(::message::Reader::new(segments, options))
}

/// Constructs a flat vector containing the entire message.
pub fn write_message_to_words<A>(message: &message::Builder<A>) -> Vec<Word>
where
    A: message::Allocator,
{
    flatten_segments(&*message.get_segments_for_output())
}

pub fn write_message_segments_to_words<R>(message: &R) -> Vec<Word>
where
    R: message::ReaderSegments,
{
    flatten_segments(message)
}

fn flatten_segments<R: message::ReaderSegments + ?Sized>(segments: &R) -> Vec<Word> {
    let word_count = compute_serialized_size(segments);
    let segment_count = segments.len();
    let table_size = segment_count / 2 + 1;
    let mut result = Vec::with_capacity(word_count);
    for _ in 0..table_size {
        result.push(Word { raw_content: 0 });
    }
    {
        let mut bytes = ::Word::words_to_bytes_mut(&mut result[..]);
        write_segment_table_internal(&mut bytes, segments).expect("Failed to write segment table.");
    }
    for i in 0..segment_count {
        let segment = segments.get_segment(i as u32).unwrap();
        for idx in 0..segment.len() {
            result.push(segment[idx]);
        }
    }
    result
}

/// Writes the provided message to `write`.
///
/// For optimal performance, `write` should be a buffered writer. `flush` will not be called on
/// the writer.
pub fn write_message<W, A>(write: &mut W, message: &message::Builder<A>) -> ::std::io::Result<()>
where
    W: Write,
    A: message::Allocator,
{
    let segments = message.get_segments_for_output();
    try!(write_segment_table(write, &segments));
    write_segments(write, &segments)
}

pub fn write_message_segments<W, R>(write: &mut W, segments: &R) -> ::std::io::Result<()>
where
    W: Write,
    R: message::ReaderSegments,
{
    try!(write_segment_table_internal(write, segments));
    write_segments(write, segments)
}

fn write_segment_table<W>(write: &mut W, segments: &[&[Word]]) -> ::std::io::Result<()>
where
    W: Write,
{
    write_segment_table_internal(write, segments)
}

/// Writes a segment table to `write`.
///
/// `segments` must contain at least one segment.
fn write_segment_table_internal<W, R>(write: &mut W, segments: &R) -> ::std::io::Result<()>
where
    W: Write,
    R: message::ReaderSegments + ?Sized,
{
    let mut buf: [u8; 8] = [0; 8];
    let segment_count = segments.len();

    // write the first Word, which contains segment_count and the 1st segment length
    <LittleEndian as ByteOrder>::write_u32(&mut buf[0..4], segment_count as u32 - 1);
    <LittleEndian as ByteOrder>::write_u32(
        &mut buf[4..8],
        segments.get_segment(0).unwrap().len() as u32,
    );
    try!(write.write_all(&buf));

    if segment_count > 1 {
        if segment_count < 4 {
            for idx in 1..segment_count {
                <LittleEndian as ByteOrder>::write_u32(
                    &mut buf[(idx - 1) * 4..idx * 4],
                    segments.get_segment(idx as u32).unwrap().len() as u32,
                );
            }
            if segment_count == 2 {
                for idx in 4..8 {
                    buf[idx] = 0
                }
            }
            try!(write.write_all(&buf));
        } else {
            let mut buf = vec![0; (segment_count & !1) * 4];
            for idx in 1..segment_count {
                <LittleEndian as ByteOrder>::write_u32(
                    &mut buf[(idx - 1) * 4..idx * 4],
                    segments.get_segment(idx as u32).unwrap().len() as u32,
                );
            }
            if segment_count % 2 == 0 {
                for idx in (buf.len() - 4)..(buf.len()) {
                    buf[idx] = 0
                }
            }
            try!(write.write_all(&buf));
        }
    }
    Ok(())
}

/// Writes segments to `write`.
fn write_segments<W, R: message::ReaderSegments + ?Sized>(
    write: &mut W,
    segments: &R,
) -> ::std::io::Result<()>
where
    W: Write,
{
    for i in 0.. {
        if let Some(segment) = segments.get_segment(i) {
            try!(write.write_all(Word::words_to_bytes(segment)));
        } else {
            break;
        }
    }
    Ok(())
}

fn compute_serialized_size<R: message::ReaderSegments + ?Sized>(segments: &R) -> usize {
    // Table size
    let len = segments.len();
    let mut size = (len / 2) + 1;
    for i in 0..len {
        let segment = segments.get_segment(i as u32).unwrap();
        size += segment.len();
    }
    size
}

/// Returns the number of words required to serialize the message.
pub fn compute_serialized_size_in_words<A>(message: &::message::Builder<A>) -> usize
where
    A: ::message::Allocator,
{
    compute_serialized_size(&message.get_segments_for_output())
}

#[cfg(test)]
pub mod test {

    use std::io::{Cursor, Write};

    use quickcheck::{quickcheck, TestResult};

    use super::{
        flatten_segments, read_message, read_message_from_words, read_segment_table,
        write_segment_table, write_segments,
    };
    use message;
    use message::ReaderSegments;
    use Word;

    /// Writes segments as if they were a Capnproto message.
    pub fn write_message_segments<W>(write: &mut W, segments: &Vec<Vec<Word>>)
    where
        W: Write,
    {
        let borrowed_segments: &[&[Word]] = &segments
            .iter()
            .map(|segment| &segment[..])
            .collect::<Vec<_>>()[..];
        write_segment_table(write, borrowed_segments).unwrap();
        write_segments(write, borrowed_segments).unwrap();
    }

    #[test]
    fn test_read_segment_table() {
        let mut buf = vec![];

        buf.extend(
            [0,0,0,0, // 1 segments
                    0,0,0,0] // 0 length
                    .iter().cloned(),
        );
        let (words, segment_slices) =
            read_segment_table(&mut Cursor::new(&buf[..]), message::ReaderOptions::new()).unwrap();
        assert_eq!(0, words);
        assert_eq!(vec![(0, 0)], segment_slices);
        buf.clear();

        buf.extend(
            [0,0,0,0, // 1 segments
                    1,0,0,0] // 1 length
                    .iter().cloned(),
        );
        let (words, segment_slices) =
            read_segment_table(&mut Cursor::new(&buf[..]), message::ReaderOptions::new()).unwrap();
        assert_eq!(1, words);
        assert_eq!(vec![(0, 1)], segment_slices);
        buf.clear();

        buf.extend(
            [1,0,0,0, // 2 segments
                    1,0,0,0, // 1 length
                    1,0,0,0, // 1 length
                    0,0,0,0] // padding
                    .iter().cloned(),
        );
        let (words, segment_slices) =
            read_segment_table(&mut Cursor::new(&buf[..]), message::ReaderOptions::new()).unwrap();
        assert_eq!(2, words);
        assert_eq!(vec![(0, 1), (1, 2)], segment_slices);
        buf.clear();

        buf.extend(
            [2,0,0,0, // 3 segments
                    1,0,0,0, // 1 length
                    1,0,0,0, // 1 length
                    0,1,0,0] // 256 length
                    .iter().cloned(),
        );
        let (words, segment_slices) =
            read_segment_table(&mut Cursor::new(&buf[..]), message::ReaderOptions::new()).unwrap();
        assert_eq!(258, words);
        assert_eq!(vec![(0, 1), (1, 2), (2, 258)], segment_slices);
        buf.clear();

        buf.extend(
            [3,0,0,0,  // 4 segments
                    77,0,0,0, // 77 length
                    23,0,0,0, // 23 length
                    1,0,0,0,  // 1 length
                    99,0,0,0, // 99 length
                    0,0,0,0]  // padding
                    .iter().cloned(),
        );
        let (words, segment_slices) =
            read_segment_table(&mut Cursor::new(&buf[..]), message::ReaderOptions::new()).unwrap();
        assert_eq!(200, words);
        assert_eq!(
            vec![(0, 77), (77, 100), (100, 101), (101, 200)],
            segment_slices
        );
        buf.clear();
    }

    #[test]
    fn test_read_invalid_segment_table() {
        let mut buf = vec![];

        buf.extend([0, 2, 0, 0].iter().cloned()); // 513 segments
        buf.extend([0; 513 * 4].iter().cloned());
        assert!(
            read_segment_table(&mut Cursor::new(&buf[..]), message::ReaderOptions::new()).is_err()
        );
        buf.clear();

        buf.extend([0, 0, 0, 0].iter().cloned()); // 1 segments
        assert!(
            read_segment_table(&mut Cursor::new(&buf[..]), message::ReaderOptions::new()).is_err()
        );
        buf.clear();

        buf.extend([0, 0, 0, 0].iter().cloned()); // 1 segments
        buf.extend([0; 3].iter().cloned());
        assert!(
            read_segment_table(&mut Cursor::new(&buf[..]), message::ReaderOptions::new()).is_err()
        );
        buf.clear();

        buf.extend([255, 255, 255, 255].iter().cloned()); // 0 segments
        assert!(
            read_segment_table(&mut Cursor::new(&buf[..]), message::ReaderOptions::new()).is_err()
        );
        buf.clear();
    }

    #[test]
    fn test_write_segment_table() {
        let mut buf = vec![];

        let segment_0 = [capnp_word!(0, 0, 0, 0, 0, 0, 0, 0); 0];
        let segment_1 = [capnp_word!(1, 1, 1, 1, 1, 1, 1, 1); 1];
        let segment_199 = [capnp_word!(201, 202, 203, 204, 205, 206, 207, 208); 199];

        write_segment_table(&mut buf, &[&segment_0]).unwrap();
        assert_eq!(
            &[
                0, 0, 0, 0, // 1 segments
                0, 0, 0, 0
            ], // 0 length
            &buf[..]
        );
        buf.clear();

        write_segment_table(&mut buf, &[&segment_1]).unwrap();
        assert_eq!(
            &[
                0, 0, 0, 0, // 1 segments
                1, 0, 0, 0
            ], // 1 length
            &buf[..]
        );
        buf.clear();

        write_segment_table(&mut buf, &[&segment_199]).unwrap();
        assert_eq!(
            &[
                0, 0, 0, 0, // 1 segments
                199, 0, 0, 0
            ], // 199 length
            &buf[..]
        );
        buf.clear();

        write_segment_table(&mut buf, &[&segment_0, &segment_1]).unwrap();
        assert_eq!(
            &[
                1, 0, 0, 0, // 2 segments
                0, 0, 0, 0, // 0 length
                1, 0, 0, 0, // 1 length
                0, 0, 0, 0
            ], // padding
            &buf[..]
        );
        buf.clear();

        write_segment_table(
            &mut buf,
            &[&segment_199, &segment_1, &segment_199, &segment_0],
        ).unwrap();
        assert_eq!(
            &[
                3, 0, 0, 0, // 4 segments
                199, 0, 0, 0, // 199 length
                1, 0, 0, 0, // 1 length
                199, 0, 0, 0, // 199 length
                0, 0, 0, 0, // 0 length
                0, 0, 0, 0
            ], // padding
            &buf[..]
        );
        buf.clear();

        write_segment_table(
            &mut buf,
            &[
                &segment_199,
                &segment_1,
                &segment_199,
                &segment_0,
                &segment_1,
            ],
        ).unwrap();
        assert_eq!(
            &[
                4, 0, 0, 0, // 5 segments
                199, 0, 0, 0, // 199 length
                1, 0, 0, 0, // 1 length
                199, 0, 0, 0, // 199 length
                0, 0, 0, 0, // 0 length
                1, 0, 0, 0
            ], // 1 length
            &buf[..]
        );
        buf.clear();
    }

    #[test]
    fn check_round_trip() {
        fn round_trip(segments: Vec<Vec<Word>>) -> TestResult {
            if segments.len() == 0 {
                return TestResult::discard();
            }
            let mut cursor = Cursor::new(Vec::new());

            write_message_segments(&mut cursor, &segments);
            cursor.set_position(0);

            let message = read_message(&mut cursor, message::ReaderOptions::new()).unwrap();
            let result_segments = message.into_segments();

            TestResult::from_bool(
                segments.iter().enumerate().all(|(i, segment)| {
                    &segment[..] == result_segments.get_segment(i as u32).unwrap()
                }),
            )
        }

        quickcheck(round_trip as fn(Vec<Vec<Word>>) -> TestResult);
    }

    #[test]
    fn check_round_trip_slice_segments() {
        fn round_trip(segments: Vec<Vec<Word>>) -> TestResult {
            if segments.len() == 0 {
                return TestResult::discard();
            }
            let borrowed_segments: &[&[Word]] = &segments
                .iter()
                .map(|segment| &segment[..])
                .collect::<Vec<_>>()[..];
            let words = flatten_segments(borrowed_segments);
            let message =
                read_message_from_words(&words[..], message::ReaderOptions::new()).unwrap();
            let result_segments = message.into_segments();

            TestResult::from_bool(
                segments.iter().enumerate().all(|(i, segment)| {
                    &segment[..] == result_segments.get_segment(i as u32).unwrap()
                }),
            )
        }

        quickcheck(round_trip as fn(Vec<Vec<Word>>) -> TestResult);
    }
}
