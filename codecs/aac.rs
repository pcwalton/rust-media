// Copyright 2015 The Servo Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

use audiodecoder::AudioHeaders;
use codecs::vorbis::VorbisHeaders;

pub struct AacHeaders {
    pub esds_chunk: Vec<u8>,
}

impl AudioHeaders for AacHeaders {
    fn vorbis_headers<'a>(&'a self) -> Option<&'a VorbisHeaders> {
        None
    }
    fn aac_headers<'a>(&'a self) -> Option<&'a AacHeaders> {
        Some(self)
    }
}

