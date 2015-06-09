// Copyright 2015 The Servo Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

//! Codec support via `libavcodec` from FFmpeg.

use audiodecoder;
use codecs::h264;
use pixelformat::PixelFormat;
use timing::Timestamp;
use videodecoder;

use libc::{c_double, c_int, c_uint, c_void};
use std::any::Any;
use std::cell::RefCell;
use std::ffi::CString;
use std::i32;
use std::mem;
use std::ptr;
use std::slice;
use std::marker::PhantomData;

pub type AvCodecId = ffi::AVCodecID;

pub const AV_CODEC_ID_H264: AvCodecId = 28;
pub const AV_CODEC_ID_AAC: AvCodecId = 0x15000 + 2;

pub const FF_INPUT_BUFFER_PADDING_SIZE: usize = 32;

pub fn init() {
    unsafe {
        ffi::avcodec_register_all()
    }
}

pub fn version() -> c_uint {
    unsafe {
        ffi::avcodec_version()
    }
}

#[allow(missing_copy_implementations)]
pub struct AvCodec {
    codec: *mut ffi::AVCodec,
}

impl AvCodec {
    pub fn find_decoder(codec_id: AvCodecId) -> Result<AvCodec,()> {
        let codec = unsafe {
            ffi::avcodec_find_decoder(codec_id)
        };
        if !codec.is_null() {
            Ok(AvCodec {
                codec: codec,
            })
        } else {
            Err(())
        }
    }
}

pub struct AvCodecContext {
    context: ffi::EitherAVCodecContext,
    extra_data: Option<Vec<u8>>,
}

impl AvCodecContext {
    pub fn new(codec: &AvCodec) -> AvCodecContext {
        unsafe {
            let context = ffi::avcodec_alloc_context3(codec.codec);
            AvCodecContext {
                context: if version() < 0x380d64 {
                    ffi::EitherAVCodecContext::V362300(context as *mut ffi::AVCodecContextV362300)
                } else {
                    ffi::EitherAVCodecContext::V380D64(context as *mut ffi::AVCodecContextV380D64)
                },
                extra_data: None,
            }
        }
    }

    pub fn open(&self, codec: &AvCodec, options: AvDictionary) -> (Result<(),()>, AvDictionary) {
        // The memory management that `libavcodec` expects around the `options` argument is really
        // weird.
        let mut options_not_found = options.dictionary;
        let result;
        unsafe {
            result = ffi::avcodec_open2(self.context.ptr(), codec.codec, &mut options_not_found);
            mem::forget(options);
        }
        let options_not_found = AvDictionary {
            dictionary: options_not_found,
        };
        if result == 0 {
            (Ok(()), options_not_found)
        } else {
            (Err(()), options_not_found)
        }
    }

    pub fn set_extra_data(&mut self, mut extra_data: Vec<u8>) {
        assert!(extra_data.len() <= (i32::MAX as usize));
        unsafe {
            match self.context {
                ffi::EitherAVCodecContext::V362300(context) => {
                    (*context).extradata = extra_data.as_mut_ptr();
                    (*context).extradata_size = extra_data.len() as i32;
                }
                ffi::EitherAVCodecContext::V380D64(context) => {
                    (*context).extradata = extra_data.as_mut_ptr();
                    (*context).extradata_size = extra_data.len() as i32;
                }
            }
        }
        self.extra_data = Some(extra_data);
    }

    pub fn set_get_buffer_callback(&mut self, callback: Box<FnMut(&AvFrame)>) {
        unsafe {
            match self.context {
                ffi::EitherAVCodecContext::V362300(context) => {
                    (*context).opaque = mem::transmute::<_,*mut c_void>(Box::new(callback));
                    (*context).get_buffer = get_buffer;
                }
                ffi::EitherAVCodecContext::V380D64(context) => {
                    (*context).opaque = mem::transmute::<_,*mut c_void>(Box::new(callback));
                    (*context).get_buffer = get_buffer;
                }
            }
        }
    }

    pub fn decode_video(&self, picture: &AvFrame, packet: &mut AvPacket) -> Result<bool,()> {
        let mut got_picture = 0;
        let result = unsafe {
            ffi::avcodec_decode_video2(self.context.ptr(),
                                       picture.frame,
                                       &mut got_picture,
                                       packet.packet.ptr())
        };
        if result >= 0 && got_picture != 0 {
            Ok(result > 0)
        } else {
            Err(())
        }
    }

    pub fn decode_audio(&self, frame: &AvFrame, packet: &mut AvPacket) -> Result<c_int,()> {
        let mut got_frame = 0;
        let result = unsafe {
            ffi::avcodec_decode_audio4(self.context.ptr(),
                                       frame.frame,
                                       &mut got_frame,
                                       packet.packet.ptr())
        };
        if result >= 0 && got_frame != 0 {
            Ok(result)
        } else {
            Err(())
        }
    }

