// Copyright 2015 The Servo Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

use codecs::aac::AacHeaders;
use codecs::vorbis::{self, VorbisHeaders};

use libc::c_int;

#[cfg(feature="ffmpeg")]
use codecs::libavcodec;
#[cfg(target_os="macos")]
use platform;

pub trait AudioHeaders {
    fn vorbis_headers<'a>(&'a self) -> Option<&'a VorbisHeaders>;
    fn aac_headers<'a>(&'a self) -> Option<&'a AacHeaders>;
}

pub trait AudioDecoderInfo {
    fn create_decoder(self: Box<Self>) -> Box<AudioDecoder + 'static>;
}

pub trait AudioDecoder {
    fn decode(&mut self, data: &[u8]) -> Result<(),()>;
    fn decoded_samples<'a>(&'a mut self) -> Result<Box<DecodedAudioSamples + 'a>,()>;
    fn acknowledge(&mut self, sample_count: c_int);
}

pub trait DecodedAudioSamples {
    fn samples<'a>(&'a self, channel: i32) -> Option<&'a [f32]>;
}

/// For codecs that require no headers, or as a placeholder.
#[derive(Copy, Clone)]
pub struct EmptyAudioHeadersImpl;

impl AudioHeaders for EmptyAudioHeadersImpl {
    fn vorbis_headers<'a>(&'a self) -> Option<&'a VorbisHeaders> {
        None
    }
    fn aac_headers<'a>(&'a self) -> Option<&'a AacHeaders> {
        None
    }
}

#[allow(missing_copy_implementations)]
pub struct RegisteredAudioDecoder {
    pub id: [u8; 4],
    pub constructor: extern "Rust" fn(headers: &AudioHeaders, sample_rate: f64, channels: u16)
                                      -> Box<AudioDecoderInfo + 'static>,
}

impl RegisteredAudioDecoder {
    pub fn get(codec_id: &[u8]) -> Result<&'static RegisteredAudioDecoder,()> {
        for decoder in AUDIO_DECODERS.iter() {
            if decoder.id == codec_id {
                return Ok(decoder)
            }
        }
        Err(())
    }

    pub fn new(&self, headers: &AudioHeaders, sample_rate: f64, channels: u16)
               -> Box<AudioDecoderInfo + 'static> {
        (self.constructor)(headers, sample_rate, channels)
    }

    pub fn id(&self) -> [u8; 4] {
        self.id
    }
}

#[cfg(all(target_os="macos", feature="ffmpeg"))]
pub static AUDIO_DECODERS: [RegisteredAudioDecoder; 3] = [
    vorbis::AUDIO_DECODER,
    libavcodec::AUDIO_DECODER,
    platform::macos::audiounit::AUDIO_DECODER,
];

#[cfg(all(target_os="macos", not(feature="ffmpeg")))]
pub static AUDIO_DECODERS: [RegisteredAudioDecoder; 2] = [
    vorbis::AUDIO_DECODER,
    platform::macos::audiounit::AUDIO_DECODER,
];

#[cfg(all(not(target_os="macos"), feature="ffmpeg"))]
pub static AUDIO_DECODERS: [RegisteredAudioDecoder; 2] = [
    vorbis::AUDIO_DECODER,
    libavcodec::AUDIO_DECODER,
];

#[cfg(all(not(target_os="macos"), not(feature="ffmpeg")))]
pub static AUDIO_DECODERS: [RegisteredAudioDecoder; 1] = [
    vorbis::AUDIO_DECODER,
];

