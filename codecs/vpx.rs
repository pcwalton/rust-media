// Copyright 2015 The Servo Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

#![allow(missing_copy_implementations)]

use pixelformat::PixelFormat;
use timing::Timestamp;
use videodecoder;

use libc::{c_int, c_long, c_uint};
use std::ptr;
use std::slice;
use std::u32;
use std::marker::PhantomData;

pub struct VpxCodecIface {
    iface: *mut ffi::vpx_codec_iface_t,
}

impl VpxCodecIface {
    pub fn vp8() -> VpxCodecIface {
        VpxCodecIface {
            iface: unsafe {
                ffi::vpx_codec_vp8_dx()
            },
        }
    }
}

pub struct VpxCodec {
    ctx: ffi::vpx_codec_ctx_t,
}

impl VpxCodec {
    pub fn init(iface: &VpxCodecIface) -> Result<VpxCodec,ffi::vpx_codec_err_t> {
        let mut ctx = ffi::vpx_codec_ctx_t {
            name: ptr::null(),
            iface: ptr::null_mut(),
            err: 0,
            err_detail: ptr::null(),
            init_flags: 0,
            config: ptr::null(),
            private: ptr::null_mut(),
        };
        let err = unsafe {
            ffi::vpx_codec_dec_init_ver(&mut ctx,
                                        iface.iface,
                                        ptr::null(),
                                        0,
                                        ffi::VPX_DECODER_ABI_VERSION)
        };
        if err != ffi::VPX_CODEC_OK {
            return Err(err)
        }
        Ok(VpxCodec {
            ctx: ctx,
        })
    }

    pub fn decode(&self, data: &[u8], deadline: c_long) -> Result<(),ffi::vpx_codec_err_t> {
        assert!(data.len() <= (u32::MAX as usize));
        let error = unsafe {
            ffi::vpx_codec_decode(&self.ctx as *const _ as *mut _,
                                  data.as_ptr(),
                                  data.len() as c_uint,
                                  ptr::null_mut(),
                                  deadline)
        };
        if error == ffi::VPX_CODEC_OK {
            Ok(())
        } else {
            Err(error)
        }
    }

    pub fn frame<'a>(&'a self, iter: &mut Option<VpxCodecIter<'a>>) -> Option<VpxImage> {
        let mut iter_ptr = match *iter {
            None => ptr::null_mut(),
            Some(ref iter) => iter.iter,
        };
        let image = unsafe {
            ffi::vpx_codec_get_frame(&self.ctx as *const ffi::vpx_codec_ctx_t
                                               as *mut ffi::vpx_codec_ctx_t,
                                     &mut iter_ptr)
        };
        *iter = if iter_ptr == ptr::null_mut() {
            None
        } else {
            Some(VpxCodecIter {
                iter: iter_ptr,
                phantom: PhantomData,
            })
        };
        if !image.is_null() {
            Some(VpxImage {
                image: image,
            })
        } else {
            None
        }
    }
}

pub struct VpxCodecIter<'a> {
    iter: ffi::vpx_codec_iter_t,
    phantom: PhantomData<&'a u8>,
}

pub struct VpxImage {
    image: *mut ffi::vpx_image_t,
}

impl Drop for VpxImage {
    fn drop(&mut self) {
        unsafe {
            ffi::vpx_img_free(self.image)
        }
    }
}

impl VpxImage {
    pub fn width(&self) -> c_uint {
        unsafe {
            (*self.image).w
        }
    }

    pub fn height(&self) -> c_uint {
        unsafe {
            (*self.image).h
        }
    }

    pub fn bit_depth(&self) -> c_uint {
        unsafe {
            (*self.image).bit_depth
        }
    }

    pub fn stride(&self, index: c_uint) -> c_int {
        assert!(index < 4);
        unsafe {
            (*self.image).stride[index as usize]
        }
    }

    pub fn plane<'a>(&'a self, index: c_uint) -> &'a [u8] {
        assert!(index < 4);
        unsafe {
            let len = (self.stride(index) as c_uint) * (*self.image).h;
            slice::from_raw_parts_mut((*self.image).planes[index as usize], len as usize)
        }
    }

    pub fn format(&self) -> ffi::vpx_img_fmt_t {
        unsafe {
            (*self.image).fmt
        }
    }

    pub fn bps(&self) -> c_int {
        unsafe {
            (*self.image).bps
        }
    }
}

// Implementation of the abstract `VideoDecoder` interface

struct VideoDecoderImpl {
    codec: VpxCodec,
}

impl VideoDecoderImpl {
    fn new(_: &videodecoder::VideoHeaders, _: i32, _: i32)
           -> Result<Box<videodecoder::VideoDecoder + 'static>,()> {
        match VpxCodec::init(&VpxCodecIface::vp8()) {
            Ok(codec) => {
                Ok(Box::new(VideoDecoderImpl {
                    codec: codec,
                }) as Box<videodecoder::VideoDecoder>)
            }
            Err(_) => Err(()),
        }
    }
}

