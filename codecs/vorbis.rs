// Copyright 2015 The Servo Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

use audiodecoder;
use codecs::aac::AacHeaders;
use containers::ogg::Packet;

use libc::{c_float, c_int};
use std::mem;
use std::ptr;
use std::slice;
use std::marker::PhantomData;

pub struct VorbisInfo {
    info: Box<ffi::vorbis_info>,
}

impl Drop for VorbisInfo {
    fn drop(&mut self) {
        unsafe {
            ffi::vorbis_info_clear(&mut *self.info)
        }
    }
}

impl VorbisInfo {
    pub fn new() -> VorbisInfo {
        unsafe {
            let mut info = Box::new(mem::uninitialized());
            ffi::vorbis_info_init(&mut *info);
            VorbisInfo {
                info: info,
            }
        }
    }

    pub fn header_in(&mut self, comment: &mut VorbisComment, packet: &mut Packet)
                     -> Result<(),c_int> {
        let err = unsafe {
            ffi::vorbis_synthesis_headerin(&mut *self.info,
                                           &mut comment.comment,
                                           packet.raw_packet())
        };
        if err >= 0 {
            Ok(())
        } else {
            Err(err)
        }
    }
}

pub struct VorbisComment {
    comment: ffi::vorbis_comment,
}

impl Drop for VorbisComment {
    fn drop(&mut self) {
        unsafe {
            ffi::vorbis_comment_clear(&mut self.comment)
        }
    }
}

impl VorbisComment {
    pub fn new() -> VorbisComment {
        unsafe {
            let mut comment = mem::uninitialized();
            ffi::vorbis_comment_init(&mut comment);
            VorbisComment {
                comment: comment,
            }
        }
    }
}

#[allow(dead_code)]
pub struct VorbisDspState {
    state: ffi::vorbis_dsp_state,

    // This field is unused, but the Vorbis DSP state above may keep pointers into it, so it's
    // important that it stay alive!
    info: VorbisInfo,
}

impl Drop for VorbisDspState {
    fn drop(&mut self) {
        unsafe {
            ffi::vorbis_dsp_clear(&mut self.state)
        }
    }
}

impl VorbisDspState {
    pub fn new(mut info: VorbisInfo) -> Result<VorbisDspState,c_int> {
        unsafe {
            let mut state = mem::uninitialized();
            let err = ffi::vorbis_synthesis_init(&mut state, &mut *info.info);
            if err < 0 {
                println!("vorbis DSP init failed: {}", err);
                return Err(err)
            }
            Ok(VorbisDspState {
                state: state,
                info: info,
            })
        }
    }

    pub fn pcm_out<'b>(&'b mut self) -> Result<Pcm<'b>,c_int> {
        let mut pcm = ptr::null_mut();
        let result = unsafe {
            ffi::vorbis_synthesis_pcmout(&mut self.state, &mut pcm)
        };
        if result < 0 {
            return Err(result)
        }
        Ok(Pcm {
            pcm: pcm,
            channels: unsafe {
                (*self.state.vi).channels
            },
            samples: result,
            phantom: PhantomData,
        })
    }

    pub fn read(&mut self, samples: c_int) -> Result<(),c_int> {
        let err = unsafe {
            ffi::vorbis_synthesis_read(&mut self.state, samples)
        };
        if err >= 0 {
            Ok(())
        } else {
            Err(err)
        }
    }
}

pub struct VorbisBlock<'a> {
    block: ffi::vorbis_block,
    state: &'a mut VorbisDspState,
}

impl<'a> Drop for VorbisBlock<'a> {
    fn drop(&mut self) {
        unsafe {
            ffi::vorbis_block_clear(&mut self.block)
        }
    }
}

impl<'a> VorbisBlock<'a> {
    pub fn new<'b>(state: &'b mut VorbisDspState) -> Result<VorbisBlock<'b>,c_int> {
        unsafe {
            let mut block = mem::uninitialized();
            let err = ffi::vorbis_block_init(&mut state.state, &mut block);
            if err < 0 {
                return Err(err)
            }
            Ok(VorbisBlock {
                block: block,
                state: state,
            })
        }
    }

    pub fn synthesis(&mut self, packet: &mut Packet) -> Result<(),c_int> {
        let err = unsafe {
            ffi::vorbis_synthesis(&mut self.block, packet.raw_packet())
        };
        if err >= 0 {
            Ok(())
        } else {
            Err(err)
        }
    }

    pub fn block_in(&mut self) -> Result<(),c_int> {
        let err = unsafe {
            ffi::vorbis_synthesis_blockin(&mut self.state.state, &mut self.block)
        };
        if err >= 0 {
            Ok(())
        } else {
            Err(err)
        }
    }

    pub fn dsp_state(&'a mut self) -> &'a mut VorbisDspState {
        self.state
    }
}

