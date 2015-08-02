// Copyright 2015 The Servo Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

#![allow(non_upper_case_globals)]

use pixelformat::PixelFormat;
use platform::macos::coremedia::{CMTime, OSType};
use timing::Timestamp;
use videodecoder;

use core_foundation::base::{CFRelease, CFRetain, CFTypeID, CFTypeRef, TCFType};
use libc::{c_int, c_uint, c_void, size_t};
use std::mem;
use std::slice;

pub type CVReturn = i32;

pub type CVPixelBufferLockFlags = u64;

pub const kCVPixelBufferLock_ReadOnly: CVPixelBufferLockFlags = 1;

pub struct CVBuffer {
    buffer: ffi::CVBufferRef,
}

impl Drop for CVBuffer {
    fn drop(&mut self) {
        unsafe {
            CFRelease(self.as_CFTypeRef())
        }
    }
}

impl Clone for CVBuffer {
    fn clone(&self) -> CVBuffer {
        unsafe {
            TCFType::wrap_under_get_rule(self.as_concrete_TypeRef())
        }
    }
}

impl TCFType<ffi::CVBufferRef> for CVBuffer {
    fn as_concrete_TypeRef(&self) -> ffi::CVBufferRef {
        self.buffer
    }
    unsafe fn wrap_under_get_rule(buffer: ffi::CVBufferRef) -> CVBuffer {
        TCFType::wrap_under_create_rule(mem::transmute(CFRetain(mem::transmute(buffer))))
    }
    fn as_CFTypeRef(&self) -> CFTypeRef {
        unsafe {
            mem::transmute(self.as_concrete_TypeRef())
        }
    }
    unsafe fn wrap_under_create_rule(buffer: ffi::CVBufferRef) -> CVBuffer {
        CVBuffer {
            buffer: buffer,
        }
    }
    fn type_id() -> CFTypeID {
        unsafe {
            ffi::CVBufferGetTypeID()
        }
    }
}

impl CVBuffer {
    pub fn lock_base_address<'a>(&'a self, flags: CVPixelBufferLockFlags)
                                 -> Result<CVBufferLockGuard<'a>,CVReturn> {
        let err = unsafe {
            ffi::CVPixelBufferLockBaseAddress(self.buffer, flags)
        };
        if err == 0 {
            Ok(CVBufferLockGuard {
                buffer: self,
                flags: flags,
            })
        } else {
            Err(err)
        }
    }

    pub fn is_planar(&self) -> bool {
        unsafe {
            ffi::CVPixelBufferIsPlanar(self.buffer) != 0
        }
    }

    pub fn pixel_format_type(&self) -> OSType {
        unsafe {
            ffi::CVPixelBufferGetPixelFormatType(self.buffer)
        }
    }

    pub fn plane_count(&self) -> size_t {
        unsafe {
            ffi::CVPixelBufferGetPlaneCount(self.buffer)
        }
    }

    pub fn bytes_per_row_of_plane(&self, plane_index: size_t) -> size_t {
        unsafe {
            ffi::CVPixelBufferGetBytesPerRowOfPlane(self.buffer, plane_index)
        }
    }

    pub fn width_of_plane(&self, plane_index: size_t) -> size_t {
        unsafe {
            ffi::CVPixelBufferGetWidthOfPlane(self.buffer, plane_index)
        }
    }

    pub fn height_of_plane(&self, plane_index: size_t) -> size_t {
        unsafe {
            ffi::CVPixelBufferGetHeightOfPlane(self.buffer, plane_index)
        }
    }
}

pub struct CVBufferLockGuard<'a> {
    buffer: &'a CVBuffer,
    flags: CVPixelBufferLockFlags,
}

impl<'a> Drop for CVBufferLockGuard<'a> {
    fn drop(&mut self) {
        unsafe {
            assert!(ffi::CVPixelBufferUnlockBaseAddress(self.buffer.as_concrete_TypeRef(),
                                                        self.flags) == 0);
        }
    }
}

impl<'a> CVBufferLockGuard<'a> {
    pub fn base_address_of_plane(&self, plane_index: size_t) -> &'a [u8] {
        let len = self.buffer.bytes_per_row_of_plane(plane_index) *
            self.buffer.height_of_plane(plane_index);
        unsafe {
            let ptr = ffi::CVPixelBufferGetBaseAddressOfPlane(self.buffer.as_concrete_TypeRef(),
                                                              plane_index);
            mem::transmute::<&mut [c_void],&'a mut [u8]>(slice::from_raw_parts_mut(ptr,
                                                                                   len as usize))
        }
    }
}

