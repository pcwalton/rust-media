// Copyright 2015 The Servo Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

#![allow(missing_copy_implementations)]

use audiodecoder;
use platform::macos::coreaudio::{kAudioFormatFlagIsFloat, kAudioFormatFlagIsPacked};
use platform::macos::coreaudio::{kLinearPCMFormatFlagIsNonInterleaved, AudioBuffer};
use platform::macos::coreaudio::{AudioBufferList, AudioBufferListRef, AudioStreamBasicDescription};
use platform::macos::coreaudio::{AudioStreamPacketDescription};
use platform::macos::coremedia::{OSStatus, OSType};

use libc::{c_int, c_void};
use std::iter;
use std::mem;
use std::ptr;
use std::slice;
use std::u32;

#[repr(C)]
pub struct AudioComponentDescription {
    pub component_type: OSType,
    pub component_subtype: OSType,
    pub component_manufacturer: OSType,
    pub component_flags: u32,
    pub component_flags_mask: u32,
}

pub struct AudioComponent {
    component: ffi::AudioComponent,
}

impl AudioComponent {
    pub fn find_next(component: Option<&AudioComponent>, description: &AudioComponentDescription)
                     -> Option<AudioComponent> {
        let component = match component {
            None => ptr::null_mut(),
            Some(component) => component.component,
        };
        let result = unsafe {
            ffi::AudioComponentFindNext(component, description)
        };
        if !result.is_null() {
            Some(AudioComponent {
                component: result
            })
        } else {
            None
        }
    }
}

pub struct AudioCodec {
    codec: ffi::AudioCodec,
}

impl Drop for AudioCodec {
    fn drop(&mut self) {
        unsafe {
            ffi::AudioCodecUninitialize(self.codec);
        }
    }
}

impl AudioCodec {
    pub fn new(component: &AudioComponent) -> Result<AudioCodec,OSStatus> {
        let mut codec = ptr::null_mut();
        let result = unsafe {
            ffi::AudioComponentInstanceNew(component.component, &mut codec)
        };
        if result == 0 {
            Ok(AudioCodec {
                codec: codec,
            })
        } else {
            Err(result)
        }
    }

    pub fn get_property(&self, property: AudioCodecPropertyId)
                        -> Result<AudioCodecProperty,OSStatus> {
        let (mut size, mut writable) = (0, 0);
        let status = unsafe {
            ffi::AudioCodecGetPropertyInfo(self.codec,
                                           property as ffi::AudioCodecPropertyID,
                                           &mut size,
                                           &mut writable)
        };
        if status != 0 {
            return Err(status)
        }
        let mut data: Vec<u8> = iter::repeat(0).take(size as usize).collect();
        let status = unsafe {
            ffi::AudioCodecGetProperty(self.codec,
                                       property as ffi::AudioCodecPropertyID,
                                       &mut size,
                                       data.as_mut_ptr() as *mut c_void)
        };
        if status != 0 {
            return Err(status)
        }
        match property {
            AudioCodecPropertyId::SupportedInputFormats => {
                let mut result = Vec::new();
                unsafe {
                    let mut ptr: *const AudioStreamBasicDescription =
                        mem::transmute(data.as_ptr());
                    let end: *const AudioStreamBasicDescription =
                        mem::transmute(data.as_ptr().offset(data.len() as isize));
                    while ptr < end {
                        result.push(*ptr);
                        ptr = ptr.offset(1);
                    }
                }
                Ok(AudioCodecProperty::SupportedInputFormats(result))
            }
            AudioCodecPropertyId::SupportedOutputFormats => {
                let mut result = Vec::new();
                unsafe {
                    let mut ptr: *const AudioStreamBasicDescription =
                        mem::transmute(data.as_ptr());
                    let end: *const AudioStreamBasicDescription =
                        mem::transmute(data.as_ptr().offset(data.len() as isize));
                    while ptr < end {
                        result.push(*ptr);
                        ptr = ptr.offset(1);
                    }
                }
                Ok(AudioCodecProperty::SupportedOutputFormats(result))
            }
            AudioCodecPropertyId::PacketFrameSize => {
                unsafe {
                    let ptr = mem::transmute::<*const u8,*const u32>(data.as_ptr());
                    Ok(AudioCodecProperty::PacketFrameSize(*ptr))
                }
            }
            AudioCodecPropertyId::MagicCookie => {
                Ok(AudioCodecProperty::MagicCookie(data))
            }
        }
    }

