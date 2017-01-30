// Copyright 2015 The Servo Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

use audiodecoder;

use libc::{c_int};

use lewton::header::{self, IdentHeader, CommentHeader, SetupHeader, HeaderReadError};
use lewton::audio::{self, PreviousWindowRight};

pub struct DecodedHeaders {
    ident: IdentHeader,
    #[allow(dead_code)]
    comment: CommentHeader,
    setup: SetupHeader,
}

impl DecodedHeaders {
    pub fn from_encoded(headers: &audiodecoder::AudioHeaders) -> Result<Self, HeaderReadError> {
        if let Some(hdrs) = headers.vorbis_headers() {
            let ident = try!(header::read_header_ident(hdrs.id()));
            let setup = try!(header::read_header_setup(hdrs.setup(),
                ident.audio_channels,
                (ident.blocksize_0, ident.blocksize_1)));
            Ok(DecodedHeaders {
                ident: ident,
                comment: try!(header::read_header_comment(hdrs.comment())),
                setup: setup,
            })
        } else {
            Err(HeaderReadError::NotVorbisHeader)
        }
    }
}

// Implementation of the abstract `AudioDecoder` interface

pub struct VorbisHeaders {
    pub data: Vec<u8>,
    pub id_size: usize,
    pub comment_size: usize,
}

impl VorbisHeaders {
    pub fn id<'a>(&'a self) -> &'a [u8] {
        &self.data[0..self.id_size]
    }
    pub fn comment<'a>(&'a self) -> &'a [u8] {
        &self.data[self.id_size..self.id_size + self.comment_size]
    }
    pub fn setup<'a>(&'a self) -> &'a [u8] {
        &self.data[self.id_size + self.comment_size..]
    }
}

impl audiodecoder::AudioHeaders for VorbisHeaders {
    fn vorbis_headers<'a>(&'a self) -> Option<&'a VorbisHeaders> {
        Some(self)
    }
}

struct AudioDecoderInfoImpl {
    headers: DecodedHeaders,
}

impl AudioDecoderInfoImpl {
    pub fn new(headers: &audiodecoder::AudioHeaders, _: f64, _: u16)
               -> Box<audiodecoder::AudioDecoderInfo + 'static> {
        let decoded_headers = DecodedHeaders::from_encoded(headers).unwrap();
        Box::new(AudioDecoderInfoImpl {
            headers: decoded_headers,
        }) as Box<audiodecoder::AudioDecoderInfo + 'static>
    }
}

impl audiodecoder::AudioDecoderInfo for AudioDecoderInfoImpl {
    fn create_decoder(self: Box<AudioDecoderInfoImpl>)
                      -> Box<audiodecoder::AudioDecoder + 'static> {
        Box::new(AudioDecoderImpl {
            headers: self.headers,
            pwr: PreviousWindowRight::new(),
            packet_queue: Vec::new(),
        }) as Box<audiodecoder::AudioDecoder + 'static>
    }
}

struct AudioDecoderImpl {
    headers: DecodedHeaders,
    pwr: PreviousWindowRight,
    packet_queue: Vec<Vec<Vec<i16>>>,
}

impl audiodecoder::AudioDecoder for AudioDecoderImpl {
    fn decode(&mut self, data: &[u8]) -> Result<(),()> {
        match audio::read_audio_packet(&self.headers.ident, &self.headers.setup,
        data, &mut self.pwr) {
            Ok(pck) => self.packet_queue.push(pck),
            Err(_) => return Err(()),
        }
        Ok(())
    }

    fn decoded_samples<'b>(&'b mut self)
                           -> Result<Box<audiodecoder::DecodedAudioSamples + 'b>,()> {
        if self.packet_queue.len() == 0 {
            return Err(())
        }
        Ok(Box::new(DecodedAudioSamplesImpl {
                pck_samples: self.packet_queue.remove(0).iter()
                    .map(|c| c.iter().map(|s| *s as f32 / 32768.).collect())
                    .collect(),
            }) as Box<audiodecoder::DecodedAudioSamples + 'b>)
    }

    fn acknowledge(&mut self, _: c_int) {
        // Nothing to do
    }
}

struct DecodedAudioSamplesImpl {
    pck_samples: Vec<Vec<f32>>,
}

impl audiodecoder::DecodedAudioSamples for DecodedAudioSamplesImpl {
    fn samples<'b>(&'b self, channel: i32) -> Option<&'b [f32]> {
        self.pck_samples.get(channel as usize).map(|s| s.as_slice())
    }
}

pub const AUDIO_DECODER: audiodecoder::RegisteredAudioDecoder =
    audiodecoder::RegisteredAudioDecoder {
        id: [ b'v', b'o', b'r', b'b' ],
        constructor: AudioDecoderInfoImpl::new,
    };

