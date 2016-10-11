// Copyright 2015 The Servo Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

#![feature(alloc, libc, custom_derive, plugin)]
#![feature(heap_api)]
#![plugin(num_macros)]

extern crate alloc;
extern crate byteorder;
extern crate libc;
extern crate num;
extern crate time;

#[cfg(target_os = "macos")]
extern crate core_foundation;

pub mod audiodecoder;
pub mod audioformat;
pub mod container;
pub mod pixelformat;
pub mod playback;
pub mod streaming;
pub mod timing;
pub mod videodecoder;

pub mod codecs {
    pub mod aac;
    pub mod h264;
    pub mod vorbis;
    pub mod vpx;

    #[cfg(feature="ffmpeg")]
    pub mod libavcodec;
}

pub mod containers {
    pub mod gif;
    pub mod mkv;
    pub mod mp4;
    pub mod ogg;
}

pub mod platform {
    #[cfg(target_os="macos")]
    pub mod macos {
        pub mod audiounit;
        pub mod coreaudio;
        pub mod coremedia;
        pub mod corevideo;
        pub mod videotoolbox;
    }
}