    pub fn set_pkt_timebase(&self, timebase: &ffi::AVRational) {
        unsafe {
            ffi::av_codec_set_pkt_timebase(self.context.ptr(), *timebase)
        }
    }

    pub fn get_double_opt(&self, name: &[u8]) -> Result<c_double,c_int> {
        let name = CString::new(name).unwrap();
        let mut out_val = 0.0;
        let result = unsafe {
            ffi::av_opt_get_double(self.context.ptr() as *mut c_void,
                                   name.as_ptr(),
                                   0,
                                   &mut out_val)
        };
        if result >= 0 {
            Ok(out_val)
        } else {
            Err(result)
        }
    }

    pub fn get_q_opt(&self, name: &[u8]) -> Result<ffi::AVRational,c_int> {
        let name = CString::new(name).unwrap();
        let mut out_val = ffi::AVRational {
            num: 0,
            den: 0,
        };
        let result = unsafe {
            ffi::av_opt_get_q(self.context.ptr() as *mut c_void, name.as_ptr(), 0, &mut out_val)
        };
        if result >= 0 {
            Ok(out_val)
        } else {
            Err(result)
        }
    }

    pub fn channels(&self) -> i32 {
        unsafe {
            match self.context {
                ffi::EitherAVCodecContext::V362300(context) => (*context).channels,
                ffi::EitherAVCodecContext::V380D64(context) => (*context).channels,
            }
        }
    }
}

extern "C" fn get_buffer(context: *mut ffi::AVCodecContext, frame: *mut ffi::AVFrame) -> c_int {
    let result = unsafe {
        ffi::avcodec_default_get_buffer(context, frame)
    };
    let frame = AvFrame {
        frame: frame,
    };
    unsafe {
        let mut callback = if version() < 0x380d64 {
            let context = mem::transmute::<_,*mut ffi::AVCodecContextV362300>(context);
            mem::transmute::<*mut c_void,Box<Box<FnMut(&AvFrame)>>>((*context).opaque)
        } else {
            let context = mem::transmute::<_,*mut ffi::AVCodecContextV380D64>(context);
            mem::transmute::<*mut c_void,Box<Box<FnMut(&AvFrame)>>>((*context).opaque)
        };
        (*callback)(&frame);
        mem::forget(frame);
    }
    result
}

pub struct AvFrame {
    frame: *mut ffi::AVFrame,
}

impl Drop for AvFrame {
    fn drop(&mut self) {
        unsafe {
            ffi::avcodec_free_frame(&mut self.frame)
        }
    }
}

impl AvFrame {
    pub fn new() -> AvFrame {
        unsafe {
            let frame = AvFrame {
                frame: ffi::avcodec_alloc_frame(),
            };
            (*frame.frame).opaque = ptr::null_mut();
            frame
        }
    }

    pub fn width(&self) -> c_int {
        unsafe {
            (*self.frame).width
        }
    }

    pub fn height(&self) -> c_int {
        unsafe {
            (*self.frame).height
        }
    }

    pub fn linesize(&self, plane_index: usize) -> c_int {
        unsafe {
            (*self.frame).linesize[plane_index]
        }
    }

    pub fn sample_count(&self) -> c_int {
        unsafe {
            (*self.frame).nb_samples
        }
    }

    pub fn format(&self) -> c_int {
        unsafe {
            (*self.frame).format
        }
    }

    pub fn user_data<'a>(&'a self) -> &'a Any {
        unsafe {
            assert!(!(*self.frame).opaque.is_null());
            let user_data = mem::transmute::<_,&Box<Box<Any>>>(&(*self.frame).opaque);
            &***user_data
        }
    }

    pub fn set_user_data(&self, user_data: Box<Any>) {
        unsafe {
            if !(*self.frame).opaque.is_null() {
                drop(mem::transmute::<_,Box<Box<Any>>>((*self.frame).opaque));
            }
            (*self.frame).opaque = mem::transmute::<Box<Box<Any>>,*mut c_void>(Box::new(user_data))
        }
    }

    pub fn pts(&self) -> i64 {
        unsafe {
            (*self.frame).pts
        }
    }

    pub fn pkt_pts(&self) -> i64 {
        unsafe {
            (*self.frame).pkt_pts
        }
    }

    pub fn pkt_dts(&self) -> i64 {
        unsafe {
            (*self.frame).pkt_dts
        }
    }

    pub fn video_data<'a>(&'a self, plane_index: usize) -> &'a [u8] {
        let len = self.linesize(plane_index) * self.height();
        unsafe {
            slice::from_raw_parts_mut((*self.frame).data[plane_index], len as usize)
        }
    }

    pub fn audio_data<'a>(&'a self, channel: usize, channels: i32) -> &'a [u8] {
        let len = samples::buffer_size(channels,
                                       self.sample_count(),
                                       self.format(),
                                       true).unwrap()
                                            .linesize;
        unsafe {
            slice::from_raw_parts_mut((*self.frame).data[channel], len as usize)
        }
    }
}

