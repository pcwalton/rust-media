// Copyright 2015 The Servo Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

use codecs::h264;
use platform::macos::coremedia::{self, CMBlockBuffer, CMFormatDescription, CMSampleBuffer};
use platform::macos::coremedia::{CMSampleTimingInfo, CMTime, OSStatus, kCMVideoCodecType_H264};
use platform::macos::corevideo::{CVBuffer, DecodedFrameImpl};
use platform::macos::corevideo::ffi::CVImageBufferRef;
use timing::Timestamp;
use videodecoder;

use core_foundation::base::{CFRelease, CFRetain, CFTypeID, CFTypeRef, TCFType};
use core_foundation::base::{kCFAllocatorDefault};
use core_foundation::data::CFData;
use core_foundation::dictionary::CFDictionary;
use core_foundation::string::CFString;
use libc::c_void;
use std::cell::RefCell;
use std::mem;
use std::ptr;
use std::rc::Rc;
use std::str::FromStr;

pub type VTDecodeFrameFlags = u32;

pub type VTDecodeInfoFlags = u32;

pub trait VTDecompressionOutputCallback {
    fn call(&mut self,
            status: OSStatus,
            info_flags: VTDecodeInfoFlags,
            image_buffer: &CVBuffer,
            presentation_time_stamp: CMTime,
            presentation_duration: CMTime);
}

pub struct VTDecompressionSession {
    session: ffi::VTDecompressionSessionRef,
}

impl Drop for VTDecompressionSession {
    fn drop(&mut self) {
        unsafe {
            CFRelease(self.as_CFTypeRef())
        }
    }
}

impl TCFType<ffi::VTDecompressionSessionRef> for VTDecompressionSession {
    fn as_concrete_TypeRef(&self) -> ffi::VTDecompressionSessionRef {
        self.session
    }
    unsafe fn wrap_under_get_rule(session: ffi::VTDecompressionSessionRef)
                                  -> VTDecompressionSession {
        TCFType::wrap_under_create_rule(mem::transmute(CFRetain(mem::transmute(session))))
    }
    fn as_CFTypeRef(&self) -> CFTypeRef {
        unsafe {
            mem::transmute(self.as_concrete_TypeRef())
        }
    }
    unsafe fn wrap_under_create_rule(session: ffi::VTDecompressionSessionRef)
                                     -> VTDecompressionSession {
        VTDecompressionSession {
            session: session,
        }
    }
    fn type_id() -> CFTypeID {
        unsafe {
            ffi::VTDecompressionSessionGetTypeID()
        }
    }
}

impl VTDecompressionSession {
    pub fn new(video_format_description: &CMFormatDescription,
               video_decoder_specification: Option<&CFDictionary>,
               destination_image_buffer_attributes: Option<&CFDictionary>,
               output_callback: Box<VTDecompressionOutputCallback>)
               -> Result<VTDecompressionSession,OSStatus> {
        let mut result = ptr::null_mut();
        let video_decoder_specification = match video_decoder_specification {
            None => ptr::null(),
            Some(video_decoder_specification) => video_decoder_specification.as_concrete_TypeRef(),
        };
        let destination_image_buffer_attributes = match destination_image_buffer_attributes {
            None => ptr::null(),
            Some(destination_image_buffer_attributes) => {
                destination_image_buffer_attributes.as_concrete_TypeRef()
            }
        };
        let output_callback = Box::new(output_callback);
        let callback_record = unsafe {
            ffi::VTDecompressionOutputCallbackRecord {
                decompressionOutputCallback: decompression_output_callback,
                decompressionOutputRefCon: mem::transmute::<Box<_>,*mut c_void>(output_callback),
            }
        };
        unsafe {
            let err =
                ffi::VTDecompressionSessionCreate(kCFAllocatorDefault,
                                                  video_format_description.as_concrete_TypeRef(),
                                                  video_decoder_specification,
                                                  destination_image_buffer_attributes,
                                                  &callback_record,
                                                  &mut result);
            if err == 0 {
                Ok(TCFType::wrap_under_create_rule(result))
            } else {
                Err(err)
            }
        }
    }

