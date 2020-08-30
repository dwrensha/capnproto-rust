// Copyright (c) 2013-2016 Sandstorm Development Group, Inc. and contributors
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

//! Asynchronous reading and writing of messages using the
//! [standard stream framing](https://capnproto.org/encoding.html#serialization-over-a-stream).

use std::convert::TryInto;

use capnp::serialize::{OwnedSegments, SegmentLengthsBuilder};
use capnp::{message, Error, OutputSegments, Result};

use futures_io::{AsyncRead, AsyncWrite};
use futures_util::{AsyncReadExt, AsyncWriteExt};

/// Begins an asynchronous read of a message from `reader`.
pub async fn read_message<R>(
    mut reader: R,
    options: message::ReaderOptions,
) -> Result<Option<message::Reader<OwnedSegments>>>
where
    R: AsyncRead + Unpin,
{
    let segment_lengths_builder = match read_segment_table(&mut reader, options).await? {
        Some(s) => s,
        None => return Ok(None),
    };
    Ok(Some(
        read_segments(
            reader,
            segment_lengths_builder.into_owned_segments(),
            options,
        )
        .await?,
    ))
}

async fn read_segment_table<R>(
    mut reader: R,
    options: message::ReaderOptions,
) -> Result<Option<SegmentLengthsBuilder>>
where
    R: AsyncRead + Unpin,
{
    let mut buf: [u8; 8] = [0; 8];
    {
        let n = reader.read(&mut buf[..]).await?;
        if n == 0 {
            return Ok(None);
        } else if n < 8 {
            reader.read_exact(&mut buf[n..]).await?;
        }
    }
    let (segment_count, first_segment_length) = parse_segment_table_first(&buf[..])?;

    let mut segment_lengths_builder = SegmentLengthsBuilder::with_capacity(segment_count);
    segment_lengths_builder.push_segment(first_segment_length);
    if segment_count > 1 {
        if segment_count < 4 {
            // small enough that we can reuse our existing buffer
            reader.read_exact(&mut buf).await?;
            for idx in 0..(segment_count - 1) {
                let segment_len =
                    u32::from_le_bytes(buf[(idx * 4)..(idx + 1) * 4].try_into().unwrap()) as usize;
                segment_lengths_builder.push_segment(segment_len);
            }
        } else {
            let mut segment_sizes = vec![0u8; (segment_count & !1) * 4];
            reader.read_exact(&mut segment_sizes[..]).await?;
            for idx in 0..(segment_count - 1) {
                let segment_len =
                    u32::from_le_bytes(segment_sizes[(idx * 4)..(idx + 1) * 4].try_into().unwrap())
                        as usize;
                segment_lengths_builder.push_segment(segment_len);
            }
        }
    }

    // Don't accept a message which the receiver couldn't possibly traverse without hitting the
    // traversal limit. Without this check, a malicious client could transmit a very large segment
    // size to make the receiver allocate excessive space and possibly crash.
    if segment_lengths_builder.total_words() as u64 > options.traversal_limit_in_words {
        return Err(Error::failed(format!(
            "Message has {} words, which is too large. To increase the limit on the \
             receiving end, see capnp::message::ReaderOptions.",
            segment_lengths_builder.total_words()
        )));
    }

    Ok(Some(segment_lengths_builder))
}

/// Reads segments from `read`.
async fn read_segments<R>(
    mut read: R,
    mut owned_segments: OwnedSegments,
    options: message::ReaderOptions,
) -> Result<message::Reader<OwnedSegments>>
where
    R: AsyncRead + Unpin,
{
    read.read_exact(&mut owned_segments[..]).await?;
    Ok(message::Reader::new(owned_segments, options))
}

/// Parses the first word of the segment table.
///
/// The segment table format for streams is defined in the Cap'n Proto
/// [encoding spec](https://capnproto.org/encoding.html#serialization-over-a-stream)
///
/// Returns the segment count and first segment length, or a state if the
/// read would block.
fn parse_segment_table_first(buf: &[u8]) -> Result<(usize, usize)> {
    let segment_count = u32::from_le_bytes(buf[0..4].try_into().unwrap()).wrapping_add(1);
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

    let first_segment_len = u32::from_le_bytes(buf[4..8].try_into().unwrap());
    Ok((segment_count as usize, first_segment_len as usize))
}

/// Something that contains segments ready to be written out.
pub trait AsOutputSegments {
    fn as_output_segments<'a>(&'a self) -> OutputSegments<'a>;
}

impl<'a, M> AsOutputSegments for &'a M
where
    M: AsOutputSegments,
{
    fn as_output_segments<'b>(&'b self) -> OutputSegments<'b> {
        (*self).as_output_segments()
    }
}

impl<A> AsOutputSegments for message::Builder<A>
where
    A: message::Allocator,
{
    fn as_output_segments<'a>(&'a self) -> OutputSegments<'a> {
        self.get_segments_for_output()
    }
}

