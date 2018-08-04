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

use std::io;

/// Reads into `buf` until it is full. Returns an error if EOF is encountered first.
pub fn read_exact<R>(read: &mut R, buf: &mut [u8]) -> io::Result<()>
where
    R: io::Read,
{
    let mut pos = 0;
    let len = buf.len();
    while pos < len {
        let buf1 = &mut buf[pos..];
        match read.read(buf1) {
            Ok(n) => {
                pos += n;
                if n == 0 {
                    return Err(io::Error::new(io::ErrorKind::Other, "Premature EOF"));
                }
            }
            Err(e) => {
                if e.kind() != io::ErrorKind::Interrupted {
                    return Err(e);
                }
                // Retry if we were interrupted.
            }
        }
    }
    Ok(())
}