pub struct AvPacket<'a> {
    packet: ffi::EitherAVPacket,
    phantom: PhantomData<&'a u8>,
}

impl<'a> AvPacket<'a> {
    /// NB: `FF_INPUT_BUFFER_PADDING_SIZE` bytes of data at the end of the slice are ignored!
    pub fn new<'b>(data: &'b mut [u8]) -> AvPacket<'b> {
        // Guard against segfaults per the documentation by setting the padding to zero.
        assert!(data.len() <= (i32::MAX as usize));
        assert!(data.len() >= FF_INPUT_BUFFER_PADDING_SIZE);
        for i in (data.len() - FF_INPUT_BUFFER_PADDING_SIZE) .. data.len() {
            data[i] = 0
        }

        let mut packet;
        unsafe {
            packet = if version() < 0x380d64 {
                ffi::EitherAVPacket::V362300(mem::uninitialized())
            } else {
                ffi::EitherAVPacket::V380D64(mem::uninitialized())
            };
            match packet {
                ffi::EitherAVPacket::V362300(ref mut packet) => {
                    ffi::av_init_packet(packet as *mut ffi::AVPacketV362300 as *mut ffi::AVPacket);
                    packet.size = (data.len() - FF_INPUT_BUFFER_PADDING_SIZE) as i32;
                    packet.data = data.as_ptr() as *mut u8;
                }
                ffi::EitherAVPacket::V380D64(ref mut packet) => {
                    ffi::av_init_packet(packet as *mut ffi::AVPacketV380D64 as *mut ffi::AVPacket);
                    packet.size = (data.len() - FF_INPUT_BUFFER_PADDING_SIZE) as i32;
                    packet.data = data.as_ptr() as *mut u8;
                }
            }
        }

        AvPacket {
            packet: packet,
            phantom: PhantomData,
        }
    }
}

pub struct AvDictionary {
    dictionary: *mut ffi::AVDictionary,
}

impl Drop for AvDictionary {
    fn drop(&mut self) {
        unsafe {
            ffi::av_dict_free(&mut self.dictionary)
        }
    }
}

impl AvDictionary {
    pub fn new() -> AvDictionary {
        AvDictionary {
            dictionary: ptr::null_mut(),
        }
    }

    pub fn set(&mut self, key: &str, value: &str) {
        unsafe {
            let key = CString::new(key.as_bytes()).unwrap();
            let value = CString::new(value.as_bytes()).unwrap();
            assert!(ffi::av_dict_set(&mut self.dictionary, key.as_ptr(), value.as_ptr(), 0) >= 0);
        }
    }
}

pub mod samples {
    use codecs::libavcodec::ffi;

    use libc::c_int;

    #[derive(Copy, Clone)]
    pub struct BufferSizeResult {
        pub buffer_size: c_int,
        pub linesize: c_int,
    }

    pub fn buffer_size(channels: c_int, samples: c_int, format: ffi::AVSampleFormat, align: bool)
                       -> Result<BufferSizeResult,c_int> {
        let mut linesize = 0;
        let align = if !align {
            0
        } else {
            1
        };
        let result = unsafe {
            ffi::av_samples_get_buffer_size(&mut linesize, channels, samples, format, align)
        };
        if result >= 0 {
            Ok(BufferSizeResult {
                buffer_size: result,
                linesize: linesize,
            })
        } else {
            Err(result)
        }
    }
}

// Implementation of the abstract `VideoDecoder` interface

#[allow(dead_code)]
struct VideoDecoderImpl {
    codec: AvCodec,
    context: RefCell<AvCodecContext>,
}

impl VideoDecoderImpl {
    fn h264(headers: &videodecoder::VideoHeaders, _: i32, _: i32)
           -> Result<Box<videodecoder::VideoDecoder + 'static>,()> {
        init();

        let avcc = h264::create_avcc_chunk(headers);
        let codec = try!(AvCodec::find_decoder(AV_CODEC_ID_H264));
        let mut context = AvCodecContext::new(&codec);
        context.set_extra_data(avcc);
        let (result, _) = context.open(&codec, AvDictionary::new());
        try!(result);
        Ok(Box::new(VideoDecoderImpl {
            codec: codec,
            context: RefCell::new(context),
        }) as Box<videodecoder::VideoDecoder + 'static>)
    }
}

impl videodecoder::VideoDecoder for VideoDecoderImpl {
    fn decode_frame(&self, data: &[u8], presentation_time: &Timestamp)
                    -> Result<Box<videodecoder::DecodedVideoFrame + 'static>,()> {
        let mut data: Vec<_> = data.iter().map(|x| *x).collect();
        for _ in 0 .. FF_INPUT_BUFFER_PADDING_SIZE {
            data.push(0);
        }

        let mut packet = AvPacket::new(&mut data);
        let presentation_time = *presentation_time;
        self.context.borrow_mut().set_get_buffer_callback(Box::new(move |frame: &AvFrame| {
            frame.set_user_data(Box::new(presentation_time))
        }));