/*impl <'a, A> AsOutputSegments for &'a message::Builder<A> where A: message::Allocator {
    fn as_output_segments<'b>(&'b self) -> OutputSegments<'b> {
        self.get_segments_for_output()
    }
}*/

impl<A> AsOutputSegments for ::std::rc::Rc<message::Builder<A>>
where
    A: message::Allocator,
{
    fn as_output_segments<'a>(&'a self) -> OutputSegments<'a> {
        self.get_segments_for_output()
    }
}

/// Writes the provided message to `writer`. Does not call `flush()`.
pub async fn write_message<W, M>(mut writer: W, message: M) -> Result<()>
where
    W: AsyncWrite + Unpin,
    M: AsOutputSegments,
{
    let segments = message.as_output_segments();
    write_segment_table(&mut writer, &segments[..]).await?;
    write_segments(writer, &segments[..]).await?;
    Ok(())
}

async fn write_segment_table<W>(mut write: W, segments: &[&[u8]]) -> ::std::io::Result<()>
where
    W: AsyncWrite + Unpin,
{
    let mut buf: [u8; 8] = [0; 8];
    let segment_count = segments.len();

    // write the first Word, which contains segment_count and the 1st segment length
    buf[0..4].copy_from_slice(&(segment_count as u32 - 1).to_le_bytes());
    buf[4..8].copy_from_slice(&((segments[0].len() / 8) as u32).to_le_bytes());
    write.write_all(&buf).await?;

    if segment_count > 1 {
        if segment_count < 4 {
            for idx in 1..segment_count {
                buf[(idx - 1) * 4..idx * 4]
                    .copy_from_slice(&((segments[idx].len() / 8) as u32).to_le_bytes());
            }
            if segment_count == 2 {
                for idx in 4..8 {
                    buf[idx] = 0
                }
            }
            write.write_all(&buf).await?;
        } else {
            let mut buf = vec![0; (segment_count & !1) * 4];
            for idx in 1..segment_count {
                buf[(idx - 1) * 4..idx * 4]
                    .copy_from_slice(&((segments[idx].len() / 8) as u32).to_le_bytes());
            }
            if segment_count % 2 == 0 {
                for idx in (buf.len() - 4)..(buf.len()) {
                    buf[idx] = 0
                }
            }
            write.write_all(&buf).await?;
        }
    }
    Ok(())
}

/// Writes segments to `write`.
async fn write_segments<W>(mut write: W, segments: &[&[u8]]) -> Result<()>
where
    W: AsyncWrite + Unpin,
{
    for i in 0..segments.len() {
        write.write_all(segments[i]).await?;
    }
    Ok(())
}

#[cfg(test)]
pub mod test {
    use std::cmp;
    use std::io::{self, Read, Write};
    use std::pin::Pin;
    use std::task::{Context, Poll};

    use futures::io::Cursor;
    use futures::{AsyncRead, AsyncWrite};

    use quickcheck::{quickcheck, TestResult};

    use capnp::message::ReaderSegments;
    use capnp::{message, OutputSegments};

    use super::{read_message, read_segment_table, write_message, AsOutputSegments};

