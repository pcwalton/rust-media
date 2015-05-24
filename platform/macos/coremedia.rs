// Copyright 2015 The Servo Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

#![allow(non_upper_case_globals)]

use timing::Timestamp;

use core_foundation::base::{Boolean, CFRelease, CFRetain, CFTypeID, CFTypeRef, TCFType};
use core_foundation::base::{kCFAllocatorDefault};
use core_foundation::dictionary::CFDictionary;
use core_foundation::string::CFString;
use libc::{c_long, c_void, size_t};
use std::mem;
use std::ptr;

// Here is as good a place as any to put these...
pub type OSStatus = i32;
pub type OSType = u32;

pub type CMItemCount = c_long;
pub type CMItemIndex = c_long;

const kCMTimeFlags_Valid: CMTimeFlags = 1 << 0;

#[repr(C)]
#[derive(Copy, Clone)]
pub struct CMTime {
    pub value: CMTimeValue,
    pub timescale: CMTimeScale,
    pub flags: CMTimeFlags,
    pub epoch: CMTimeEpoch,
}

impl CMTime {
    /// Creates a new "invalid" `CFTime`.
    pub fn invalid() -> CMTime {
        CMTime {
            value: 0,
            timescale: 0,
            flags: 0,
            epoch: 0,
        }
    }

    pub fn from_timestamp(timestamp: &Timestamp) -> CMTime {
        CMTime {
            value: timestamp.ticks,
            timescale: timestamp.ticks_per_second as i32,
            flags: kCMTimeFlags_Valid,
            epoch: 0,
        }
    }

    pub fn as_timestamp(&self) -> Timestamp {
        Timestamp {
            ticks: self.value as i64,
            ticks_per_second: self.timescale as f64,
        }
    }

    pub fn nanoseconds(&self) -> i64 {
        if self.flags == 0 {
            return 0
        }
        (self.value * 1_000_000_000) / (self.timescale as i64)
    }
}

#[repr(C)]
#[derive(Copy, Clone)]
pub struct CMSampleTimingInfo {
    pub duration: CMTime,
    pub presentation_time_stamp: CMTime,
    pub decode_time_stamp: CMTime,
}

pub type CMTimeValue = i64;
pub type CMTimeScale = i32;
pub type CMTimeFlags = u32;
pub type CMTimeEpoch = i64;

pub type CMVideoCodecType = u32;

#[allow(non_upper_case_globals)]
pub const kCMVideoCodecType_H264: CMVideoCodecType =
    ((b'a' as u32) << 24) | ((b'v' as u32) << 16) | ((b'c' as u32) << 8) | (b'1' as u32);

pub struct CMFormatDescription {
    description: ffi::CMFormatDescriptionRef,
}

impl Drop for CMFormatDescription {
    fn drop(&mut self) {
        unsafe {
            CFRelease(self.as_CFTypeRef())
        }
    }
}

impl TCFType<ffi::CMFormatDescriptionRef> for CMFormatDescription {
    fn as_concrete_TypeRef(&self) -> ffi::CMFormatDescriptionRef {
        self.description
    }
    unsafe fn wrap_under_get_rule(description: ffi::CMFormatDescriptionRef)
                                  -> CMFormatDescription {
        TCFType::wrap_under_create_rule(mem::transmute(CFRetain(mem::transmute(description))))
    }
    fn as_CFTypeRef(&self) -> CFTypeRef {
        unsafe {
            mem::transmute(self.as_concrete_TypeRef())
        }
    }
    unsafe fn wrap_under_create_rule(description: ffi::CMFormatDescriptionRef)
                                     -> CMFormatDescription {
        CMFormatDescription {
            description: description,
        }
    }
    fn type_id() -> CFTypeID {
        unsafe {
            ffi::CMFormatDescriptionGetTypeID()
        }
    }
}

impl CMFormatDescription {
    pub fn new_video_format_description(codec_type: CMVideoCodecType,
                                        width: i32,
                                        height: i32,
                                        extensions: &CFDictionary)
                                        -> Result<CMFormatDescription,OSStatus> {
        let mut result = ptr::null_mut();
        let err = unsafe {
            ffi::CMVideoFormatDescriptionCreate(kCFAllocatorDefault,
                                                codec_type,
                                                width,
                                                height,
                                                extensions.as_concrete_TypeRef(),
                                                &mut result)
        };
        if err == 0 {
            unsafe {
                Ok(TCFType::wrap_under_create_rule(result))
            }
        } else {
            Err(err)
        }
    }
}

pub struct CMBlockBuffer {
    buffer: ffi::CMBlockBufferRef,
}

impl Drop for CMBlockBuffer {
    fn drop(&mut self) {
        unsafe {
            CFRelease(self.as_CFTypeRef())
        }
    }
}