        let frame = AvFrame::new();
        match self.context.borrow().decode_video(&frame, &mut packet) {
            Ok(true) => {
                Ok(Box::new(DecodedVideoFrameImpl {
                    frame: frame,
                }) as Box<videodecoder::DecodedVideoFrame>)
            }
            Ok(false) | Err(_) => Err(()),
        }
    }
}

struct DecodedVideoFrameImpl {
    frame: AvFrame,
}

impl videodecoder::DecodedVideoFrame for DecodedVideoFrameImpl {
    fn width(&self) -> c_uint {
        self.frame.width() as c_uint
    }

    fn height(&self) -> c_uint {
        self.frame.height() as c_uint
    }

    fn stride(&self, plane_index: usize) -> c_int {
        self.frame.linesize(plane_index)
    }

    fn pixel_format<'a>(&'a self) -> PixelFormat<'a> {
        PixelFormat::I420
    }

    fn presentation_time(&self) -> Timestamp {
        *self.frame.user_data().downcast_ref::<Timestamp>().unwrap()
    }

    fn lock<'a>(&'a self) -> Box<videodecoder::DecodedVideoFrameLockGuard + 'a> {
        Box::new(DecodedVideoFrameLockGuardImpl {
            frame: &self.frame,
        }) as Box<videodecoder::DecodedVideoFrameLockGuard + 'a>
    }
}

struct DecodedVideoFrameLockGuardImpl<'a> {
    frame: &'a AvFrame,
}

impl<'a> videodecoder::DecodedVideoFrameLockGuard for DecodedVideoFrameLockGuardImpl<'a> {
    fn pixels<'b>(&'b self, plane_index: usize) -> &'b [u8] {
        self.frame.video_data(plane_index)
    }
}

pub const VIDEO_DECODER: videodecoder::RegisteredVideoDecoder =
    videodecoder::RegisteredVideoDecoder {
        id: [ b'a', b'v', b'c', b' ' ],
        constructor: VideoDecoderImpl::h264,
    };

// Implementation of the abstract `AudioDecoder` interface

struct AudioDecoderInfoImpl {
    sample_rate: c_int,
    channels: c_int,
}

impl AudioDecoderInfoImpl {
    fn aac(_: &audiodecoder::AudioHeaders, sample_rate: f64, channels: u16)
           -> Box<audiodecoder::AudioDecoderInfo + 'static> {
        Box::new(AudioDecoderInfoImpl {
            sample_rate: sample_rate as c_int,
            channels: channels as c_int,
        })
    }
}

impl audiodecoder::AudioDecoderInfo for AudioDecoderInfoImpl {
    fn create_decoder(self: Box<AudioDecoderInfoImpl>)
                      -> Box<audiodecoder::AudioDecoder + 'static> {
        init();

        let codec = AvCodec::find_decoder(AV_CODEC_ID_AAC).unwrap();
        let context = AvCodecContext::new(&codec);
        let mut options = AvDictionary::new();
        options.set("ac", &self.channels.to_string());
        options.set("ar", &self.sample_rate.to_string());
        options.set("request_sample_fmt", "fltp");

        let (result, _) = context.open(&codec, options);
        result.unwrap();
        Box::new(AudioDecoderImpl {
            context: context,
            frame: None,
        }) as Box<audiodecoder::AudioDecoder + 'static>
    }
}

struct AudioDecoderImpl {
    context: AvCodecContext,
    frame: Option<AvFrame>,
}

impl audiodecoder::AudioDecoder for AudioDecoderImpl {
    fn decode(&mut self, data: &[u8]) -> Result<(),()> {
        let data_len = data.len();
        let mut data: Vec<_> = data.iter().map(|x| *x).collect();
        for _ in 0 .. FF_INPUT_BUFFER_PADDING_SIZE {
            data.push(0);
        }
        let mut packet = AvPacket::new(&mut data);
        let frame = AvFrame::new();
        let result = self.context.decode_audio(&frame, &mut packet);
        match result {
            Ok(length) if length as usize == data_len => {
                self.frame = Some(frame);
                Ok(())
            }
            _ => Err(()),
        }
    }

    fn decoded_samples<'a>(&'a mut self)
                           -> Result<Box<audiodecoder::DecodedAudioSamples + 'a>,()> {
        match self.frame {
            Some(ref frame) => {
                Ok(Box::new(DecodedAudioSamplesImpl {
                    frame: frame,
                    channels: self.context.channels(),
                }) as Box<audiodecoder::DecodedAudioSamples>)
            }
            None => Err(()),
        }
    }

    fn acknowledge(&mut self, _: c_int) {
        self.frame = None
    }
}

struct DecodedAudioSamplesImpl<'a> {
    frame: &'a AvFrame,
    channels: i32,
}