    #[test]
    fn test_read_segment_table() {
        let mut exec = futures::executor::LocalPool::new();
        let mut buf = vec![];

        buf.extend(
            [
                0, 0, 0, 0, // 1 segments
                0, 0, 0, 0,
            ] // 0 length
            .iter()
            .cloned(),
        );
        let segment_lengths = exec
            .run_until(read_segment_table(
                Cursor::new(&buf[..]),
                message::ReaderOptions::new(),
            ))
            .unwrap()
            .unwrap();
        assert_eq!(0, segment_lengths.total_words());
        assert_eq!(vec![(0, 0)], segment_lengths.to_segment_indices());
        buf.clear();

        buf.extend(
            [
                0, 0, 0, 0, // 1 segments
                1, 0, 0, 0,
            ] // 1 length
            .iter()
            .cloned(),
        );

        let segment_lengths = exec
            .run_until(read_segment_table(
                &mut Cursor::new(&buf[..]),
                message::ReaderOptions::new(),
            ))
            .unwrap()
            .unwrap();
        assert_eq!(1, segment_lengths.total_words());
        assert_eq!(vec![(0, 1)], segment_lengths.to_segment_indices());
        buf.clear();

        buf.extend(
            [
                1, 0, 0, 0, // 2 segments
                1, 0, 0, 0, // 1 length
                1, 0, 0, 0, // 1 length
                0, 0, 0, 0,
            ] // padding
            .iter()
            .cloned(),
        );
        let segment_lengths = exec
            .run_until(read_segment_table(
                &mut Cursor::new(&buf[..]),
                message::ReaderOptions::new(),
            ))
            .unwrap()
            .unwrap();
        assert_eq!(2, segment_lengths.total_words());
        assert_eq!(vec![(0, 1), (1, 2)], segment_lengths.to_segment_indices());
        buf.clear();

        buf.extend(
            [
                2, 0, 0, 0, // 3 segments
                1, 0, 0, 0, // 1 length
                1, 0, 0, 0, // 1 length
                0, 1, 0, 0,
            ] // 256 length
            .iter()
            .cloned(),
        );
        let segment_lengths = exec
            .run_until(read_segment_table(
                &mut Cursor::new(&buf[..]),
                message::ReaderOptions::new(),
            ))
            .unwrap()
            .unwrap();
        assert_eq!(258, segment_lengths.total_words());
        assert_eq!(
            vec![(0, 1), (1, 2), (2, 258)],
            segment_lengths.to_segment_indices()
        );
        buf.clear();

        buf.extend(
            [
                3, 0, 0, 0, // 4 segments
                77, 0, 0, 0, // 77 length
                23, 0, 0, 0, // 23 length
                1, 0, 0, 0, // 1 length
                99, 0, 0, 0, // 99 length
                0, 0, 0, 0,
            ] // padding
            .iter()
            .cloned(),
        );
        let segment_lengths = exec
            .run_until(read_segment_table(
                &mut Cursor::new(&buf[..]),
                message::ReaderOptions::new(),
            ))
            .unwrap()
            .unwrap();
        assert_eq!(200, segment_lengths.total_words());
        assert_eq!(
            vec![(0, 77), (77, 100), (100, 101), (101, 200)],
            segment_lengths.to_segment_indices()
        );
        buf.clear();
    }

    #[test]
    fn test_read_invalid_segment_table() {
        let mut exec = futures::executor::LocalPool::new();
        let mut buf = vec![];

        buf.extend([0, 2, 0, 0].iter().cloned()); // 513 segments
        buf.extend([0; 513 * 4].iter().cloned());
        assert!(exec
            .run_until(read_segment_table(
                Cursor::new(&buf[..]),
                message::ReaderOptions::new()
            ))
            .is_err());
        buf.clear();

        buf.extend([0, 0, 0, 0].iter().cloned()); // 1 segments
        assert!(exec
            .run_until(read_segment_table(
                Cursor::new(&buf[..]),
                message::ReaderOptions::new()
            ))
            .is_err());

        buf.clear();

        buf.extend([0, 0, 0, 0].iter().cloned()); // 1 segments
        buf.extend([0; 3].iter().cloned());
        assert!(exec
            .run_until(read_segment_table(
                Cursor::new(&buf[..]),
                message::ReaderOptions::new()
            ))
            .is_err());
        buf.clear();

        buf.extend([255, 255, 255, 255].iter().cloned()); // 0 segments
        assert!(exec
            .run_until(read_segment_table(
                Cursor::new(&buf[..]),
                message::ReaderOptions::new()
            ))
            .is_err());
        buf.clear();
    }

    fn construct_segment_table(segments: &[&[u8]]) -> Vec<u8> {
        let mut exec = futures::executor::LocalPool::new();
        let mut buf = vec![];
        exec.run_until(super::write_segment_table(&mut buf, segments))
            .unwrap();
        buf
    }

    #[test]
    fn test_construct_segment_table() {
        let segment_0: [u8; 0] = [];
        let segment_1 = [1, 0, 0, 0, 0, 0, 0, 0];
        let segment_199 = [197; 199 * 8];

        let buf = construct_segment_table(&[&segment_0]);
        assert_eq!(
            &[
                0, 0, 0, 0, // 1 segments
                0, 0, 0, 0
            ], // 0 length
            &buf[..]
        );

        let buf = construct_segment_table(&[&segment_1]);
        assert_eq!(
            &[
                0, 0, 0, 0, // 1 segments
                1, 0, 0, 0
            ], // 1 length
            &buf[..]
        );

        let buf = construct_segment_table(&[&segment_199]);
        assert_eq!(
            &[
                0, 0, 0, 0, // 1 segments
                199, 0, 0, 0
            ], // 199 length
            &buf[..]
        );

        let buf = construct_segment_table(&[&segment_0, &segment_1]);
        assert_eq!(
            &[
                1, 0, 0, 0, // 2 segments
                0, 0, 0, 0, // 0 length
                1, 0, 0, 0, // 1 length
                0, 0, 0, 0
            ], // padding
            &buf[..]
        );

        let buf = construct_segment_table(&[&segment_199, &segment_1, &segment_199, &segment_0]);
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

        let buf = construct_segment_table(&[
            &segment_199,
            &segment_1,
            &segment_199,
            &segment_0,
            &segment_1,
        ]);
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
    }