impl TCFType<ffi::CMBlockBufferRef> for CMBlockBuffer {
    fn as_concrete_TypeRef(&self) -> ffi::CMBlockBufferRef {
        self.buffer
    }
    unsafe fn wrap_under_get_rule(buffer: ffi::CMBlockBufferRef) -> CMBlockBuffer {
        TCFType::wrap_under_create_rule(mem::transmute(CFRetain(mem::transmute(buffer))))
    }
    fn as_CFTypeRef(&self) -> CFTypeRef {
        unsafe {
            mem::transmute(self.as_concrete_TypeRef())
        }
    }
    unsafe fn wrap_under_create_rule(buffer: ffi::CMBlockBufferRef) -> CMBlockBuffer {
        CMBlockBuffer {
            buffer: buffer,
        }
    }
    fn type_id() -> CFTypeID {
        unsafe {
            ffi::CMBlockBufferGetTypeID()
        }
    }
}

impl CMBlockBuffer {
    pub fn from_memory_block(length: size_t) -> Result<CMBlockBuffer,OSStatus> {
        let mut result = ptr::null_mut();
        let err = unsafe {
            ffi::CMBlockBufferCreateWithMemoryBlock(kCFAllocatorDefault,
                                                    ptr::null_mut(),
                                                    length,
                                                    kCFAllocatorDefault,
                                                    ptr::null(),
                                                    0,
                                                    length,
                                                    0,
                                                    &mut result)
        };
        if err == 0 {
            unsafe {
                Ok(TCFType::wrap_under_create_rule(result))
            }
        } else {
            Err(err)
        }
    }

    pub fn replace_data_bytes(&self, source_bytes: &[u8], offset_into_destination: size_t)
                              -> Result<(),OSStatus> {
        let err = unsafe {
            ffi::CMBlockBufferReplaceDataBytes(source_bytes.as_ptr() as *const c_void,
                                               self.buffer,
                                               offset_into_destination,
                                               source_bytes.len() as u64)
        };
        if err == 0 {
            Ok(())
        } else {
            Err(err)
        }
    }
}

pub struct CMSampleBuffer {
    buffer: ffi::CMSampleBufferRef,
}

impl Drop for CMSampleBuffer {
    fn drop(&mut self) {
        unsafe {
            CFRelease(self.as_CFTypeRef())
        }
    }
}

impl TCFType<ffi::CMSampleBufferRef> for CMSampleBuffer {
    fn as_concrete_TypeRef(&self) -> ffi::CMSampleBufferRef {
        self.buffer
    }
    unsafe fn wrap_under_get_rule(buffer: ffi::CMSampleBufferRef) -> CMSampleBuffer {
        TCFType::wrap_under_create_rule(mem::transmute(CFRetain(mem::transmute(buffer))))
    }
    fn as_CFTypeRef(&self) -> CFTypeRef {
        unsafe {
            mem::transmute(self.as_concrete_TypeRef())
        }
    }
    unsafe fn wrap_under_create_rule(buffer: ffi::CMSampleBufferRef) -> CMSampleBuffer {
        CMSampleBuffer {
            buffer: buffer,
        }
    }
    fn type_id() -> CFTypeID {
        unsafe {
            ffi::CMSampleBufferGetTypeID()
        }
    }
}

impl CMSampleBuffer {
    pub fn new(block_buffer: &CMBlockBuffer,
               data_ready: bool,
               format_description: &CMFormatDescription,
               num_samples: CMItemCount,
               sample_timing_array: &[CMSampleTimingInfo])
               -> Result<CMSampleBuffer,OSStatus> {
        let mut result = ptr::null_mut();
        let err = unsafe {
            ffi::CMSampleBufferCreate(kCFAllocatorDefault,
                                      block_buffer.as_concrete_TypeRef(),
                                      data_ready as Boolean,
                                      mem::transmute(0usize),
                                      ptr::null_mut(),
                                      format_description.as_concrete_TypeRef(),
                                      num_samples,
                                      sample_timing_array.len() as i64,
                                      sample_timing_array.as_ptr(),
                                      0,
                                      ptr::null(),
                                      &mut result)
        };
        if err == 0 {
            unsafe {
                Ok(TCFType::wrap_under_create_rule(result))
            }
        } else {
            Err(err)
        }
    }

    pub fn timing_info(&self, sample_index: CMItemIndex) -> Result<CMSampleTimingInfo,OSStatus> {
        let mut result = CMSampleTimingInfo {
            duration: CMTime::invalid(),
            presentation_time_stamp: CMTime::invalid(),
            decode_time_stamp: CMTime::invalid(),
        };
        let err = unsafe {
            ffi::CMSampleBufferGetSampleTimingInfo(self.buffer, sample_index, &mut result)
        };
        if err == 0 {
            Ok(result)
        } else {
            Err(err)
        }
    }
}

