// Copyright 2015 The Servo Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

#![allow(unstable)]
#![feature(unsafe_destructor)]

extern crate core_foundation;
extern crate libc;

pub mod container;
pub mod decoder;
pub mod mkvparser;
pub mod mp4;
pub mod openh264;
pub mod vpxdecoder;

pub mod platform {
    pub mod macos {
        pub mod coremedia;
        pub mod corevideo;
        pub mod videotoolbox;
    }
}

