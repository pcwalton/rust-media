//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

use codecs::vpx;
use containers::gif;
use pixelformat::PixelFormat;
use timing::Timestamp;

use libc::{c_int, c_uint};

#[cfg(feature="ffmpeg")]
use codecs::libavcodec;
#[cfg(target_os="macos")]
use platform;

pub trait VideoDecoder {
    fn decode_frame(&self, data: &[u8], presentation_time: &Timestamp)
                    -> Result<Box<DecodedVideoFrame + 'static>,()>;
}

pub trait VideoHeaders {
    fn h264_seq_headers<'a>(&'a self) -> Option<Vec<&'a [u8]>>;
    fn h264_pict_headers<'a>(&'a self) -> Option<Vec<&'a [u8]>>;
}

pub trait DecodedVideoFrame {
    fn width(&self) -> c_uint;
    fn height(&self) -> c_uint;
    fn stride(&self, plane_index: usize) -> c_int;
    fn presentation_time(&self) -> Timestamp;
    fn pixel_format<'a>(&'a self) -> PixelFormat<'a>;
    fn lock<'a>(&'a self) -> Box<DecodedVideoFrameLockGuard + 'a>;
}

pub trait DecodedVideoFrameLockGuard {
    fn pixels<'a>(&'a self, plane_index: usize) -> &'a [u8];
}

/// For codecs that require no headers, or as a placeholder.
#[derive(Copy, Clone)]
pub struct EmptyVideoHeadersImpl;

impl VideoHeaders for EmptyVideoHeadersImpl {
    fn h264_seq_headers<'a>(&'a self) -> Option<Vec<&'a [u8]>> {
        None
    }

    fn h264_pict_headers<'a>(&'a self) -> Option<Vec<&'a [u8]>> {
        None
    }
}

#[allow(missing_copy_implementations)]
pub struct RegisteredVideoDecoder {
    pub id: [u8; 4],
    pub constructor: extern "Rust" fn(headers: &VideoHeaders, width: i32, height: i32)
                                      -> Result<Box<VideoDecoder + 'static>,()>,
}

impl RegisteredVideoDecoder {
    pub fn get(codec_id: &[u8]) -> Result<&'static RegisteredVideoDecoder,()> {
        for decoder in VIDEO_DECODERS.iter() {
            if decoder.id == codec_id {
                return Ok(decoder)
            }
        }
        Err(())
    }

    pub fn new(&self, headers: &VideoHeaders, width: i32, height: i32)
               -> Result<Box<VideoDecoder + 'static>,()> {
        (self.constructor)(headers, width, height)
    }

    pub fn id(&self) -> [u8; 4] {
        self.id
    }
}

// FIXME(pcwalton): Combinatorial explosion imminent. :( Can we do something clever with macros?

#[cfg(all(target_os="macos", feature="ffmpeg"))]
pub static VIDEO_DECODERS: [RegisteredVideoDecoder; 4] = [
    vpx::VIDEO_DECODER,
    gif::VIDEO_DECODER,
    libavcodec::VIDEO_DECODER,
    platform::macos::videotoolbox::VIDEO_DECODER,
];

#[cfg(all(target_os="macos", not(feature="ffmpeg")))]
pub static VIDEO_DECODERS: [RegisteredVideoDecoder; 3] = [
    vpx::VIDEO_DECODER,
    gif::VIDEO_DECODER,
    platform::macos::videotoolbox::VIDEO_DECODER,
];

#[cfg(all(not(target_os="macos"), feature="ffmpeg"))]
pub static VIDEO_DECODERS: [RegisteredVideoDecoder; 3] = [
    vpx::VIDEO_DECODER,
    gif::VIDEO_DECODER,
    libavcodec::VIDEO_DECODER,
];

#[cfg(all(not(target_os="macos"), not(feature="ffmpeg")))]
pub static VIDEO_DECODERS: [RegisteredVideoDecoder; 2] = [
    vpx::VIDEO_DECODER,
    gif::VIDEO_DECODER,
];