pub fn format_description_extension_sample_description_extension_atoms() -> CFString {
    unsafe {
        TCFType::wrap_under_get_rule(
            ffi::kCMFormatDescriptionExtension_SampleDescriptionExtensionAtoms)
    }
}

#[allow(non_snake_case)]
pub mod ffi {
    use platform::macos::coremedia::{CMItemCount, CMItemIndex, CMSampleTimingInfo};
    use platform::macos::coremedia::{CMVideoCodecType, OSStatus};

    use core_foundation::base::{Boolean, CFAllocatorRef, CFTypeID};
    use core_foundation::dictionary::CFDictionaryRef;
    use core_foundation::string::CFStringRef;
    use libc::{c_void, size_t};

    pub type CMSampleBufferMakeDataReadyCallback =
        extern "C" fn(sbuf: CMSampleBufferRef, makeDataReadyRefcon: *mut c_void)
                      -> OSStatus;

    #[repr(C)]
    #[allow(missing_copy_implementations)]
    pub struct CMBlockBufferCustomBlockSource {
        version: u32,
        AllocateBlock: extern "C" fn(refCon: *mut c_void, sizeInBytes: size_t) -> *mut c_void,
        FreeBlock: extern "C" fn(refCon: *mut c_void,
                                 doomedMemoryBlock: *mut c_void,
                                 sizeInBytes: size_t),
        refCon: *mut c_void,
    }

    #[repr(C)]
    struct OpaqueCMBlockBuffer;
    #[repr(C)]
    struct OpaqueCMFormatDescription;
    #[repr(C)]
    struct OpaqueCMSampleBuffer;

    pub type CMBlockBufferRef = *mut OpaqueCMBlockBuffer;
    pub type CMFormatDescriptionRef = *mut OpaqueCMFormatDescription;
    pub type CMSampleBufferRef = *mut OpaqueCMSampleBuffer;
    pub type CMVideoFormatDescriptionRef = CMFormatDescriptionRef;

    pub type CMBlockBufferFlags = u32;

    #[link(name="CoreMedia", kind="framework")]
    extern {
        pub static kCMFormatDescriptionExtension_SampleDescriptionExtensionAtoms: CFStringRef;

        pub fn CMFormatDescriptionGetTypeID() -> CFTypeID;
        pub fn CMVideoFormatDescriptionCreate(allocator: CFAllocatorRef,
                                              codecType: CMVideoCodecType,
                                              width: i32,
                                              height: i32,
                                              extensions: CFDictionaryRef,
                                              outDesc: *mut CMVideoFormatDescriptionRef)
                                              -> OSStatus;
        pub fn CMBlockBufferGetTypeID() -> CFTypeID;
        pub fn CMBlockBufferCreateWithMemoryBlock(structureAllocator: CFAllocatorRef,
                                                  memoryBlock: *mut c_void,
                                                  blockLength: size_t,
                                                  blockAllocator: CFAllocatorRef,
                                                  customBlockSource: *const
                                                    CMBlockBufferCustomBlockSource,
                                                  offsetToData: size_t,
                                                  dataLength: size_t,
                                                  flags: CMBlockBufferFlags,
                                                  newBBufOut: *mut CMBlockBufferRef)
                                                  -> OSStatus;
        pub fn CMBlockBufferReplaceDataBytes(sourceBytes: *const c_void,
                                             destinationBuffer: CMBlockBufferRef,
                                             offsetIntoDestination: size_t,
                                             dataLength: size_t)
                                             -> OSStatus;
        pub fn CMSampleBufferGetTypeID() -> CFTypeID;
        pub fn CMSampleBufferCreate(allocator: CFAllocatorRef,
                                    dataBuffer: CMBlockBufferRef,
                                    dataReady: Boolean,
                                    makeDataReadyCallback: CMSampleBufferMakeDataReadyCallback,
                                    makeDataReadyRefcon: *mut c_void,
                                    formatDescription: CMFormatDescriptionRef,
                                    numSamples: CMItemCount,
                                    numSampleTimingEntries: CMItemCount,
                                    sampleTimingArray: *const CMSampleTimingInfo,
                                    numSampleSizeEntries: CMItemCount,
                                    sampleSizeArray: *const size_t,
                                    sBufOut: *mut CMSampleBufferRef)
                                    -> OSStatus;
        pub fn CMSampleBufferGetSampleTimingInfo(sbuf: CMSampleBufferRef,
                                                 sampleIndex: CMItemIndex,
                                                 timingInfoOut: *mut CMSampleTimingInfo)
                                                 -> OSStatus;
    }
}