impl<'a> audiodecoder::DecodedAudioSamples for DecodedAudioSamplesImpl<'a> {
    fn samples<'b>(&'b self, channel: i32) -> Option<&'b [f32]> {
        let data = self.frame.audio_data(channel as usize, self.channels);
        unsafe {
            Some(mem::transmute::<&[f32],
                                  &'b [f32]>(slice::from_raw_parts((data.as_ptr() as *const f32),
                                                                 data.len() /
                                                                    mem::size_of::<f32>())))
        }
    }
}

pub const AUDIO_DECODER: audiodecoder::RegisteredAudioDecoder =
    audiodecoder::RegisteredAudioDecoder {
        id: [ b'a', b'a', b'c', b' ' ],
        constructor: AudioDecoderInfoImpl::aac,
    };

#[allow(missing_copy_implementations)]
pub mod ffi {
    use libc::{c_char, c_double, c_float, c_int, c_short, c_uint, c_void};

    pub type AVCodecID = c_int;
    pub type AVColorRange = c_int;
    pub type AVColorSpace = c_int;
    pub type AVPictureType = c_int;
    pub type AVSampleFormat = c_int;

    pub const AV_NUM_DATA_POINTERS: usize = 8;

    #[repr(C)]
    pub struct AVBuffer;
    #[repr(C)]
    pub struct AVClass;
    #[repr(C)]
    pub struct AVCodec;
    #[repr(C)]
    pub struct AVCodecContext;
    #[repr(C)]
    pub struct AVCodecInternal;
    #[repr(C)]
    pub struct AVDictionary;
    #[repr(C)]
    pub struct AVFrameSideData;
    #[repr(C)]
    pub struct AVPacket;
    #[repr(C)]
    pub struct AVPacketSideData;
    #[repr(C)]
    pub struct AVPanScan;

    #[repr(C)]
    pub struct AVBufferRef {
        pub buffer: *mut AVBuffer,
        pub data: *mut u8,
        pub size: c_int,
    }

    /// `AVPacket` for `libavcodec` below version 0x380D64.
    #[repr(C)]
    pub struct AVCodecContextV362300 {
        pub av_class: *const AVClass,
        pub log_level_offset: c_int,
        pub codec_type: c_int,
        pub codec: *const AVCodec,
        pub codec_name: [c_char; 32],
        pub codec_id: AVCodecID,
        pub codec_tag: c_uint,
        pub stream_codec_tag: c_uint,
        pub sub_id: c_int,
        pub priv_data: *mut c_void,
        pub internal: *mut AVCodecInternal,
        pub opaque: *mut c_void,
        pub bit_rate: c_int,
        pub bit_rate_tolerance: c_int,
        pub global_quality: c_int,
        pub compression_level: c_int,
        pub flags: c_int,
        pub flags2: c_int,
        pub extradata: *mut u8,
        pub extradata_size: c_int,
        pub time_base: AVRational,
        pub ticks_per_frame: c_int,
        pub delay: c_int,
        pub width: c_int,
        pub height: c_int,
        pub coded_width: c_int,
        pub coded_height: c_int,
        pub gop_size: c_int,
        pub pix_fmt: c_int,
        pub me_method: c_int,
        pub draw_horiz_band: extern "C" fn(s: *mut AVCodecContext,
                                           src: *const AVFrame,
                                           offset: [c_int; AV_NUM_DATA_POINTERS],
                                           y: c_int,
                                           band_type: c_int,
                                           height: c_int),
        pub get_format: extern "C" fn(s: *mut AVCodecContext, fmt: *const c_int),
        pub max_b_frames: c_int,
        pub b_quant_factor: c_float,
        pub rc_strategy: c_int,
        pub b_frame_strategy: c_int,
        pub luma_elim_threshold: c_int,
        pub chroma_elim_threshold: c_int,
        pub b_quant_offset: c_float,
        pub has_b_frames: c_int,
        pub mpeg_quant: c_int,
        pub i_quant_factor: c_float,
        pub i_quant_offset: c_float,
        pub lumi_masking: c_float,
        pub temporal_cplx_masking: c_float,
        pub spatial_cplx_masking: c_float,
        pub p_masking: c_float,
        pub dark_masking: c_float,
        pub slice_count: c_int,
        pub prediction_method: c_int,
        pub slice_offset: *mut c_int,
        pub sample_aspect_ratio: AVRational,
        pub me_cmp: c_int,
        pub me_sub_cmp: c_int,
        pub mb_cmp: c_int,
        pub ildct_cmp: c_int,
        pub dia_size: c_int,
        pub last_predictor_count: c_int,
        pub pre_me: c_int,
        pub me_pre_cmp: c_int,
        pub pre_dia_size: c_int,
        pub me_subpel_quality: c_int,
        pub dtg_active_format: c_int,
        pub me_range: c_int,
        pub intra_quant_bias: c_int,
        pub inter_quant_bias: c_int,
        pub color_table_id: c_int,
        pub slice_flags: c_int,
        pub xvmc_acceleration: c_int,   // NB: Behind `#ifdef FF_API_XVMC`!
        pub mb_decision: c_int,
        pub intra_matrix: *mut u16,
        pub inter_matrix: *mut u16,
        pub scenechange_threshold: c_int,
        pub noise_reduction: c_int,
        pub inter_threshold: c_int,
        pub quantizer_noise_shaping: c_int,
        pub me_threshold: c_int,
        pub mb_threshold: c_int,
        pub intra_dc_precision: c_int,
        pub skip_top: c_int,
        pub skip_bottom: c_int,
        pub border_masking: c_float,
        pub mb_lmin: c_int,
        pub mb_lmax: c_int,
        pub me_penalty_compensation: c_int,
        pub bidir_refine: c_int,
        pub brd_scale: c_int,
        pub keyint_min: c_int,
        pub refs: c_int,
        pub chromaoffset: c_int,
        pub scenechange_factor: c_int,
        pub mv0_threshold: c_int,
        pub b_sensitivity: c_int,
        pub color_primaries: c_int,
        pub color_trc: c_int,
        pub colorspace: c_int,
        pub color_range: c_int,
        pub chroma_sample_location: c_int,
        pub slices: c_int,
        pub field_order: c_int,
        pub sample_rate: c_int,
        pub channels: c_int,
        pub sample_fmt: c_int,
        pub frame_size: c_int,
        pub frame_number: c_int,
        pub block_align: c_int,
        pub cutoff: c_int,
        pub request_channels: c_int,    // NB: Behind `#ifdef FF_API_REQUEST_CHANNELS`!
        pub channel_layout: u64,
        pub request_channel_layout: u64,
        pub audio_service_type: c_int,
        pub request_sample_fmt: c_int,
        // NB: The next three are behind `#ifdef FF_API_GET_BUFFER`!
        pub get_buffer: extern "C" fn(c: *mut AVCodecContext, pic: *mut AVFrame) -> c_int,
        pub release_buffer: extern "C" fn(c: *mut AVCodecContext, pic: *mut AVFrame),
        pub reget_buffer: extern "C" fn(c: *mut AVCodecContext, pic: *mut AVFrame),
        pub get_buffer2: extern "C" fn(s: *mut AVCodecContext, frame: *mut AVFrame, flags: c_int)
                                       -> c_int,
        // More follow...
    }

