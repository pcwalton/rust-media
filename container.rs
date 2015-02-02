// Copyright 2015 The Servo Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

use decoder::Headers;
use mkvparser;
use mp4;

use libc::{c_double, c_int, c_long};

pub trait ContainerReader {
    fn track_count(&self) -> u16;
    fn track_by_index<'a>(&'a self, index: u16) -> Box<Track + 'a>;
}

pub trait Track {
    fn track_type(&self) -> TrackType;
    fn cluster_count(&self) -> c_int;
    fn number(&self) -> c_long;
    fn as_video_track<'a>(&'a self) -> Result<Box<VideoTrack + 'a>,()>;
}

pub trait VideoTrack : Track {
    fn width(&self) -> u16;
    fn height(&self) -> u16;
    fn frame_rate(&self) -> c_double;
    fn cluster<'a>(&'a self, cluster_index: i32) -> Box<Cluster + 'a>;
	fn headers(&self) -> Box<Headers>;
}

pub trait Cluster {
    fn frame_count(&self) -> c_int;
    fn read_frame<'a>(&'a self, frame_index: i32) -> Box<Frame + 'a>;
}

pub trait Frame {
    fn len(&self) -> c_long;
    fn read(&self, buffer: &mut [u8]) -> Result<(),()>;
    fn track_number(&self) -> c_long;
}

#[derive(Clone, Copy, PartialEq, Show)]
pub enum TrackType {
    Video,
    Audio,
    Other,
}

#[allow(missing_copy_implementations)]
pub struct RegisteredContainerReader {
    name: &'static str,
    read: extern "Rust" fn(path: &Path) -> Result<Box<ContainerReader + 'static>,()>,
}

impl RegisteredContainerReader {
    pub fn read(&self, path: &Path) -> Result<Box<ContainerReader + 'static>,()> {
        (self.read)(path)
    }
    pub fn name(&self) -> &'static str {
        self.name
    }
}

pub static CONTAINER_READERS: [RegisteredContainerReader; 2] = [
    RegisteredContainerReader {
        name: "Matroska",
        read: mkvparser::ContainerReaderImpl::read,
    },
    RegisteredContainerReader {
        name: "MP4",
        read: mp4::ContainerReaderImpl::read,
    }
];

