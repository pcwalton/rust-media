// Copyright 2015 The Servo Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

#![allow(missing_copy_implementations, non_camel_case_types, non_snake_case)]
#![allow(non_upper_case_globals)]

use decoder;

use libc::{self, c_char, c_int, c_long, c_uchar, c_uint, c_ulonglong, c_void};
use std::mem;
use std::ptr;
use std::slice;

pub struct Decoder {
    decoder: *mut ISVCDecoder,
}

impl Drop for Decoder {
    fn drop(&mut self) {
        unsafe {
            WelsDestroyDecoder(self.decoder)
        }
    }
}

impl Decoder {
    pub fn new() -> Result<Decoder,c_long> {
        let mut decoder = ptr::null_mut();
        let err = unsafe {
            WelsCreateDecoder(&mut decoder)
        };
        if err != 0 {
            return Err(err)
        }

        unsafe {
            ((**decoder).SetOption)(decoder,
                                    DECODER_OPTION_TRACE_LEVEL,
                                    &WELS_LOG_WARNING as *const _ as *const c_void);
        }

        let params = SDecodingParam {
            pFileNameRestructed: ptr::null_mut(),
            eOutputColorFormat: videoFormatI420,
            uiCpuLoad: 0,
            uiTargetDqLayer: -1,
            eEcActiveIdc: ERROR_CON_SLICE_COPY,
            bParseOnly: false,
            sVideoProperty: SVideoProperty {
                size: mem::size_of::<SVideoProperty>() as u32,
                eVideoBsType: VIDEO_BITSTREAM_SVC,
            },
        };
        let err = unsafe {
            ((**decoder).Initialize)(decoder, &params)
        };
        if err == 0 {
            Ok(Decoder {
                decoder: decoder,
            })
        } else {
            Err(err)
        }
    }

    pub fn decode_frame(&self, src: &[u8]) -> Result<DecodedFrame,DECODING_STATE> {
        let mut planes = [ ptr::null_mut(), ptr::null_mut(), ptr::null_mut() ];
        let mut strides = [ 0, 0, 0 ];
        let (mut width, mut height) = (0, 0);
        let err = unsafe {
            ((**self.decoder).DecodeFrame)(self.decoder,
                                           src.as_ptr(),
                                           src.len() as c_int,
                                           planes.as_mut_ptr(),
                                           strides.as_mut_ptr(),
                                           &mut width,
                                           &mut height)
        };
        if err == dsErrorFree {
            Ok(DecodedFrame {
                planes: planes,
                strides: strides,
                width: width,
                height: height,
            })
        } else {
            println!("failed: {}", err);
            Err(err)
        }
    }
}

pub struct DecodedFrame {
    planes: [*mut u8; 3],
    strides: [c_int; 3],
    width: c_int,
    height: c_int,
}

impl Drop for DecodedFrame {
    fn drop(&mut self) {
        for &plane in self.planes.iter() {
            unsafe {
                libc::free(plane as *mut c_void)
            }
        }
    }
}

impl DecodedFrame {
    pub fn plane<'a>(&'a self, index: usize) -> &'a [u8] {
        let mut plane = self.planes[index as usize];
        let len = (self.stride(index) * self.height) as usize;
        unsafe {
            mem::transmute::<&_,&_>(slice::from_raw_mut_buf(&mut plane, len))
        }
    }

    pub fn width(&self) -> c_int {
        self.width
    }

    pub fn height(&self) -> c_int {
        self.height
    }

    pub fn stride(&self, index: usize) -> c_int {
        self.strides[index]
    }
}

// Implementation of the `Decoder` interface

pub struct DecoderImpl {
    decoder: Decoder,
}

impl DecoderImpl {
    pub fn new() -> Result<Box<decoder::Decoder + 'static>,()> {
        match Decoder::new() {
            Ok(decoder) => {
                Ok(Box::new(DecoderImpl {
                    decoder: decoder,
                }) as Box<decoder::Decoder + 'static>)
            }
            Err(_) => Err(()),
        }
    }
}

impl decoder::Decoder for DecoderImpl {
    fn set_headers(&mut self, headers: &decoder::Headers, _: i32, _: i32) -> Result<(),()> {
        // Convert to Annex B format.
        let mut annex_b = Vec::new();
        for seq_header in headers.h264_seq_headers().unwrap().iter() {
            copy_with_start_code(&mut annex_b, seq_header.as_slice())
        }
        for pict_header in headers.h264_pict_headers().unwrap().iter() {
            copy_with_start_code(&mut annex_b, pict_header.as_slice())
        }
        return match self.decode_frame(annex_b.as_slice()) {
            Ok(_) => Ok(()),
            Err(_) => Err(()),
        };

        fn copy_with_start_code(annex_b: &mut Vec<u8>, data: &[u8]) {
            annex_b.push_all(&[ 0, 0, 0, 1 ]);
            annex_b.push_all(data);
        }
    }

    fn decode_frame(&self, data: &[u8]) -> Result<Box<decoder::DecodedFrame>,()> {
        match self.decoder.decode_frame(data) {
            Ok(frame) => {
                Ok(Box::new(DecodedFrameImpl {
                    frame: frame,
                }) as Box<decoder::DecodedFrame>)
            }
            Err(_) => Err(()),
        }
    }
}

struct DecodedFrameImpl {
    frame: DecodedFrame,
}

impl decoder::DecodedFrame for DecodedFrameImpl {
    fn width(&self) -> c_uint {
        self.frame.width() as c_uint
    }

    fn height(&self) -> c_uint {
        self.frame.height() as c_uint
    }

    fn stride(&self, index: usize) -> c_int {
        self.frame.stride(index)
    }

