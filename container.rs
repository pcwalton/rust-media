// Copyright 2015 The Servo Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

use audiodecoder;
use containers::gif;
use containers::mkv;
use containers::mp4;
use pixelformat::PixelFormat;
use streaming::StreamReader;
use timing::Timestamp;
use videodecoder;

use libc::{c_double, c_int, c_long};
use std::str;

pub trait ContainerReader {
    fn track_count(&self) -> u16;
    fn track_by_index<'a>(&'a self, index: u16) -> Box<Track + 'a>;
    fn track_by_number<'a>(&'a self, number: c_long) -> Box<Track + 'a>;
}

pub trait Track {
    fn track_type(&self) -> TrackType;

    /// Returns the number of clusters in this track, if possible. Returns `None` if this container
    /// has no table of contents (so the number of clusters is unknown).
    fn cluster_count(&self) -> Option<c_int>;

    fn number(&self) -> c_long;
    fn codec(&self) -> Option<Vec<u8>>;
    fn cluster<'a>(&'a self, cluster_index: i32) -> Result<Box<Cluster + 'a>,()>;
    fn as_video_track<'a>(&'a self) -> Result<Box<VideoTrack + 'a>,()>;
    fn as_audio_track<'a>(&'a self) -> Result<Box<AudioTrack + 'a>,()>;
}

pub trait VideoTrack : Track {
    /// Returns the width of this track in pixels.
    fn width(&self) -> u16;

    /// Returns the height of this track in pixels.
    fn height(&self) -> u16;

    /// Returns the frame rate of this track in Hertz.
    fn frame_rate(&self) -> c_double;

    /// Returns a the pixel format of this track. This is usually derived from the codec. If the
    /// pixel format is indexed, ignore the associated palette; each frame can have its own
    /// palette, so there can in general be no global per-track palette.
    fn pixel_format(&self) -> PixelFormat<'static>;

    /// Returns codec-specific headers for this track.
	fn headers(&self) -> Box<videodecoder::VideoHeaders>;
}

pub trait AudioTrack : Track {
    fn sampling_rate(&self) -> c_double;
    fn channels(&self) -> u16;
    fn headers(&self) -> Box<audiodecoder::AudioHeaders>;
}

pub trait Cluster {
    /// Reads out a frame from this cluster.
    fn read_frame<'a>(&'a self, frame_index: i32, track_number: c_long)
                      -> Result<Box<Frame + 'a>,()>;
}

pub trait Frame {
    fn len(&self) -> c_long;
    fn read(&self, buffer: &mut [u8]) -> Result<(),()>;
    fn track_number(&self) -> c_long;
    /// Returns the absolute time of this frame.
    fn time(&self) -> Timestamp;
    /// Returns the rendering offset of this frame, in the same time units as `time`.
    fn rendering_offset(&self) -> i64;
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum TrackType {
    Video,
    Audio,
    Other,
}

/// Generic convenience methods for tracks.
pub trait TrackExt {
    fn debug(&self) -> String;
}

impl<'a,'b> TrackExt for &'a (Track + 'b) {
    fn debug(&self) -> String {
        let mut result = format!("Track {}\n", self.number());
        result.push_str(format!("  Type: {:?}\n", self.track_type()).as_slice());
        if let Some(codec) = self.codec() {
            if let Ok(codec) = str::from_utf8(codec.as_slice()) {
                result.push_str(format!("  Codec: {}\n", codec).as_slice());
            }
        }

        match self.track_type() {
            TrackType::Video => {
                let video_track = self.as_video_track().unwrap();
                result.push_str(format!("  Width: {}\n", video_track.width()).as_slice());
                result.push_str(format!("  Height: {}\n", video_track.height()).as_slice());
                result.push_str(format!("  Frame Rate: {}\n",
                                        video_track.frame_rate()).as_slice());
                if let Some(cluster_count) = video_track.cluster_count() {
                    result.push_str(format!("  Cluster Count: {}\n", cluster_count).as_slice());
                }
            }
            TrackType::Audio => {
                let audio_track = self.as_audio_track().unwrap();
                result.push_str(format!("  Channels: {}\n", audio_track.channels()).as_slice());
                result.push_str(format!("  Rate: {}\n", audio_track.sampling_rate()).as_slice());
                if let Some(cluster_count) = audio_track.cluster_count() {
                    result.push_str(format!("  Cluster Count: {}\n", cluster_count).as_slice());
                }
            }
            _ => {}
        }
        result
    }
}

#[allow(missing_copy_implementations)]
pub struct RegisteredContainerReader {
    pub mime_types: &'static [&'static str],
    pub read: extern "Rust" fn(reader: Box<StreamReader>)
                               -> Result<Box<ContainerReader + 'static>,()>,
}

impl RegisteredContainerReader {
    pub fn get(mime_type: &str) -> Result<&'static RegisteredContainerReader,()> {
        for container_reader in CONTAINER_READERS.iter() {
            if container_reader.mime_types.iter().any(|mime| mime == &mime_type) {
                return Ok(container_reader)
            }
        }
        Err(())
    }

    pub fn new(&self, reader: Box<StreamReader>) -> Result<Box<ContainerReader + 'static>,()> {
        (self.read)(reader)
    }

    pub fn mime_types(&self) -> &'static [&'static str] {
        self.mime_types
    }
}

pub static CONTAINER_READERS: [RegisteredContainerReader; 3] = [
    mkv::CONTAINER_READER,
    mp4::CONTAINER_READER,
    gif::CONTAINER_READER,
];