    /// `AVPacket` for `libavcodec` version 0x380D64 or greater.
    #[repr(C)]
    pub struct AVCodecContextV380D64 {
        pub av_class: *const AVClass,
        pub log_level_offset: c_int,
        pub codec_type: c_int,
        pub codec: *const AVCodec,
        pub codec_name: [c_char; 32],
        pub codec_id: AVCodecID,
        pub codec_tag: c_uint,
        pub stream_codec_tag: c_uint,
        pub priv_data: *mut c_void,
        pub internal: *mut AVCodecInternal,
        pub opaque: *mut c_void,
        pub bit_rate: c_int,
        pub bit_rate_tolerance: c_int,
        pub global_quality: c_int,
        pub compression_level: c_int,
        pub flags: c_int,
        pub flags2: c_int,
        pub extradata: *mut u8,
        pub extradata_size: c_int,
        pub time_base: AVRational,
        pub ticks_per_frame: c_int,
        pub delay: c_int,
        pub width: c_int,
        pub height: c_int,
        pub coded_width: c_int,
        pub coded_height: c_int,
        pub gop_size: c_int,
        pub pix_fmt: c_int,
        pub me_method: c_int,
        pub draw_horiz_band: extern "C" fn(s: *mut AVCodecContext,
                                           src: *const AVFrame,
                                           offset: [c_int; AV_NUM_DATA_POINTERS],
                                           y: c_int,
                                           band_type: c_int,
                                           height: c_int),
        pub get_format: extern "C" fn(s: *mut AVCodecContext, fmt: *const c_int),
        pub max_b_frames: c_int,
        pub b_quant_factor: c_float,
        pub rc_strategy: c_int,
        pub b_frame_strategy: c_int,
        pub b_quant_offset: c_float,
        pub has_b_frames: c_int,
        pub mpeg_quant: c_int,
        pub i_quant_factor: c_float,
        pub i_quant_offset: c_float,
        pub lumi_masking: c_float,
        pub temporal_cplx_masking: c_float,
        pub spatial_cplx_masking: c_float,
        pub p_masking: c_float,
        pub dark_masking: c_float,
        pub slice_count: c_int,
        pub prediction_method: c_int,
        pub slice_offset: *mut c_int,
        pub sample_aspect_ratio: AVRational,
        pub me_cmp: c_int,
        pub me_sub_cmp: c_int,
        pub mb_cmp: c_int,
        pub ildct_cmp: c_int,
        pub dia_size: c_int,
        pub last_predictor_count: c_int,
        pub pre_me: c_int,
        pub me_pre_cmp: c_int,
        pub pre_dia_size: c_int,
        pub me_subpel_quality: c_int,
        pub dtg_active_format: c_int,
        pub me_range: c_int,
        pub intra_quant_bias: c_int,
        pub inter_quant_bias: c_int,
        pub slice_flags: c_int,
        pub xvmc_acceleration: c_int,   // NB: Behind `#ifdef FF_API_XVMC`!
        pub mb_decision: c_int,
        pub intra_matrix: *mut u16,
        pub inter_matrix: *mut u16,
        pub scenechange_threshold: c_int,
        pub noise_reduction: c_int,
        pub me_threshold: c_int,
        pub mb_threshold: c_int,
        pub intra_dc_precision: c_int,
        pub skip_top: c_int,
        pub skip_bottom: c_int,
        pub border_masking: c_float,
        pub mb_lmin: c_int,
        pub mb_lmax: c_int,
        pub me_penalty_compensation: c_int,
        pub bidir_refine: c_int,
        pub brd_scale: c_int,
        pub keyint_min: c_int,
        pub refs: c_int,
        pub chromaoffset: c_int,
        pub scenechange_factor: c_int,
        pub mv0_threshold: c_int,
        pub b_sensitivity: c_int,
        pub color_primaries: c_int,
        pub color_trc: c_int,
        pub colorspace: c_int,
        pub color_range: c_int,
        pub chroma_sample_location: c_int,
        pub slices: c_int,
        pub field_order: c_int,
        pub sample_rate: c_int,
        pub channels: c_int,
        pub sample_fmt: c_int,
        pub frame_size: c_int,
        pub frame_number: c_int,
        pub block_align: c_int,
        pub cutoff: c_int,
        pub request_channels: c_int,    // NB: Behind `#ifdef FF_API_REQUEST_CHANNELS`!
        pub channel_layout: u64,
        pub request_channel_layout: u64,
        pub audio_service_type: c_int,
        pub request_sample_fmt: c_int,
        // NB: The next three are behind `#ifdef FF_API_GET_BUFFER`!
        pub get_buffer: extern "C" fn(c: *mut AVCodecContext, pic: *mut AVFrame) -> c_int,
        pub release_buffer: extern "C" fn(c: *mut AVCodecContext, pic: *mut AVFrame),
        pub reget_buffer: extern "C" fn(c: *mut AVCodecContext, pic: *mut AVFrame),
        pub get_buffer2: extern "C" fn(s: *mut AVCodecContext, frame: *mut AVFrame, flags: c_int)
                                       -> c_int,
        // More follow...
    }

