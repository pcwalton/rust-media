// Copyright 2015 The Servo Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

use platform;
use vpxdecoder;

use libc::{c_int, c_uint};

pub trait Decoder {
    fn set_headers(&mut self, headers: &Headers, width: i32, height: i32) -> Result<(),()>;
    fn decode_frame(&self, data: &[u8]) -> Result<Box<DecodedFrame>,()>;
}

pub trait Headers {
    fn h264_seq_headers<'a>(&'a self) -> Option<Vec<&'a [u8]>>;
    fn h264_pict_headers<'a>(&'a self) -> Option<Vec<&'a [u8]>>;
}

pub trait DecodedFrame {
    fn width(&self) -> c_uint;
    fn height(&self) -> c_uint;
    fn stride(&self, plane_index: usize) -> c_int;
    fn lock<'a>(&'a self) -> Box<DecodedFrameLockGuard + 'a>;
}

pub trait DecodedFrameLockGuard {
    fn pixels<'a>(&'a self, plane_index: usize) -> &'a [u8];
}

/// For codecs that require no headers, or as a placeholder.
#[derive(Copy)]
pub struct EmptyHeadersImpl;

impl Headers for EmptyHeadersImpl {
    fn h264_seq_headers<'a>(&'a self) -> Option<Vec<&'a [u8]>> {
        None
    }

    fn h264_pict_headers<'a>(&'a self) -> Option<Vec<&'a [u8]>> {
        None
    }
}

#[allow(missing_copy_implementations)]
pub struct RegisteredDecoder {
    id: [u8; 4],
    constructor: extern "Rust" fn() -> Result<Box<Decoder + 'static>,()>,
}

impl RegisteredDecoder {
    pub fn new(&self) -> Result<Box<Decoder + 'static>,()> {
        (self.constructor)()
    }
    pub fn id(&self) -> [u8; 4] {
        self.id
    }
}

pub static DECODERS: [RegisteredDecoder; 2] = [
    RegisteredDecoder {
        id: [ b'V', b'P', b'8', b'0' ],
        constructor: vpxdecoder::DecoderImpl::new,
    },
    RegisteredDecoder {
        id: [ b'a', b'v', b'c', b'1' ],
        constructor: platform::macos::videotoolbox::DecoderImpl::new,
    }
];