    pub fn set_property(&self, property: AudioCodecProperty) -> Result<(),OSStatus> {
        let (id, data, size) = match property {
            AudioCodecProperty::MagicCookie(ref data) => {
                (AudioCodecPropertyId::MagicCookie, data.as_ptr(), data.len())
            }
            AudioCodecProperty::SupportedInputFormats(_) |
            AudioCodecProperty::SupportedOutputFormats(_) |
            AudioCodecProperty::PacketFrameSize(_) => return Err(-50),
        };
        assert!(size < (u32::MAX as usize));
        let err = unsafe {
            ffi::AudioCodecSetProperty(self.codec, id as u32, size as u32, data as *const c_void)
        };
        if err == 0 {
            Ok(())
        } else {
            Err(err)
        }
    }

    pub fn initialize(&self,
                      input_format: &AudioStreamBasicDescription,
                      output_format: &AudioStreamBasicDescription,
                      magic_cookie: &[u8])
                      -> Result<(),OSStatus> {
        assert!(magic_cookie.len() <= (u32::MAX as usize));
        let result = unsafe {
            ffi::AudioCodecInitialize(self.codec,
                                      input_format,
                                      output_format,
                                      magic_cookie.as_ptr() as *const c_void,
                                      magic_cookie.len() as u32)
        };
        if result == 0 {
            Ok(())
        } else {
            Err(result)
        }
    }

    pub fn append_input_data(&self,
                             input_data: &[u8],
                             packet_description: &[AudioStreamPacketDescription])
                             -> AppendInputDataResult {
        assert!(input_data.len() <= (u32::MAX as usize));
        assert!(packet_description.len() <= (u32::MAX as usize));
        let mut input_data_bytes_consumed = input_data.len() as u32;
        let mut packets_consumed = packet_description.len() as u32;
        let result = unsafe {
            ffi::AudioCodecAppendInputData(self.codec,
                                           input_data.as_ptr() as *const c_void,
                                           &mut input_data_bytes_consumed,
                                           &mut packets_consumed,
                                           packet_description.as_ptr())
        };
        let result = if result == 0 {
            Ok(())
        } else {
            Err(result)
        };
        AppendInputDataResult {
            result: result,
            input_data_bytes_consumed: input_data_bytes_consumed,
            packets_consumed: packets_consumed,
        }
    }

    pub fn append_input_buffer_list(&self,
                                    buffer_list: &AudioBufferListRef,
                                    packet_description: &[AudioStreamPacketDescription])
                                    -> AppendInputDataResult {
        assert!(packet_description.len() <= (u32::MAX as usize));
        let mut input_data_bytes_consumed = 0;
        let mut packets_consumed = packet_description.len() as u32;
        let result = unsafe {
            ffi::AudioCodecAppendInputBufferList(self.codec,
                                                 buffer_list.as_ptr(),
                                                 &mut packets_consumed,
                                                 packet_description.as_ptr(),
                                                 &mut input_data_bytes_consumed)
        };
        let result = if result == 0 {
            Ok(())
        } else {
            Err(result)
        };
        AppendInputDataResult {
            result: result,
            input_data_bytes_consumed: input_data_bytes_consumed,
            packets_consumed: packets_consumed,
        }
    }

    pub fn produce_output_packets(&self, output_data: &mut [u8], mut number_packets: u32)
                                  -> ProduceOutputPacketsResult {
        assert!(output_data.len() <= (u32::MAX as usize));
        let mut output_data_byte_size = output_data.len() as u32;
        let mut status = 0;
        let result = unsafe {
            ffi::AudioCodecProduceOutputPackets(self.codec,
                                                output_data.as_mut_ptr() as *mut c_void,
                                                &mut output_data_byte_size,
                                                &mut number_packets,
                                                ptr::null_mut(),
                                                &mut status)
        };
        let result = if result == 0 {
            Ok(())
        } else {
            Err(result)
        };
        ProduceOutputPacketsResult {
            result: result,
            output_data_byte_size: output_data_byte_size,
            number_packets: number_packets,
            status: status,
        }
    }