    pub enum EitherAVCodecContext {
        V362300(*mut AVCodecContextV362300),
        V380D64(*mut AVCodecContextV380D64),
    }

    impl EitherAVCodecContext {
        pub fn ptr(&self) -> *mut AVCodecContext {
            match *self {
                EitherAVCodecContext::V362300(context) => context as *mut AVCodecContext,
                EitherAVCodecContext::V380D64(context) => context as *mut AVCodecContext,
            }
        }
    }

    #[repr(C)]
    pub struct AVFrame {
        pub data: [*mut u8; AV_NUM_DATA_POINTERS],
        pub linesize: [c_int; AV_NUM_DATA_POINTERS],
        pub extended_data: *mut *mut u8,
        pub width: c_int,
        pub height: c_int,
        pub nb_samples: c_int,
        pub format: c_int,
        pub keyframe: c_int,
        pub pict_type: AVPictureType,
        pub base: [*mut u8; AV_NUM_DATA_POINTERS],
        pub sample_aspect_ratio: AVRational,
        pub pts: i64,
        pub pkt_pts: i64,
        pub pkt_dts: i64,
        pub coded_picture_number: c_int,
        pub display_picture_number: c_int,
        pub quality: c_int,
        pub reference: c_int,
        pub qscale_table: *mut i8,
        pub qstride: c_int,
        pub qscale_type: c_int,
        pub mbskip_table: *mut u8,
        pub motion_val: [[*mut i16; 2]; 2],
        pub mb_type: *mut u32,
        pub dct_coeff: *mut c_short,
        pub ref_index: [*mut i8; 2],
        pub opaque: *mut c_void,
        pub error: [u64; AV_NUM_DATA_POINTERS],
        pub frame_type: c_int,
        pub repeat_pict: c_int,
        pub interlaced_frame: c_int,
        pub top_field_first: c_int,
        pub palette_has_changed: c_int,
        pub buffer_hints: c_int,
        pub pan_scan: *mut AVPanScan,
        pub reordered_opaque: i64,
        pub hwaccel_picture_private: *mut c_void,
        pub owner: *mut AVCodecContext,
        pub thread_opaque: *mut c_void,
        pub motion_subsample_log2: u8,
        pub sample_rate: c_int,
        pub channel_layout: u64,
        pub buf: [*mut AVBufferRef; AV_NUM_DATA_POINTERS],
        pub extended_buf: *mut *mut AVBufferRef,
        pub nb_extended_buf: c_int,
        pub side_data: *mut *mut AVFrameSideData,
        pub nb_side_data: c_int,
        pub flags: c_int,
        pub best_effort_timestamp: i64,
        pub pkt_pos: i64,
        pub pkt_duration: i64,
        pub metadata: *mut AVDictionary,
        pub decode_error_flags: c_int,
        pub channels: c_int,
        pub pkt_size: c_int,
        pub colorspace: AVColorSpace,
        pub color_range: AVColorRange,
        pub qp_table_buf: *mut AVBufferRef,
    }

