// Copyright 2015 The Servo Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

use std::fs::File;
use std::io::{Read, Seek};

pub trait StreamReader : Read + Seek {
    /// Returns the number of bytes available in this stream.
    fn available_size(&self) -> u64;
    /// Returns the total number of octets in this stream, including those that are not yet
    /// available.
    fn total_size(&self) -> u64;
}

/// TODO(pcwalton): Should probably buffer reads, maybe by implementing on BufferedReader<File> or
/// something.
impl StreamReader for File {
    fn available_size(&self) -> u64 {
        self.total_size()
    }
    fn total_size(&self) -> u64 {
        self.metadata().unwrap().len()
    }
}