    pub fn produce_output_buffer_list(&self,
                                      buffer_list: &mut AudioBufferListRef,
                                      mut number_packets: u32)
                                      -> ProduceOutputBufferListResult {
        let mut status = 0;
        let result = unsafe {
            ffi::AudioCodecProduceOutputBufferList(self.codec,
                                                   (*buffer_list).as_mut_ptr(),
                                                   &mut number_packets,
                                                   ptr::null_mut(),
                                                   &mut status)
        };
        let result = if result == 0 {
            Ok(())
        } else {
            Err(result)
        };
        ProduceOutputBufferListResult {
            result: result,
            number_packets: number_packets,
            status: status,
        }
    }
}

pub enum AudioCodecProperty {
    SupportedInputFormats(Vec<AudioStreamBasicDescription>),
    SupportedOutputFormats(Vec<AudioStreamBasicDescription>),
    PacketFrameSize(u32),
    MagicCookie(Vec<u8>),
}

#[repr(u32)]
#[derive(Copy, Clone)]
pub enum AudioCodecPropertyId {
    SupportedInputFormats =
        ((b'i' as u32) << 24) | ((b'f' as u32) << 16) | ((b'm' as u32) << 8) | (b'#' as u32),
    SupportedOutputFormats =
        ((b'o' as u32) << 24) | ((b'f' as u32) << 16) | ((b'm' as u32) << 8) | (b'#' as u32),
    PacketFrameSize =
        ((b'p' as u32) << 24) | ((b'a' as u32) << 16) | ((b'k' as u32) << 8) | (b'f' as u32),
    MagicCookie =
        ((b'k' as u32) << 24) | ((b'u' as u32) << 16) | ((b'k' as u32) << 8) | (b'i' as u32),
}

pub struct AppendInputDataResult {
    pub result: Result<(),OSStatus>,
    pub input_data_bytes_consumed: u32,
    pub packets_consumed: u32,
}

pub struct ProduceOutputPacketsResult {
    pub result: Result<(),OSStatus>,
    pub output_data_byte_size: u32,
    pub number_packets: u32,
    pub status: u32,
}

pub struct ProduceOutputBufferListResult {
    pub result: Result<(),OSStatus>,
    pub number_packets: u32,
    pub status: u32,
}

// Implementation of the abstract `AudioDecoder` interface

struct AudioDecoderInfoImpl {
    esds_chunk: Vec<u8>,
}

impl AudioDecoderInfoImpl {
    fn new(headers: &audiodecoder::AudioHeaders, _: f64, _: u16)
           -> Box<audiodecoder::AudioDecoderInfo + 'static> {
        let headers = headers.aac_headers().unwrap();
        Box::new(AudioDecoderInfoImpl {
            esds_chunk: headers.esds_chunk.iter().map(|x| *x).collect(),
        }) as Box<audiodecoder::AudioDecoderInfo + 'static>
    }
}

