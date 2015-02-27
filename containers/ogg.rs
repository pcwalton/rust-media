// Copyright 2015 The Servo Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

//! Basic support for Ogg, Vorbis audio only at present.
//!
//! TODO(pcwalton): Support video and other codecs.

use libc::{c_char, c_int, c_long};
use std::i32;
use std::mem;
use std::slice;
use std::marker::PhantomData;

pub struct SyncState {
    state: ffi::ogg_sync_state,
}

impl Drop for SyncState {
    fn drop(&mut self) {
        unsafe {
            ffi::ogg_sync_clear(&mut self.state);
        }
    }
}

impl SyncState {
    pub fn new() -> SyncState {
        let mut state;
        unsafe {
            state = mem::uninitialized();
            assert!(ffi::ogg_sync_init(&mut state) == 0)
        }
        SyncState {
            state: state,
        }
    }

    pub fn buffer<'a>(&'a mut self, size: c_long) -> &'a mut [u8] {
        unsafe {
            let ptr = ffi::ogg_sync_buffer(&mut self.state, size);
            assert!(!ptr.is_null());
            mem::transmute::<&mut [c_char],&'a mut [u8]>(slice::from_raw_parts_mut(ptr,
                                                                                 size as usize))
        }
    }

    pub fn wrote(&mut self, bytes: c_long) {
        unsafe {
            assert!(ffi::ogg_sync_wrote(&mut self.state, bytes) == 0)
        }
    }

    pub fn pageout(&mut self) -> Result<Page,c_int> {
        let mut page;
        let result = unsafe {
            page = mem::uninitialized();
            ffi::ogg_sync_pageout(&mut self.state, &mut page)
        };
        if result == 0 {
            Ok(Page {
                page: page,
            })
        } else {
            Err(result)
        }
    }
}

pub struct Page {
    page: ffi::ogg_page,
}

impl Page {
    pub fn serialno(&self) -> c_int {
        unsafe {
            ffi::ogg_page_serialno(&self.page)
        }
    }

    pub fn eos(&self) -> bool {
        unsafe {
            ffi::ogg_page_eos(&self.page) != 0
        }
    }
}

pub struct StreamState {
    state: ffi::ogg_stream_state,
}

impl Drop for StreamState {
    fn drop(&mut self) {
        unsafe {
            assert!(ffi::ogg_stream_clear(&mut self.state) == 0)
        }
    }
}

impl StreamState {
    pub fn new(serialno: c_int) -> StreamState {
        let mut state;
        unsafe {
            state = mem::uninitialized();
            assert!(ffi::ogg_stream_init(&mut state, serialno) == 0)
        }
        StreamState {
            state: state,
        }
    }

    pub fn pagein(&mut self, page: &mut Page) {
        unsafe {
            assert!(ffi::ogg_stream_pagein(&mut self.state, &mut page.page) == 0)
        }
    }

    pub fn packetout(&mut self) -> Packet {
        let mut packet;
        unsafe {
            packet = mem::uninitialized();
            assert!(ffi::ogg_stream_packetout(&mut self.state, &mut packet) == 0)
        }
        Packet {
            packet: packet,
            phantom: PhantomData,
        }
    }
}

pub struct Packet<'a> {
    packet: ffi::ogg_packet,
    phantom: PhantomData<&'a u8>,
}

impl<'a> Packet<'a> {
    pub fn new<'b>(data: &'b [u8], packet_number: i64) -> Packet<'b> {
        // FIXME(pcwalton): b_o_s/e_o_s/granulepos/packetno are guesses; are they right?
        assert!(data.len() <= (i32::MAX as usize));
        let beginning_of_stream = if packet_number == 0 {
            1
        } else {
            0
        };
        Packet {
            packet: ffi::ogg_packet {
                packet: data.as_ptr() as *mut u8,
                bytes: data.len() as c_long,
                b_o_s: beginning_of_stream,
                e_o_s: 0,
                granulepos: 0,
                packetno: packet_number,
            },
            phantom: PhantomData,
        }
    }

    pub fn raw_packet<'b>(&'b mut self) -> &'b mut ffi::ogg_packet {
        &mut self.packet
    }
}

#[allow(missing_copy_implementations)]
pub mod ffi {
    use libc::{c_char, c_int, c_long, c_uchar};

    #[repr(C)]
    pub struct ogg_page {
        pub header: *mut c_uchar,
        pub header_len: c_long,
        pub body: *mut c_uchar,
        pub body_len: c_long,
    }

    #[repr(C)]
    pub struct ogg_stream_state {
        pub body_data: *mut c_uchar,
        pub body_storage: c_long,
        pub body_fill: c_long,
        pub body_returned: c_long,
        pub lacing_vals: *mut c_int,
        pub granule_vals: *mut i64,
        pub lacing_storage: c_long,
        pub lacing_fill: c_long,
        pub lacing_packet: c_long,
        pub lacing_returned: c_long,
        pub header: [c_uchar; 282],
        pub header_fill: c_int,
        pub e_o_s: c_int,
        pub b_o_s: c_int,
        pub serialno: c_long,
        pub pageno: c_long,
        pub packetno: i64,
        pub granulepos: i64,
    }

    #[repr(C)]
    pub struct ogg_packet {
        pub packet: *mut c_uchar,
        pub bytes: c_long,
        pub b_o_s: c_long,
        pub e_o_s: c_long,
        pub granulepos: i64,
        pub packetno: i64,
    }

    #[repr(C)]
    pub struct ogg_sync_state {
        pub data: *mut c_uchar,
        pub storage: c_int,
        pub fill: c_int,
        pub returned: c_int,
        pub unsynced: c_int,
        pub headerbytes: c_int,
        pub bodybytes: c_int,
    }

    #[link(name="rustogg")]
    extern {
        pub fn ogg_sync_init(oy: *mut ogg_sync_state) -> c_int;
        pub fn ogg_sync_clear(oy: *mut ogg_sync_state) -> c_int;

        pub fn ogg_sync_buffer(oy: *mut ogg_sync_state, size: c_long) -> *mut c_char;
        pub fn ogg_sync_wrote(oy: *mut ogg_sync_state, bytes: c_long) -> c_int;
        pub fn ogg_sync_pageout(oy: *mut ogg_sync_state, og: *mut ogg_page) -> c_int;

        pub fn ogg_stream_init(os: *mut ogg_stream_state, serialno: c_int) -> c_int;
        pub fn ogg_stream_clear(os: *mut ogg_stream_state) -> c_int;
        pub fn ogg_stream_pagein(os: *mut ogg_stream_state, og: *mut ogg_page) -> c_int;
        pub fn ogg_stream_packetout(os: *mut ogg_stream_state, op: *mut ogg_packet) -> c_int;

        pub fn ogg_page_serialno(og: *const ogg_page) -> c_int;
        pub fn ogg_page_eos(og: *const ogg_page) -> c_int;
    }
}