pub struct DecodedFrameImpl {
    buffer: CVBuffer,
    presentation_timestamp: CMTime,
}

impl DecodedFrameImpl {
    pub fn new(buffer: CVBuffer, presentation_timestamp: CMTime) -> DecodedFrameImpl {
        DecodedFrameImpl {
            buffer: buffer,
            presentation_timestamp: presentation_timestamp,
        }
    }
}

impl videodecoder::DecodedVideoFrame for DecodedFrameImpl {
    fn width(&self) -> c_uint {
        self.buffer.width_of_plane(0) as c_uint
    }

    fn height(&self) -> c_uint {
        self.buffer.height_of_plane(0) as c_uint
    }

    fn stride(&self, index: usize) -> c_int {
        debug_assert!(self.buffer.is_planar());
        debug_assert!(self.buffer.plane_count() == 2);
        self.buffer.bytes_per_row_of_plane(index as u64) as c_int
    }

    fn pixel_format<'a>(&'a self) -> PixelFormat<'a> {
        PixelFormat::NV12
    }

    fn presentation_time(&self) -> Timestamp {
        self.presentation_timestamp.as_timestamp()
    }

    fn lock<'a>(&'a self) -> Box<videodecoder::DecodedVideoFrameLockGuard + 'a> {
        let guard = self.buffer.lock_base_address(kCVPixelBufferLock_ReadOnly).unwrap();
        Box::new(DecodedVideoFrameLockGuardImpl {
            guard: guard,
        }) as Box<videodecoder::DecodedVideoFrameLockGuard + 'a>
    }
}

struct DecodedVideoFrameLockGuardImpl<'a> {
    guard: CVBufferLockGuard<'a>,
}

impl<'a> videodecoder::DecodedVideoFrameLockGuard for DecodedVideoFrameLockGuardImpl<'a> {
    fn pixels<'b>(&'b self, plane_index: usize) -> &'b [u8] {
        self.guard.base_address_of_plane(plane_index as u64)
    }
}

pub mod ffi {
    use platform::macos::coremedia::OSType;
    use platform::macos::corevideo::CVReturn;

    use core_foundation::base::{Boolean, CFTypeID};
    use libc::{c_void, size_t};

    #[repr(C)]
    struct __CVBuffer;

    pub type CVBufferRef = *mut __CVBuffer;
    pub type CVImageBufferRef = CVBufferRef;
    pub type CVPixelBufferRef = CVImageBufferRef;

    pub type CVOptionFlags = u64;

    #[link(name="CoreVideo", kind="framework")]
    extern {
        pub fn CVBufferGetTypeID() -> CFTypeID;
        pub fn CVPixelBufferLockBaseAddress(pixelBuffer: CVPixelBufferRef,
                                            lockFlags: CVOptionFlags)
                                            -> CVReturn;
        pub fn CVPixelBufferUnlockBaseAddress(pixelBuffer: CVPixelBufferRef,
                                              unlockFlags: CVOptionFlags)
                                              -> CVReturn;
        pub fn CVPixelBufferGetBaseAddressOfPlane(pixelBuffer: CVPixelBufferRef,
                                                  planeIndex: size_t)
                                                  -> *mut c_void;
        pub fn CVPixelBufferIsPlanar(pixelBuffer: CVPixelBufferRef) -> Boolean;
        pub fn CVPixelBufferGetPixelFormatType(pixelBuffer: CVPixelBufferRef) -> OSType;
        pub fn CVPixelBufferGetPlaneCount(pixelBuffer: CVPixelBufferRef) -> size_t;
        pub fn CVPixelBufferGetBytesPerRowOfPlane(pixelBuffer: CVPixelBufferRef,
                                                  planeIndex: size_t)
                                                  -> size_t;
        pub fn CVPixelBufferGetWidthOfPlane(pixelBuffer: CVPixelBufferRef, planeIndex: size_t)
                                            -> size_t;
        pub fn CVPixelBufferGetHeightOfPlane(pixelBuffer: CVPixelBufferRef, planeIndex: size_t)
                                             -> size_t;
    }
}