impl audiodecoder::AudioDecoderInfo for AudioDecoderInfoImpl {
    fn create_decoder(mut self: Box<AudioDecoderInfoImpl>)
                      -> Box<audiodecoder::AudioDecoder + 'static> {
        let description = AudioComponentDescription {
            component_type: fourcc(b"adec"),
            component_subtype: fourcc(b"aac "),
            component_manufacturer: 0,
            component_flags: 0,
            component_flags_mask: 0,
        };
        let component = AudioComponent::find_next(None, &description).unwrap();
        let codec = AudioCodec::new(&component).unwrap();
        let mut input_formats =
            if let Ok(AudioCodecProperty::SupportedInputFormats(formats)) =
                    codec.get_property(AudioCodecPropertyId::SupportedInputFormats) {
                formats
            } else {
                panic!("failed to request input formats")
            };
        let mut output_formats =
            if let Ok(AudioCodecProperty::SupportedOutputFormats(formats)) =
                    codec.get_property(AudioCodecPropertyId::SupportedOutputFormats) {
                formats
            } else {
                panic!("failed to request output formats")
            };
        codec.set_property(AudioCodecProperty::MagicCookie(mem::replace(&mut self.esds_chunk,
                                                                        Vec::new()))).unwrap();
        input_formats = input_formats.into_iter().filter(|input_format| {
            input_format.channels_per_frame == 6
        }).collect();
        output_formats = output_formats.into_iter().filter(|output_format| {
            let flags = kAudioFormatFlagIsFloat | kAudioFormatFlagIsPacked |
                kLinearPCMFormatFlagIsNonInterleaved;
            (output_format.format_flags & flags) == flags &&
                output_format.channels_per_frame == 6
        }).collect();
        codec.initialize(&input_formats[0], &output_formats[0], &[]).unwrap();
        Box::new(AudioDecoderImpl {
            codec: codec,
        }) as Box<audiodecoder::AudioDecoder + 'static>
    }
}

pub struct AudioDecoderImpl {
    codec: AudioCodec,
}

impl audiodecoder::AudioDecoder for AudioDecoderImpl {
    fn decode(&mut self, data: &[u8]) -> Result<(),()> {
        let length = data.len();
        assert!(length <= (u32::MAX as usize));
        let data: Vec<u8> = data.iter().map(|x| *x).collect();
        let result = match self.codec.append_input_data(&data, &[
            AudioStreamPacketDescription {
                start_offset: 0,
                variable_frames_in_packet: 0,
                data_byte_size: length as u32,
            },
        ]).result {
            Ok(_) => Ok(()),
            Err(_) => Err(()),
        };
        result
    }

    fn decoded_samples<'a>(&'a mut self)
                           -> Result<Box<audiodecoder::DecodedAudioSamples + 'a>,()> {
        let packet_frame_size =
            if let Ok(AudioCodecProperty::PacketFrameSize(packet_frame_size)) =
                    self.codec.get_property(AudioCodecPropertyId::PacketFrameSize) {
                packet_frame_size
            } else {
                panic!("couldn't get packet frame size")
            };
        let mut output_buffers = [
            AudioBuffer::new(1,
                             iter::repeat(0).take((packet_frame_size as usize) * 4).collect()),
            AudioBuffer::new(1,
                             iter::repeat(0).take((packet_frame_size as usize) * 4).collect()),
            AudioBuffer::new(1,
                             iter::repeat(0).take((packet_frame_size as usize) * 4).collect()),
            AudioBuffer::new(1,
                             iter::repeat(0).take((packet_frame_size as usize) * 4).collect()),
            AudioBuffer::new(1,
                             iter::repeat(0).take((packet_frame_size as usize) * 4).collect()),
            AudioBuffer::new(1,
                             iter::repeat(0).take((packet_frame_size as usize) * 4).collect()),
        ];
        let mut output_buffer_list = AudioBufferList::new(&mut output_buffers);
        let result = self.codec.produce_output_buffer_list(&mut output_buffer_list, 1024);
        if result.result.is_err() {
            return Err(())
        }
        Ok(Box::new(DecodedAudioSamplesImpl {
            output_buffer_list: output_buffer_list,
        }) as Box<audiodecoder::DecodedAudioSamples + 'static>)
    }

    fn acknowledge(&mut self, _: c_int) {}
}

struct DecodedAudioSamplesImpl {
    output_buffer_list: AudioBufferListRef,
}

impl audiodecoder::DecodedAudioSamples for DecodedAudioSamplesImpl {
    fn samples<'a>(&'a self, channel: i32) -> Option<&'a [f32]> {
        let buffer = self.output_buffer_list.buffers()[channel as usize].data();
        unsafe {
            Some(slice::from_raw_parts(buffer.as_ptr() as *const f32,
                                       buffer.len() / 4))
        }
    }
}

fn fourcc(id: &[u8]) -> OSType {
    ((id[0] as u32) << 24) | ((id[1] as u32) << 16) | ((id[2] as u32) << 8) | (id[3] as u32)
}

