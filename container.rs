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
    fn track_by_index<'a>(&'a self, index: u16) -> Box<Track<'a> + 'a>;
    fn track_by_number<'a>(&'a self, number: c_long) -> Box<Track<'a> + 'a>;

    fn debug(&self, number: c_long) -> String {
        use std::fmt::Write; // Shim for `writeln` being duck-typed during old_io transition

        let track = self.track_by_number(number);

        let mut result = format!("Track {}\n", track.number());
        if let Some(codec) = track.codec() {
            if let Ok(codec) = str::from_utf8(&codec) {
                writeln!(&mut result, "  Codec: {}", codec).unwrap();
            }
        }

        match track.track_type() {
            TrackType::Video(video_track) => {
                writeln!(&mut result, " Type: Video").unwrap();
                writeln!(&mut result, "  Width: {}", video_track.width()).unwrap();
                writeln!(&mut result, "  Height: {}", video_track.height()).unwrap();
                writeln!(&mut result, "  Frame Rate: {}", video_track.frame_rate()).unwrap();
                if let Some(cluster_count) = video_track.cluster_count() {
                    writeln!(&mut result, "  Cluster Count: {}", cluster_count).unwrap();
                }
            }
            TrackType::Audio(audio_track) => {
                writeln!(&mut result, " Type: Audio").unwrap();
                writeln!(&mut result, "  Channels: {}", audio_track.channels()).unwrap();
                writeln!(&mut result, "  Rate: {}", audio_track.sampling_rate()).unwrap();
                if let Some(cluster_count) = audio_track.cluster_count() {
                    writeln!(&mut result, "  Cluster Count: {}", cluster_count).unwrap();
                }
            }
            _ => {}
        }
        result
    }
}

pub trait Track<'x> {
    fn track_type(self: Box<Self>) -> TrackType<'x>;
    fn is_video(&self) -> bool;
    fn is_audio(&self) -> bool;

    /// Returns the number of clusters in this track, if possible. Returns `None` if this container
    /// has no table of contents (so the number of clusters is unknown).
    fn cluster_count(&self) -> Option<c_int>;

    fn number(&self) -> c_long;
    fn codec(&self) -> Option<Vec<u8>>;
    fn cluster<'a>(&'a self, cluster_index: i32) -> Result<Box<Cluster + 'a>,()>;
}

pub trait VideoTrack<'a> : Track<'a> {
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

pub trait AudioTrack<'a> : Track<'a> {
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

pub enum TrackType<'a> {
    Video(Box<VideoTrack<'a> + 'a>),
    Audio(Box<AudioTrack<'a> + 'a>),
    Other(Box<Track<'a> + 'a>),
}

#[allow(missing_copy_implementations)]
pub struct RegisteredContainerReader {
    pub mime_types: &'static [&'static str],
    pub read: extern "Rust" fn(reader: Box<StreamReader>)
                               -> Result<Box<ContainerReader + 'static>,()>,
}

impl RegisteredContainerReader {
    pub fn get(mime_type: &str) -> Result<&'static RegisteredContainerReader,()> {
        for container_reader in &CONTAINER_READERS {
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