    fn lock<'a>(&'a self) -> Box<decoder::DecodedFrameLockGuard + 'a> {
        Box::new(DecodedFrameLockGuardImpl {
            frame: &self.frame,
        }) as Box<decoder::DecodedFrameLockGuard + 'a>
    }
}

struct DecodedFrameLockGuardImpl<'a> {
    frame: &'a DecodedFrame,
}

impl<'a> decoder::DecodedFrameLockGuard for DecodedFrameLockGuardImpl<'a> {
    fn pixels<'b>(&'b self, plane_index: usize) -> &'b [u8] {
        self.frame.plane(plane_index)
    }
}

// FFI stuff

pub type DECODER_OPTION = c_int;
pub type DECODING_STATE = c_int;
pub type VIDEO_BITSTREAM_TYPE = c_int;

pub type ERROR_CON_IDC = c_int;
pub type EVideoFormatType = c_int;

pub type ISVCDecoder = *const ISVCDecoderVtbl;

pub const dsErrorFree: DECODING_STATE = 0x00;

pub const MAX_NAL_UNITS_IN_LAYER: usize = 128;

pub const ERROR_CON_SLICE_COPY: ERROR_CON_IDC = 2;

pub const WELS_LOG_WARNING: c_int = 2;

pub const DECODER_OPTION_TRACE_LEVEL: DECODER_OPTION = 9;

pub const videoFormatI420: EVideoFormatType = 23;

pub const VIDEO_BITSTREAM_AVC: VIDEO_BITSTREAM_TYPE = 0;
pub const VIDEO_BITSTREAM_SVC: VIDEO_BITSTREAM_TYPE = 1;
pub const VIDEO_BITSTREAM_DEFAULT: VIDEO_BITSTREAM_TYPE = VIDEO_BITSTREAM_SVC;

#[repr(C)]
pub struct ISVCDecoderVtbl {
    Initialize: extern "C" fn(this: *const ISVCDecoder, pParam: *const SDecodingParam) -> c_long,
    Uninitialize: extern "C" fn(this: *const ISVCDecoder) -> c_long,
    DecodeFrame: extern "C" fn(this: *mut ISVCDecoder,
                               pSrc: *const c_uchar,
                               iSrcLen: c_int,
                               ppDst: *mut *mut c_uchar,
                               pStride: *mut c_int,
                               iWidth: *mut c_int,
                               iHeight: *mut c_int)
                               -> DECODING_STATE,
    DecodeFrameNoDelay: extern "C" fn(this: *mut ISVCDecoder,
                                      pSrc: *const c_uchar,
                                      iSrcLen: c_int,
                                      ppDst: *mut *mut c_uchar,
                                      pDstInfo: *mut SBufferInfo)
                                      -> DECODING_STATE,
    DecodeFrame2: extern "C" fn(this: *mut ISVCDecoder,
                                pSrc: *const c_uchar,
                                iSrcLen: c_int,
                                ppDst: *mut *mut c_uchar,
                                pDstInfo: *mut SBufferInfo)
                                -> DECODING_STATE,
    DecodeParser: extern "C" fn(this: *mut ISVCDecoder,
                                pSrc: *const c_uchar,
                                iSrcLen: c_int,
                                pDstInfo: *mut SParserBsInfo)
                                -> DECODING_STATE,
    DecodeFrameEx: extern "C" fn(this: *mut ISVCDecoder,
                                 pSrc: *const c_uchar,
                                 iSrcLen: c_int,
                                 pDst: *mut c_uchar,
                                 iDstStride: c_int,
                                 iDstLen: *mut c_int,
                                 iWidth: *mut c_int,
                                 iHeight: *mut c_int,
                                 iColorFormat: *mut c_int)
                                 -> DECODING_STATE,
    SetOption: extern "C" fn(this: *mut ISVCDecoder,
                             eOptionId: DECODER_OPTION,
                             pOption: *const c_void)
                             -> c_long,
    GetOption: extern "C" fn(this: *mut ISVCDecoder,
                             eOptionId: DECODER_OPTION,
                             pOption: *mut c_void)
                             -> c_long,
}

#[repr(C)]
pub struct SDecodingParam {
    pFileNameRestructed: *mut c_char,
    eOutputColorFormat: EVideoFormatType,
    uiCpuLoad: c_uint,
    uiTargetDqLayer: c_uint,
    eEcActiveIdc: ERROR_CON_IDC,
    bParseOnly: bool,
    sVideoProperty: SVideoProperty,
}

#[repr(C)]
pub struct SBufferInfo {
    iBufferStatus: c_int,
    uiInBsTimeStamp: c_ulonglong,
    uiOutYuvTimeStamp: c_ulonglong,
    sSystemBuffer: SSysMEMBuffer,
}

#[repr(C)]
pub struct SParserBsInfo {
    iNalNum: c_int,
    iNalLenInByte: [c_int; MAX_NAL_UNITS_IN_LAYER],
    pDstBuff: *mut c_uchar,
    iSpsWidthInPixel: c_int,
    iSpsHeightInPixel: c_int,
    uiInBsTimeStamp: c_ulonglong,
    uiOutBsTimeStamp: c_ulonglong,
}

#[repr(C)]
pub struct SSysMEMBuffer {
    iWidth: c_int,
    iHeight: c_int,
    iFormat: c_int,
    iStride: [c_int; 2],
}

#[repr(C)]
pub struct SVideoProperty {
    size: c_uint,
    eVideoBsType: VIDEO_BITSTREAM_TYPE,
}

#[link(name="openh264")]
extern {
    fn WelsCreateDecoder(ppDecoder: *mut *mut ISVCDecoder) -> c_long;
    fn WelsDestroyDecoder(pDecoder: *mut ISVCDecoder);
}