    /// `AVPacket` for `libavcodec` below version 0x380D64.
    #[repr(C)]
    pub struct AVPacketV362300 {
        pub pts: i64,
        pub dts: i64,
        pub data: *mut u8,
        pub size: c_int,
        pub stream_index: c_int,
        pub flags: c_int,
        pub side_data: *mut AVPacketSideData,
        pub side_data_elems: c_int,
        pub duration: c_int,
        pub destruct: extern "C" fn(packet: *mut AVPacket),
        pub private: *mut c_void,
        pub pos: i64,
        pub convergence_duration: i64,
    }

    /// `AVPacket` for `libavcodec` version 0x380d64 and up.
    #[repr(C)]
    pub struct AVPacketV380D64 {
        pub buf: *mut AVBufferRef,
        pub pts: i64,
        pub dts: i64,
        pub data: *mut u8,
        pub size: c_int,
        pub stream_index: c_int,
        pub flags: c_int,
        pub side_data: *mut AVPacketSideData,
        pub side_data_elems: c_int,
        pub duration: c_int,
        pub destruct: extern "C" fn(packet: *mut AVPacket),
        pub private: *mut c_void,
        pub pos: i64,
        pub convergence_duration: i64,
    }

    pub enum EitherAVPacket {
        V362300(AVPacketV362300),
        V380D64(AVPacketV380D64),
    }

    impl EitherAVPacket {
        pub fn ptr(&mut self) -> *mut AVPacket {
            match *self {
                EitherAVPacket::V362300(ref mut packet) => {
                    packet as *mut AVPacketV362300 as *mut AVPacket
                }
                EitherAVPacket::V380D64(ref mut packet) => {
                    packet as *mut AVPacketV380D64 as *mut AVPacket
                }
            }
        }
    }

    #[repr(C)]
    #[derive(Copy, Clone, Debug)]
    pub struct AVRational {
        pub num: c_int,
        pub den: c_int,
    }

    #[link(name="avcodec")]
    extern {
        pub fn avcodec_version() -> c_uint;
        pub fn avcodec_register_all();
        pub fn avcodec_find_decoder(id: AVCodecID) -> *mut AVCodec;
        pub fn avcodec_alloc_context3(codec: *const AVCodec) -> *mut AVCodecContext;
        pub fn avcodec_open2(avctx: *mut AVCodecContext,
                             codec: *const AVCodec,
                             options: *mut *mut AVDictionary)
                             -> c_int;
        pub fn avcodec_decode_video2(avctx: *mut AVCodecContext,
                                     picture: *mut AVFrame,
                                     got_picture_ptr: *mut c_int,
                                     avpkt: *const AVPacket)
                                     -> c_int;
        pub fn avcodec_decode_audio4(avctx: *mut AVCodecContext,
                                     frame: *mut AVFrame,
                                     got_frame_ptr: *mut c_int,
                                     avpkt: *const AVPacket)
                                     -> c_int;
        pub fn av_codec_set_pkt_timebase(avctx: *mut AVCodecContext, val: AVRational);
        pub fn avcodec_default_get_buffer(s: *mut AVCodecContext, frame: *mut AVFrame) -> c_int;
        pub fn av_init_packet(packet: *mut AVPacket);
        pub fn avcodec_alloc_frame() -> *mut AVFrame;
        pub fn avcodec_free_frame(frame: *mut *mut AVFrame);
    }

    #[link(name="avutil")]
    extern {
        pub fn av_dict_free(m: *mut *mut AVDictionary);
        pub fn av_dict_set(pm: *mut *mut AVDictionary,
                           key: *const c_char,
                           value: *const c_char,
                           flags: c_int)
                           -> c_int;
        pub fn av_frame_get_plane_buffer(frame: *mut AVFrame, plane: c_int) -> *mut AVBufferRef;
        pub fn av_opt_get_double(obj: *mut c_void,
                                 name: *const c_char,
                                 search_flags: c_int,
                                 out_val: *mut c_double)
                                 -> c_int;
        pub fn av_opt_get_q(obj: *mut c_void,
                            name: *const c_char,
                            search_flags: c_int,
                            out_val: *mut AVRational)
                            -> c_int;
        pub fn av_samples_get_buffer_size(linesize: *mut c_int,
                                          nb_channels: c_int,
                                          nb_samples: c_int,
                                          sample_fmt: AVSampleFormat,
                                          align: c_int)
                                          -> c_int;
    }
}

