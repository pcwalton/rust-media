// Copyright 2015 The Servo Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

#![allow(non_upper_case_globals)]

use alloc::heap;
use libc::c_void;
use std::mem;
use std::ops::Deref;
use std::ptr;
use std::slice;
use std::u32;

pub const kAudioFormatFlagIsFloat: u32 = (1 << 0);
pub const kAudioFormatFlagIsPacked: u32 = (1 << 3);
pub const kLinearPCMFormatFlagIsNonInterleaved: u32 = (1 << 5);

#[repr(C)]
#[derive(Copy, Clone, Debug)]
pub struct AudioStreamBasicDescription {
    pub sample_rate: f64,
    pub format_id: u32,
    pub format_flags: u32,
    pub bytes_per_packet: u32,
    pub frames_per_packet: u32,
    pub bytes_per_frame: u32,
    pub channels_per_frame: u32,
    pub bits_per_channel: u32,
    pub reserved: u32,
}

#[repr(C)]
#[derive(Copy, Clone)]
pub struct AudioStreamPacketDescription {
    pub start_offset: i64,
    pub variable_frames_in_packet: u32,
    pub data_byte_size: u32,
}

#[repr(C)]
#[allow(missing_copy_implementations)]
#[unsafe_no_drop_flag]
pub struct AudioBuffer {
    number_channels: u32,
    data_byte_size: u32,
    data: *mut c_void,
}

impl Drop for AudioBuffer {
    fn drop(&mut self) {
        // Reform the vector and drop it.
        unsafe {
            drop(Vec::from_raw_parts(self.data as *mut u8,
                                     self.data_byte_size as usize,
                                     self.data_byte_size as usize))
        }
    }
}

impl AudioBuffer {
    pub fn new(number_channels: u32, mut buffer: Vec<u8>) -> AudioBuffer {
        buffer.shrink_to_fit();
        assert!(buffer.len() <= (u32::MAX as usize));
        let result = AudioBuffer {
            number_channels: number_channels,
            data_byte_size: buffer.len() as u32,
            data: buffer.as_mut_ptr() as *mut c_void,
        };
        mem::forget(buffer);
        result
    }

    pub fn number_channels(&self) -> u32 {
        self.number_channels
    }

    pub fn data<'a>(&'a self) -> &'a [u8] {
        unsafe {
            mem::transmute::<&[c_void],
                             &'a [u8]>(slice::from_raw_parts_mut(self.data,
                                                               self.data_byte_size as usize))
        }
    }
}

#[repr(C)]
pub struct AudioBufferList {
    number_buffers: u32,
    // This is actually variable-length...
    buffers: [AudioBuffer; 1],
}

impl AudioBufferList {
    /// Takes ownership of each buffer supplied, replacing each buffer with an empty buffer.
    pub fn new(buffers: &mut [AudioBuffer]) -> AudioBufferListRef {
        assert!(buffers.len() <= (u32::MAX as usize));
        let buffer_list;
        unsafe {
            buffer_list = heap::allocate(AudioBufferList::size(buffers.len() as u32),
                                         mem::min_align_of::<AudioBufferList>())
                as *mut AudioBufferList;
            (*buffer_list).number_buffers = buffers.len() as u32;
            for (i, buffer) in buffers.iter_mut().enumerate() {
                let buffer = mem::replace(buffer, AudioBuffer::new(1, Vec::new()));
                ptr::write((*buffer_list).buffers.as_mut_ptr().offset(i as isize), buffer)
            }
        }
        AudioBufferListRef {
            buffer_list: buffer_list,
        }
    }

    pub fn buffers<'a>(&'a self) -> &'a [AudioBuffer] {
        unsafe {
            slice::from_raw_parts(self.buffers.as_ptr(), self.number_buffers as usize)
        }
    }

    fn size(number_buffers: u32) -> usize {
        mem::size_of::<AudioBufferList>() +
            ((number_buffers - 1) as usize) * mem::size_of::<AudioBuffer>()
    }
}

#[repr(C)]
#[unsafe_no_drop_flag]
pub struct AudioBufferListRef {
    buffer_list: *mut AudioBufferList,
}

impl Drop for AudioBufferListRef {
    fn drop(&mut self) {
        unsafe {
            heap::deallocate(self.buffer_list as *mut u8,
                             AudioBufferList::size((*self.buffer_list).number_buffers),
                             mem::min_align_of::<AudioBufferList>())
        }
    }
}

impl Deref for AudioBufferListRef {
    type Target = AudioBufferList;

    fn deref<'a>(&'a self) -> &'a AudioBufferList {
        unsafe {
            mem::transmute::<&AudioBufferList,&'a AudioBufferList>(&*self.buffer_list)
        }
    }
}

impl AudioBufferListRef {
    pub fn as_ptr(&self) -> *const AudioBufferList {
        self.buffer_list
    }

    pub fn as_mut_ptr(&mut self) -> *mut AudioBufferList {
        self.buffer_list
    }
}