    pub fn decode_frame(&self, sample_buffer: &CMSampleBuffer, decode_flags: VTDecodeFrameFlags)
                        -> Result<(),OSStatus> {
        let err = unsafe {
            ffi::VTDecompressionSessionDecodeFrame(self.as_concrete_TypeRef(),
                                                   sample_buffer.as_concrete_TypeRef(),
                                                   decode_flags,
                                                   ptr::null_mut(),
                                                   ptr::null_mut())
        };
        if err == 0 {
            Ok(())
        } else {
            Err(err)
        }
    }
}

extern "C" fn decompression_output_callback(decompression_output_ref_con: *mut c_void,
                                            _: *mut c_void,
                                            status: OSStatus,
                                            info_flags: VTDecodeInfoFlags,
                                            image_buffer: CVImageBufferRef,
                                            presentation_time_stamp: CMTime,
                                            presentation_duration: CMTime) {
    unsafe {
        let mut callback: Box<Box<VTDecompressionOutputCallback>> =
            mem::transmute(decompression_output_ref_con);
        callback.call(status,
                      info_flags,
                      &TCFType::wrap_under_get_rule(image_buffer),
                      presentation_time_stamp,
                      presentation_duration);
        mem::forget(callback);
    }
}

// Implementation of the abstract `VideoDecoder` interface

struct VideoDecoderImpl {
    session: VTDecompressionSession,
    format_description: CMFormatDescription,
    output_buffer: Rc<RefCell<Option<DecodedBuffer>>>,
}

impl VideoDecoderImpl {
    fn new(headers: &videodecoder::VideoHeaders, width: i32, height: i32)
           -> Result<Box<videodecoder::VideoDecoder + 'static>,()> {
        // Create the video format description.
        let avcc = h264::create_avcc_chunk(headers);
        let avcc = CFData::from_buffer(&avcc);
        let key: CFString = FromStr::from_str("avcC").unwrap();
        let sample_description_extensions = CFDictionary::from_CFType_pairs(&[
            (key.as_CFType(), avcc.as_CFType())
        ]);
        let extensions = CFDictionary::from_CFType_pairs(&[
            (coremedia::format_description_extension_sample_description_extension_atoms()
                .as_CFType(),
             sample_description_extensions.as_CFType())
        ]);
        let format_description =
            match CMFormatDescription::new_video_format_description(kCMVideoCodecType_H264,
                                                                    width,
                                                                    height,
                                                                    &extensions) {
                Ok(format_description) => format_description,
                Err(_) => return Err(()),
            };

        // Create a decompression session.
        let output_buffer = Rc::new(RefCell::new(None));
        let callback = Box::new(DecoderImplCallback {
            output_buffer: output_buffer.clone(),
        }) as Box<VTDecompressionOutputCallback>;
        match VTDecompressionSession::new(&format_description, None, None, callback) {
            Ok(session) => {
                Ok(Box::new(VideoDecoderImpl {
                    session: session,
                    format_description: format_description,
                    output_buffer: output_buffer,
                }) as Box<videodecoder::VideoDecoder + 'static>)
            }
            Err(_) => Err(()),
        }
    }
}

impl videodecoder::VideoDecoder for VideoDecoderImpl {
    fn decode_frame(&self, data: &[u8], presentation_time: &Timestamp)
                    -> Result<Box<videodecoder::DecodedVideoFrame + 'static>,()> {
        let block_buffer = match CMBlockBuffer::from_memory_block(data.len() as u64) {
            Ok(block_buffer) => block_buffer,
            Err(_) => return Err(()),
        };
        if block_buffer.replace_data_bytes(data, 0).is_err() {
            return Err(())
        }

        let sample_timing_info = CMSampleTimingInfo {
            duration: CMTime::invalid(),
            presentation_time_stamp: CMTime::from_timestamp(presentation_time),
            decode_time_stamp: CMTime::invalid(),
        };

        let sample_buffer = match CMSampleBuffer::new(&block_buffer,
                                                      true,
                                                      &self.format_description,
                                                      1,
                                                      &[sample_timing_info]) {
            Ok(sample_buffer) => sample_buffer,
            Err(_) => return Err(()),
        };