pub struct Pcm<'a> {
    pcm: *mut *mut c_float,
    channels: c_int,
    samples: c_int,
    phantom: PhantomData<&'a u8>,
}

impl<'a> Pcm<'a> {
    pub fn samples(&self, channel: c_int) -> &'a [c_float] {
        assert!(channel < self.channels);
        if self.pcm.is_null() {
            return &[]
        }
        unsafe {
            let buffer = (*self.pcm).offset(channel as isize);
            mem::transmute::<&[c_float],
                             &'a [c_float]>(slice::from_raw_parts_mut(buffer,
                                                                    self.samples as usize))
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
    fn aac_headers<'a>(&'a self) -> Option<&'a AacHeaders> {
        None
    }
}

struct AudioDecoderInfoImpl {
    info: VorbisInfo,
}

impl AudioDecoderInfoImpl {
    pub fn new(headers: &audiodecoder::AudioHeaders, _: f64, _: u16)
               -> Box<audiodecoder::AudioDecoderInfo + 'static> {
        let mut info = VorbisInfo::new();
        let mut comment = VorbisComment::new();
        let headers = headers.vorbis_headers().unwrap();
        info.header_in(&mut comment, &mut Packet::new(headers.id(), 0)).unwrap();
        info.header_in(&mut comment, &mut Packet::new(headers.comment(), 1)).unwrap();
        info.header_in(&mut comment, &mut Packet::new(headers.setup(), 2)).unwrap();
        Box::new(AudioDecoderInfoImpl {
            info: info,
        }) as Box<audiodecoder::AudioDecoderInfo + 'static>
    }
}

impl audiodecoder::AudioDecoderInfo for AudioDecoderInfoImpl {
    fn create_decoder(self: Box<AudioDecoderInfoImpl>)
                      -> Box<audiodecoder::AudioDecoder + 'static> {
        Box::new(AudioDecoderImpl {
            state: VorbisDspState::new(self.info).unwrap(),
            packet_index: 3,
        }) as Box<audiodecoder::AudioDecoder + 'static>
    }
}

struct AudioDecoderImpl {
    state: VorbisDspState,
    packet_index: i64,
}

impl audiodecoder::AudioDecoder for AudioDecoderImpl {
    fn decode(&mut self, data: &[u8]) -> Result<(),()> {
        let mut block = VorbisBlock::new(&mut self.state).unwrap();
        let result = block.synthesis(&mut Packet::new(data, self.packet_index));
        self.packet_index += 1;
        if result.is_err() {
            return Err(())
        }
        match block.block_in() {
            Ok(_) => Ok(()),
            Err(_) => Err(()),
        }
    }

    fn decoded_samples<'b>(&'b mut self)
                           -> Result<Box<audiodecoder::DecodedAudioSamples + 'b>,()> {
        match self.state.pcm_out() {
            Ok(pcm) => {
                Ok(Box::new(DecodedAudioSamplesImpl {
                    pcm: pcm,
                }) as Box<audiodecoder::DecodedAudioSamples + 'b>)
            }
            Err(_) => Err(()),
        }
    }

    fn acknowledge(&mut self, sample_count: c_int) {
        self.state.read(sample_count).unwrap();
    }
}

struct DecodedAudioSamplesImpl<'a> {
    pcm: Pcm<'a>,
}

impl<'a> audiodecoder::DecodedAudioSamples for DecodedAudioSamplesImpl<'a> {
    fn samples<'b>(&'b self, channel: i32) -> Option<&'b [f32]> {
        Some(self.pcm.samples(channel))
    }
}

pub const AUDIO_DECODER: audiodecoder::RegisteredAudioDecoder =
    audiodecoder::RegisteredAudioDecoder {
        id: [ b'v', b'o', b'r', b'b' ],
        constructor: AudioDecoderInfoImpl::new,
    };