    impl AsOutputSegments for Vec<Vec<capnp::Word>> {
        fn as_output_segments<'a>(&'a self) -> OutputSegments<'a> {
            if self.len() == 0 {
                OutputSegments::SingleSegment([&[]])
            } else if self.len() == 1 {
                OutputSegments::SingleSegment([capnp::Word::words_to_bytes(&self[0][..])])
            } else {
                OutputSegments::MultiSegment(
                    self.iter()
                        .map(|segment| capnp::Word::words_to_bytes(&segment[..]))
                        .collect::<Vec<_>>(),
                )
            }
        }
    }

    /// Wraps a `Read` instance and introduces blocking.
    struct BlockingRead<R>
    where
        R: Read,
    {
        /// The wrapped reader
        read: R,

        /// Number of bytes to read before blocking
        frequency: usize,

        /// Number of bytes read since last blocking
        idx: usize,
    }

    impl<R> BlockingRead<R>
    where
        R: Read,
    {
        fn new(read: R, frequency: usize) -> BlockingRead<R> {
            BlockingRead {
                read: read,
                frequency: frequency,
                idx: 0,
            }
        }
    }

    impl<R> AsyncRead for BlockingRead<R>
    where
        R: Read + Unpin,
    {
        fn poll_read(
            mut self: Pin<&mut Self>,
            cx: &mut Context,
            buf: &mut [u8],
        ) -> Poll<io::Result<usize>> {
            if self.idx == 0 {
                self.idx = self.frequency;
                cx.waker().clone().wake();
                Poll::Pending
            } else {
                let len = cmp::min(self.idx, buf.len());
                let bytes_read = match self.read.read(&mut buf[..len]) {
                    Err(e) => return Poll::Ready(Err(e)),
                    Ok(n) => n,
                };
                self.idx -= bytes_read;
                Poll::Ready(Ok(bytes_read))
            }
        }
    }

    /// Wraps a `Write` instance and introduces blocking.
    struct BlockingWrite<W>
    where
        W: Write,
    {
        /// The wrapped writer
        writer: W,

        /// Number of bytes to write before blocking
        frequency: usize,

        /// Number of bytes written since last blocking
        idx: usize,
    }

    impl<W> BlockingWrite<W>
    where
        W: Write,
    {
        fn new(writer: W, frequency: usize) -> BlockingWrite<W> {
            BlockingWrite {
                writer: writer,
                frequency: frequency,
                idx: 0,
            }
        }
        fn into_writer(self) -> W {
            self.writer
        }
    }

    impl<W> AsyncWrite for BlockingWrite<W>
    where
        W: Write + Unpin,
    {
        fn poll_write(
            mut self: Pin<&mut Self>,
            cx: &mut Context,
            buf: &[u8],
        ) -> Poll<io::Result<usize>> {
            if self.idx == 0 {
                self.idx = self.frequency;
                cx.waker().clone().wake();
                Poll::Pending
            } else {
                let len = cmp::min(self.idx, buf.len());
                let bytes_written = match self.writer.write(&buf[..len]) {
                    Err(e) => return Poll::Ready(Err(e)),
                    Ok(n) => n,
                };
                self.idx -= bytes_written;
                Poll::Ready(Ok(bytes_written))
            }
        }
        fn poll_flush(mut self: Pin<&mut Self>, _cx: &mut Context) -> Poll<io::Result<()>> {
            Poll::Ready(self.writer.flush())
        }

        fn poll_close(self: Pin<&mut Self>, _cx: &mut Context) -> Poll<io::Result<()>> {
            Poll::Ready(Ok(()))
        }
    }

    #[test]
    fn check_round_trip_async() {
        fn round_trip(
            read_block_frequency: usize,
            write_block_frequency: usize,
            segments: Vec<Vec<capnp::Word>>,
        ) -> TestResult {
            if segments.len() == 0 || read_block_frequency == 0 || write_block_frequency == 0 {
                return TestResult::discard();
            }
            let (mut read, segments) = {
                let cursor = std::io::Cursor::new(Vec::new());
                let mut writer = BlockingWrite::new(cursor, write_block_frequency);
                futures::executor::block_on(Box::pin(write_message(&mut writer, &segments)))
                    .expect("writing");

                let mut cursor = writer.into_writer();
                cursor.set_position(0);
                (BlockingRead::new(cursor, read_block_frequency), segments)
            };

            let message =
                futures::executor::block_on(Box::pin(read_message(&mut read, Default::default())))
                    .expect("reading")
                    .unwrap();
            let message_segments = message.into_segments();

            TestResult::from_bool(segments.iter().enumerate().all(|(i, segment)| {
                capnp::Word::words_to_bytes(&segment[..])
                    == message_segments.get_segment(i as u32).unwrap()
            }))
        }

        quickcheck(round_trip as fn(usize, usize, Vec<Vec<capnp::Word>>) -> TestResult);
    }
}