        if self.session.decode_frame(&sample_buffer, 0).is_err() {
            return Err(())
        }
        let output_buffer = self.output_buffer.borrow();
        let output_buffer = output_buffer.as_ref().unwrap();
        if output_buffer.status != 0 {
            return Err(())
        }
        Ok(Box::new(DecodedFrameImpl::new(output_buffer.buffer.clone(),
                                          output_buffer.presentation_timestamp)) as
           Box<videodecoder::DecodedVideoFrame>)
    }
}

struct DecodedBuffer {
    status: OSStatus,
    buffer: CVBuffer,
    presentation_timestamp: CMTime,
}

struct DecoderImplCallback {
    output_buffer: Rc<RefCell<Option<DecodedBuffer>>>,
}

impl VTDecompressionOutputCallback for DecoderImplCallback {
    fn call(&mut self,
            status: OSStatus,
            _: VTDecodeInfoFlags,
            image_buffer: &CVBuffer,
            presentation_timestamp: CMTime,
            _: CMTime) {
        *self.output_buffer.borrow_mut() = Some(DecodedBuffer {
            status: status,
            buffer: (*image_buffer).clone(),
            presentation_timestamp: presentation_timestamp,
        })
    }
}

pub const VIDEO_DECODER: videodecoder::RegisteredVideoDecoder =
    videodecoder::RegisteredVideoDecoder {
        id: [ b'a', b'v', b'c', b' ' ],
        constructor: VideoDecoderImpl::new,
    };

#[allow(non_snake_case)]
pub mod ffi {
    use platform::macos::coremedia::{CMTime, OSStatus};
    use platform::macos::coremedia::ffi::{CMSampleBufferRef, CMVideoFormatDescriptionRef};
    use platform::macos::corevideo::ffi::CVImageBufferRef;
    use platform::macos::videotoolbox::{VTDecodeFrameFlags, VTDecodeInfoFlags};

    use core_foundation::base::{CFAllocatorRef, CFTypeID};
    use core_foundation::dictionary::CFDictionaryRef;
    use libc::c_void;

    #[repr(C)]
    struct OpaqueVTDecompressionSession;

    pub type VTDecompressionSessionRef = *mut OpaqueVTDecompressionSession;

    #[repr(C)]
    #[allow(missing_copy_implementations)]
    pub struct VTDecompressionOutputCallbackRecord {
        pub decompressionOutputCallback: VTDecompressionOutputCallback,
        pub decompressionOutputRefCon: *mut c_void,
    }

    pub type VTDecompressionOutputCallback = extern "C" fn(decompressionOutputRefCon: *mut c_void,
                                                           sourceFrameRefCon: *mut c_void,
                                                           status: OSStatus,
                                                           infoFlags: VTDecodeInfoFlags,
                                                           imageBuffer: CVImageBufferRef,
                                                           presentationTimeStamp: CMTime,
                                                           presentationDuration: CMTime);

    #[link(name="VideoToolbox", kind="framework")]
    extern {
        pub fn VTDecompressionSessionGetTypeID() -> CFTypeID;
        pub fn VTDecompressionSessionCreate(allocator: CFAllocatorRef,
                                            videoFormatDescription: CMVideoFormatDescriptionRef,
                                            videoDecoderSpecification: CFDictionaryRef,
                                            destinationImageBufferAttributes: CFDictionaryRef,
                                            outputCallback: *const
                                                VTDecompressionOutputCallbackRecord,
                                            decompressionSessionOut: *mut
                                                VTDecompressionSessionRef)
                                            -> OSStatus;
        pub fn VTDecompressionSessionDecodeFrame(session: VTDecompressionSessionRef,
                                                 sampleBuffer: CMSampleBufferRef,
                                                 decodeFlags: VTDecodeFrameFlags,
                                                 sourceFrameRefCon: *mut c_void,
                                                 infoFlagsOut: *mut VTDecodeInfoFlags)
                                                 -> OSStatus;
    }
}