impl videodecoder::VideoDecoder for VideoDecoderImpl {
    fn decode_frame(&self, data: &[u8], presentation_time: &Timestamp)
                    -> Result<Box<videodecoder::DecodedVideoFrame + 'static>,()> {
        if self.codec.decode(data, 0).is_err() {
            return Err(())
        }
        let image = match self.codec.frame(&mut None) {
            None => return Err(()),
            Some(image) => image,
        };
        if image.format() != ffi::VPX_IMG_FMT_I420 {
            return Err(())
        }
        Ok(Box::new(DecodedVideoFrameImpl {
            image: image,
            presentation_time: *presentation_time,
        }) as Box<videodecoder::DecodedVideoFrame>)
    }
}

struct DecodedVideoFrameImpl {
    image: VpxImage,
    presentation_time: Timestamp,
}

impl videodecoder::DecodedVideoFrame for DecodedVideoFrameImpl {
    fn width(&self) -> c_uint {
        self.image.width()
    }

    fn height(&self) -> c_uint {
        self.image.height()
    }

    fn stride(&self, index: usize) -> c_int {
        self.image.stride(index as u32)
    }

    fn pixel_format<'a>(&'a self) -> PixelFormat<'a> {
        PixelFormat::I420
    }

    fn presentation_time(&self) -> Timestamp {
        self.presentation_time
    }

    fn lock<'a>(&'a self) -> Box<videodecoder::DecodedVideoFrameLockGuard + 'a> {
        Box::new(DecodedVideoFrameLockGuardImpl {
            image: &self.image,
        }) as Box<videodecoder::DecodedVideoFrameLockGuard + 'a>
    }
}

struct DecodedVideoFrameLockGuardImpl<'a> {
    image: &'a VpxImage,
}

impl<'a> videodecoder::DecodedVideoFrameLockGuard for DecodedVideoFrameLockGuardImpl<'a> {
    fn pixels<'b>(&'b self, plane_index: usize) -> &'b [u8] {
        self.image.plane(plane_index as u32)
    }
}

pub const VIDEO_DECODER: videodecoder::RegisteredVideoDecoder =
    videodecoder::RegisteredVideoDecoder {
        id: [ b'V', b'P', b'8', b'0' ],
        constructor: VideoDecoderImpl::new,
    };

#[allow(non_camel_case_types)]
pub mod ffi {
    use libc::{c_char, c_int, c_long, c_uchar, c_uint, c_void};

    pub type vpx_codec_flags_t = c_long;
    pub type vpx_codec_err_t = c_int;
    pub type vpx_codec_iter_t = *mut vpx_codec_iter;
    pub type vpx_color_space_t = c_int;
    pub type vpx_img_fmt_t = c_int;

    #[repr(C)]
    pub struct vpx_codec_ctx_t {
        pub name: *const c_char,
        pub iface: *mut vpx_codec_iface_t,
        pub err: vpx_codec_err_t,
        pub err_detail: *const c_char,
        pub init_flags: vpx_codec_flags_t,
        pub config: *const c_void,
        pub private: *mut vpx_codec_priv_t,
    }

    #[repr(C)]
    pub struct vpx_codec_dec_cfg_t {
        threads: c_uint,
        w: c_uint,
        h: c_uint,
    }

    #[repr(C)]
    pub struct vpx_codec_iter;

    #[repr(C)]
    pub struct vpx_image_t {
        pub fmt: vpx_img_fmt_t,
        pub cs: vpx_color_space_t,

        pub w: c_uint,
        pub h: c_uint,
        pub bit_depth: c_uint,

        pub d_w: c_uint,
        pub d_h: c_uint,

        pub x_chroma_shift: c_uint,
        pub y_chroma_shift: c_uint,

        pub planes: [*mut c_uchar; 4],
        pub stride: [c_int; 4],

        pub bps: c_int,

        pub user_priv: *mut c_void,

        img_data: *const c_uchar,
        img_data_owner: c_int,
        self_allocd: c_int,

        fb_priv: *mut c_void,
    }

    #[repr(C)]
    pub struct vpx_codec_iface_t;
    #[repr(C)]
    pub struct vpx_codec_priv_t;

    pub const VPX_IMAGE_ABI_VERSION: c_int = 3;
    pub const VPX_CODEC_ABI_VERSION: c_int = 2 + VPX_IMAGE_ABI_VERSION;
    pub const VPX_DECODER_ABI_VERSION: c_int = 3 + VPX_CODEC_ABI_VERSION;