#[allow(missing_copy_implementations)]
#[allow(non_snake_case)]
pub mod ffi {
    use containers::ogg::ffi::ogg_packet;

    use libc::{c_char, c_float, c_int, c_long, c_uchar, c_void};

    #[repr(C)]
    pub struct alloc_chain;

    #[repr(C)]
    pub struct vorbis_info {
        pub version: c_int,
        pub channels: c_int,
        pub rate: c_long,
        pub bitrate_upper: c_long,
        pub bitrate_nominal: c_long,
        pub bitrate_lower: c_long,
        pub bitrate_window: c_long,
        pub codec_setup: *mut c_void,
    }

    #[repr(C)]
    pub struct vorbis_comment {
        pub user_comments: *mut *mut c_char,
        pub comment_lengths: *mut c_int,
        pub comments: c_int,
        pub vendor: *mut c_char,
    }

    #[repr(C)]
    pub struct vorbis_dsp_state {
        pub analysisp: c_int,
        pub vi: *mut vorbis_info,
        pub pcm: *mut *mut c_float,
        pub pcmret: *mut *mut c_float,
        pub pcm_storage: c_int,
        pub pcm_current: c_int,
        pub pcm_returned: c_int,
        pub preextrapolate: c_int,
        pub eofflag: c_int,
        pub lW: c_long,
        pub W: c_long,
        pub nW: c_long,
        pub centerW: c_long,
        pub granulepos: i64,
        pub sequence: i64,
        pub glue_bits: i64,
        pub time_bits: i64,
        pub floor_bits: i64,
        pub res_bits: i64,
        pub backend_state: *mut c_void,
    }

    #[repr(C)]
    pub struct vorbis_block {
        pub pcm: *mut *mut c_float,
        pub opb: oggpack_buffer,
        pub lW: c_long,
        pub W: c_long,
        pub nW: c_long,
        pub pcmend: c_int,
        pub mode: c_int,
        pub eofflag: c_int,
        pub granulepos: i64,
        pub sequence: i64,
        pub vd: *mut vorbis_dsp_state,
        pub localstore: *mut c_void,
        pub localtop: c_long,
        pub localalloc: c_long,
        pub totaluse: c_long,
        pub reap: *mut alloc_chain,
        pub glue_bits: c_long,
        pub time_bits: c_long,
        pub floor_bits: c_long,
        pub res_bits: c_long,
        pub internal: *mut c_void,
    }

    #[repr(C)]
    pub struct oggpack_buffer {
        pub endbyte: c_long,
        pub endbit: c_int,
        pub buffer: *mut c_uchar,
        pub ptr: *mut c_uchar,
        pub storage: c_long,
    }

    #[link(name="rustvorbis")]
    extern {
        pub fn vorbis_info_init(vi: *mut vorbis_info);
        pub fn vorbis_info_clear(vi: *mut vorbis_info);
        pub fn vorbis_comment_init(vc: *mut vorbis_comment);
        pub fn vorbis_comment_clear(v: *mut vorbis_comment);
        pub fn vorbis_block_init(v: *mut vorbis_dsp_state, vb: *mut vorbis_block) -> c_int;
        pub fn vorbis_block_clear(vb: *mut vorbis_block);
        pub fn vorbis_dsp_clear(v: *mut vorbis_dsp_state);
        pub fn vorbis_synthesis_headerin(vi: *mut vorbis_info,
                                         vc: *mut vorbis_comment,
                                         op: *mut ogg_packet)
                                         -> c_int;
        pub fn vorbis_synthesis_init(v: *mut vorbis_dsp_state, vi: *mut vorbis_info)
                                     -> c_int;
        pub fn vorbis_synthesis(vb: *mut vorbis_block, op: *mut ogg_packet) -> c_int;
        pub fn vorbis_synthesis_blockin(v: *mut vorbis_dsp_state, vb: *mut vorbis_block) -> c_int;
        pub fn vorbis_synthesis_pcmout(v: *mut vorbis_dsp_state, pcm: *mut *mut *mut c_float)
                                       -> c_int;
        pub fn vorbis_synthesis_read(v: *mut vorbis_dsp_state, samples: c_int) -> c_int;
    }

    #[link(name="rustogg")]
    extern {
        pub fn ogg_packet_clear(op: *mut ogg_packet);
    }
}