pub const AUDIO_DECODER: audiodecoder::RegisteredAudioDecoder =
    audiodecoder::RegisteredAudioDecoder {
        id: [ b'a', b'a', b'c', b' ' ],
        constructor: AudioDecoderInfoImpl::new,
    };

#[allow(non_snake_case)]
pub mod ffi {
    use platform::macos::audiounit::AudioComponentDescription;
    use platform::macos::coreaudio::{AudioBufferList, AudioStreamBasicDescription};
    use platform::macos::coreaudio::{AudioStreamPacketDescription};
    use platform::macos::coremedia::OSStatus;

    use core_foundation::base::Boolean;
    use libc::c_void;

    #[repr(C)]
    struct OpaqueAudioComponent;
    #[repr(C)]
    struct OpaqueAudioComponentInstance;

    pub type AudioCodec = AudioComponentInstance;
    pub type AudioComponent = *mut OpaqueAudioComponent;
    pub type AudioComponentInstance = *mut OpaqueAudioComponentInstance;

    pub type AudioCodecPropertyID = u32;

    #[link(name="AudioUnit", kind="framework")]
    extern {
        pub fn AudioCodecInitialize(inCodec: AudioCodec,
                                    inInputFormat: *const AudioStreamBasicDescription,
                                    inOutputFormat: *const AudioStreamBasicDescription,
                                    inMagicCookie: *const c_void,
                                    inMagicCookieByteSize: u32)
                                    -> OSStatus;
        pub fn AudioCodecUninitialize(inCodec: AudioCodec) -> OSStatus;
        pub fn AudioCodecGetPropertyInfo(inCodec: AudioCodec,
                                         inPropertyID: AudioCodecPropertyID,
                                         outSize: *mut u32,
                                         outWritable: *mut Boolean)
                                         -> OSStatus;
        pub fn AudioCodecGetProperty(inCodec: AudioCodec,
                                     inPropertyID: AudioCodecPropertyID,
                                     inPropertyDataSize: *mut u32,
                                     outPropertyData: *mut c_void)
                                     -> OSStatus;
        pub fn AudioCodecSetProperty(inCodec: AudioCodec,
                                     inPropertyID: AudioCodecPropertyID,
                                     inPropertyDataSize: u32,
                                     inPropertyData: *const c_void)
                                     -> OSStatus;
        pub fn AudioCodecAppendInputData(inCodec: AudioCodec,
                                         inInputData: *const c_void,
                                         ioInputDataByteSize: *mut u32,
                                         ioNumberPackets: *mut u32,
                                         inPacketDescription: *const AudioStreamPacketDescription)
                                         -> OSStatus;
        pub fn AudioCodecAppendInputBufferList(inCodec: AudioCodec,
                                               inBufferList: *const AudioBufferList,
                                               ioNumberPackets: *mut u32,
                                               inPacketDescription:
                                                *const AudioStreamPacketDescription,
                                               outBytesConsumed: *mut u32)
                                               -> OSStatus;
        pub fn AudioCodecProduceOutputPackets(inCodec: AudioCodec,
                                              outOutputData: *mut c_void,
                                              ioOutputDataByteSize: *mut u32,
                                              ioNumberPackets: *mut u32,
                                              outPacketDescription:
                                                *mut AudioStreamPacketDescription,
                                              outStatus: *mut u32)
                                              -> OSStatus;
        pub fn AudioCodecProduceOutputBufferList(inCodec: AudioCodec,
                                                 ioBufferList: *mut AudioBufferList,
                                                 ioNumberPackets: *mut u32,
                                                 outPacketDescription:
                                                   *mut AudioStreamPacketDescription,
                                                 outStatus: *mut u32)
                                                 -> OSStatus;
        pub fn AudioComponentFindNext(inComponent: AudioComponent,
                                      inDesc: *const AudioComponentDescription)
                                      -> AudioComponent;
        pub fn AudioComponentInstanceNew(inComponent: AudioComponent,
                                         outInstance: *mut AudioComponentInstance)
                                         -> OSStatus;
    }
}