    pub const VPX_CODEC_OK: vpx_codec_err_t = 0;

    pub const VPX_IMG_FMT_NONE: vpx_img_fmt_t = 0;
    pub const VPX_IMG_FMT_RGB24: vpx_img_fmt_t = 1;
    pub const VPX_IMG_FMT_RGB32: vpx_img_fmt_t = 2;
    pub const VPX_IMG_FMT_RGB565: vpx_img_fmt_t = 3;
    pub const VPX_IMG_FMT_RGB555: vpx_img_fmt_t = 4;
    pub const VPX_IMG_FMT_UYVY: vpx_img_fmt_t = 5;
    pub const VPX_IMG_FMT_YUY2: vpx_img_fmt_t = 6;
    pub const VPX_IMG_FMT_YVYU: vpx_img_fmt_t = 7;
    pub const VPX_IMG_FMT_BGR24: vpx_img_fmt_t = 8;
    pub const VPX_IMG_FMT_RGB32_LE: vpx_img_fmt_t = 9;
    pub const VPX_IMG_FMT_ARGB: vpx_img_fmt_t = 10;
    pub const VPX_IMG_FMT_ARGB_LE: vpx_img_fmt_t = 11;
    pub const VPX_IMG_FMT_RGB565_LE: vpx_img_fmt_t = 12;
    pub const VPX_IMG_FMT_RGB555_LE: vpx_img_fmt_t = 13;
    pub const VPX_IMG_FMT_YV12: vpx_img_fmt_t = VPX_IMG_FMT_PLANAR | VPX_IMG_FMT_UV_FLIP | 1;
    pub const VPX_IMG_FMT_I420: vpx_img_fmt_t = VPX_IMG_FMT_PLANAR | 2;
    pub const VPX_IMG_FMT_VPXYV12: vpx_img_fmt_t = VPX_IMG_FMT_PLANAR | VPX_IMG_FMT_UV_FLIP | 3;
    pub const VPX_IMG_FMT_VPXI420: vpx_img_fmt_t = VPX_IMG_FMT_PLANAR | 4;
    pub const VPX_IMG_FMT_I422: vpx_img_fmt_t = VPX_IMG_FMT_PLANAR | 5;
    pub const VPX_IMG_FMT_I444: vpx_img_fmt_t = VPX_IMG_FMT_PLANAR | 6;
    pub const VPX_IMG_FMT_I440: vpx_img_fmt_t = VPX_IMG_FMT_PLANAR | 7;
    pub const VPX_IMG_FMT_444A: vpx_img_fmt_t = VPX_IMG_FMT_PLANAR | VPX_IMG_FMT_HAS_ALPHA | 6;
    pub const VPX_IMG_FMT_I42016: vpx_img_fmt_t = VPX_IMG_FMT_I420 | VPX_IMG_FMT_HIGHBITDEPTH;
    pub const VPX_IMG_FMT_I42216: vpx_img_fmt_t = VPX_IMG_FMT_I422 | VPX_IMG_FMT_HIGHBITDEPTH;
    pub const VPX_IMG_FMT_I44416: vpx_img_fmt_t = VPX_IMG_FMT_I444 | VPX_IMG_FMT_HIGHBITDEPTH;
    pub const VPX_IMG_FMT_I44016: vpx_img_fmt_t = VPX_IMG_FMT_I440 | VPX_IMG_FMT_HIGHBITDEPTH;
    pub const VPX_IMG_FMT_PLANAR: vpx_img_fmt_t = 0x100;
    pub const VPX_IMG_FMT_UV_FLIP: vpx_img_fmt_t = 0x200;
    pub const VPX_IMG_FMT_HAS_ALPHA: vpx_img_fmt_t = 0x400;
    pub const VPX_IMG_FMT_HIGHBITDEPTH: vpx_img_fmt_t = 0x800;

    extern {
        pub fn vpx_codec_vp8_dx() -> *mut vpx_codec_iface_t;
        pub fn vpx_codec_dec_init_ver(ctx: *mut vpx_codec_ctx_t,
                                      iface: *mut vpx_codec_iface_t,
                                      cfg: *const vpx_codec_dec_cfg_t,
                                      flags: vpx_codec_flags_t,
                                      ver: c_int)
                                      -> vpx_codec_err_t;
        pub fn vpx_codec_decode(ctx: *mut vpx_codec_ctx_t,
                                data: *const u8,
                                data_sz: c_uint,
                                user_priv: *mut c_void,
                                deadline: c_long)
                                -> vpx_codec_err_t;
        pub fn vpx_codec_get_frame(ctx: *mut vpx_codec_ctx_t, iter: *mut vpx_codec_iter_t)
                                   -> *mut vpx_image_t;
        pub fn vpx_img_free(img: *mut vpx_image_t);
    }
}

