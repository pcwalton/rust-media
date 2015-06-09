// Copyright 2015 The Servo Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

use audiodecoder::{AudioDecoder, AudioDecoderInfo, RegisteredAudioDecoder};
use container::{AudioTrack, ContainerReader, Frame, RegisteredContainerReader, Track, TrackType};
use container::{VideoTrack};
use streaming::StreamReader;
use timing::Timestamp;
use videodecoder::{DecodedVideoFrame, RegisteredVideoDecoder, VideoDecoder};

use libc::{c_int, c_long};
use std::iter;
use std::mem;
use std::marker::PhantomData;

/// A simple video/audio player.
pub struct Player<'a> {
    /// The container.
    pub reader: Box<ContainerReader + 'static>,
    /// Information about the video track that's playing.
    video: Option<VideoPlayerInfo>,
    /// Information about the audio track that's playing.
    audio: Option<AudioPlayerInfo>,
    /// The index of the current cluster.
    cluster_index: i32,
    /// The calculated delay between video frames (if the track contains video) or audio frames (if
    /// the track contains only audio).
    frame_delay: Option<i64>,
    /// The time at which the last frame was played.
    last_frame_presentation_time: Option<Timestamp>,
    /// The time at which the next frame is to be played.
    next_frame_presentation_time: Option<Timestamp>,
    phantom: PhantomData<&'a u8>,
}

impl<'a> Player<'a> {
    pub fn new<'b>(reader: Box<StreamReader>, mime_type: &str) -> Player<'b> {
        let mut reader = RegisteredContainerReader::get(mime_type).unwrap()
                                                                  .new(reader)
                                                                  .unwrap();

        let (video_player_info, audio_player_info) = {
            let (video_codec, audio_codec) =
                read_track_metadata_and_initialize_codecs(&mut *reader);

            let (mut video_track, mut audio_track) = (None, None);
            for track_index in 0..reader.track_count() {
                let track = reader.track_by_index(track_index);
                if track.is_video() && video_track.is_none() {
                    video_track = Some(track)
                } else if track.is_audio() && audio_track.is_none() {
                    audio_track = Some(track)
                }
            }

            (video_track.map(|video_track| {
                VideoPlayerInfo {
                    codec: video_codec.unwrap(),
                    track_number: video_track.number() as i64,
                    frames: Vec::new(),
                    frame_index: 0,
                }
            }), audio_track.map(|audio_track| {
                AudioPlayerInfo {
                    codec: audio_codec.unwrap(),
                    track_number: audio_track.number() as i64,
                    samples: None,
                    frame_index: 0,
                }
            }))
        };

        Player {
            reader: reader,
            video: video_player_info,
            audio: audio_player_info,
            cluster_index: 0,
            frame_delay: None,
            last_frame_presentation_time: None,
            next_frame_presentation_time: None,
            phantom: PhantomData,
        }
    }

    pub fn decode_frame(&mut self) -> Result<(),()> {
        // This code abuses Box's magic ownership to get audio and video tracks
        // without borrowing self. This is why we just inline the code from those
        // methods.

        let reader = &mut *self.reader;

        let video_track = self.video.as_ref().map(|video| {
            let number = video.track_number;
            if let TrackType::Video(track) = reader.track_by_number(number).track_type() {
                track
            } else {
                unreachable!()
            }
        });

        let audio_track = self.audio.as_ref().map(|audio| {
            let number = audio.track_number;
            if let TrackType::Audio(track) = reader.track_by_number(number).track_type() {
                track
            } else {
                unreachable!()
            }
        });

        'clusterloop: loop {
            let cluster = match (&video_track, &audio_track) {
                (&Some(ref video_track), _) => {
                    match video_track.cluster(self.cluster_index) {
                        Ok(cluster) => cluster,
                        Err(_) => return Err(()),
                    }
                }
                (&None, &Some(ref audio_track)) => {
                    match audio_track.cluster(self.cluster_index) {
                        Ok(cluster) => cluster,
                        Err(_) => return Err(()),
                    }
                }
                (&None, &None) => return Err(()),
            };

            // Read the video frame or frames.
            if let Some(ref mut video) = self.video {
                loop {
                    match self.frame_delay {
                        None => {
                            if !video.frames.is_empty() {
                                break
                            }
                        }
                        Some(frame_delay) => {
                            let last_frame_time = self.last_frame_presentation_time.unwrap();
                            if video.frames.iter().any(|frame| {
                                let last_frame_time = last_frame_time.ticks;
                                let delta = frame.presentation_time().ticks - (last_frame_time +
                                                                               frame_delay);
                                let is_next_frame = delta.abs() < 5;
                                let is_in_far_future = delta > 1000;
                                is_next_frame || is_in_far_future
                            }) {
                                break
                            }
                        }
                    }

                    // Read a video frame.
                    match cluster.read_frame(video.frame_index, video.track_number as c_long) {
                        Ok(frame) => {
                            decode_video_frame(&mut *video.codec, &*frame, &mut video.frames)
                        }
                        Err(_) => {
                            self.cluster_index += 1;
                            video.frame_index = 0;
                            if let Some(ref mut audio) = self.audio {
                                audio.frame_index = 0
                            }
                            continue 'clusterloop
                        }
                    }

                    video.frame_index += 1;

                    // Throw out any video frames that are too late. (This might include the one we
                    // just decoded!)
                    if let Some(last_frame_time) = self.last_frame_presentation_time {
                        let mut i = 0;
                        while i < video.frames.len() {
                            let frame_time = video.frames[i].presentation_time();
                            if last_frame_time.ticks <= frame_time.ticks {
                                i += 1
                            } else {
                                video.frames.remove(i);
                            }
                        }
                    }
                }

                // Determine when the video frame is to be shown.
                self.next_frame_presentation_time =
                    match video.frames.iter().min_by(|frame| frame.presentation_time().ticks) {
                        None => continue,
                        Some(frame) => Some(frame.presentation_time()),
                    };
            }

            // Read the audio frame or frames.
            if let Some(ref mut audio) = self.audio {
                audio.samples = Some(iter::repeat(Vec::new()).take(audio_track.as_ref()
                                                                              .unwrap()
                                                                              .channels() as usize)
                                                             .collect());
                loop {
                    let frame = match cluster.read_frame(audio.frame_index,
                                                         audio.track_number as c_long) {
                        Err(_) => break,
                        Ok(frame) => frame,
                    };
                    decode_audio_frame(&mut *audio.codec,
                                       &*frame,
                                       &mut audio.samples.as_mut().unwrap());
                    audio.frame_index += 1;

                    // If there is a video track, we synchronize to it. Otherwise, read just one
                    // audio frame.
                    if self.video.is_some() {
                        if frame.time().duration() >=
                                self.next_frame_presentation_time.unwrap().duration() {
                            break
                        }
                    } else {
                        self.next_frame_presentation_time = Some(frame.time());
                        break
                    }
                }
            }

            return Ok(())
        }
    }

    pub fn video_track<'b>(&'b self) -> Option<Box<VideoTrack + 'b>> {
        self.video.as_ref().map(|video| {
            let number = video.track_number;
            if let TrackType::Video(track) = self.reader.track_by_number(number).track_type() {
                track
            } else {
                unreachable!()
            }
        })
    }

    pub fn audio_track<'b>(&'b self) -> Option<Box<AudioTrack + 'b>> {
        self.audio.as_ref().map(|audio| {
            let number = audio.track_number;
            if let TrackType::Audio(track) = self.reader.track_by_number(number).track_type() {
                track
            } else {
                unreachable!()
            }
        })
    }

    /// Returns the presentation time of the last frame, relative to the start of playback.
    pub fn last_frame_presentation_time(&self) -> Option<Timestamp> {
        self.last_frame_presentation_time
    }

    /// Returns the presentation time of the next frame, relative to the start of playback.
    pub fn next_frame_presentation_time(&self) -> Option<Timestamp> {
        self.next_frame_presentation_time
    }

    /// Retrieves the decoded frame data and advances to the next frame.
    pub fn advance(&mut self) -> Result<DecodedFrame,()> {
        // Determine the frame delay, if possible.
        if let Some(last_frame_time) = self.last_frame_presentation_time {
            self.frame_delay = Some(self.next_frame_presentation_time.unwrap().ticks -
                                    last_frame_time.ticks);
        }

        // Record the current time.
        self.last_frame_presentation_time = self.next_frame_presentation_time;

        // Determine which video frame to show.
        let index = match self.video {
            Some(ref mut video) => {
                match video.frames
                           .iter()
                           .enumerate()
                           .min_by(|&(_, frame)| frame.presentation_time().ticks) {
                    None => return Err(()),
                    Some((index, _)) => Some(index),
                }
            }
            None => None,
        };

        // Extract and return the frame.
        Ok(DecodedFrame {
            video_frame: self.video.as_mut().map(|video| {
                video.frames.remove(index.unwrap())
            }),
            audio_samples: self.audio.as_mut().map(|audio| {
                mem::replace(&mut audio.samples, None).unwrap()
            })
        })
    }
}

/// Information about a playing video track.
struct VideoPlayerInfo {
    /// The video codec.
    codec: Box<VideoDecoder + 'static>,
    /// The number of the video track.
    track_number: i64,
    /// Buffered video frames to be displayed.
    frames: Vec<Box<DecodedVideoFrame + 'static>>,
    /// The index of the current frame.
    frame_index: i32,
}

/// Information about a playing audio track.
struct AudioPlayerInfo {
    /// The audio codec.
    codec: Box<AudioDecoder + 'static>,
    /// The number of the audio track.
    track_number: i64,
    /// Buffered audio samples to be played, in planar format.
    samples: Option<Vec<Vec<f32>>>,
    /// The index of the current frame.
    frame_index: i32,
}

pub struct DecodedFrame {
    pub video_frame: Option<Box<DecodedVideoFrame + 'static>>,
    pub audio_samples: Option<Vec<Vec<f32>>>,
}

fn read_track_metadata_and_initialize_codecs(reader: &mut ContainerReader)
                                             -> (Option<Box<VideoDecoder + 'static>>,
                                                 Option<Box<AudioDecoder + 'static>>) {
    let (mut video_codec, mut audio_codec) = (None, None);
    for track_index in 0..reader.track_count() {
        let track = reader.track_by_index(track_index);
        match track.track_type() {
            TrackType::Video(video_track) => {
                if let Some(codec) = video_track.codec() {
                    let headers = video_track.headers();
                    video_codec = Some(RegisteredVideoDecoder::get(&codec).unwrap().new(
                            &*headers,
                            video_track.width() as i32,
                            video_track.height() as i32).unwrap());
                }
            }
            TrackType::Audio(audio_track) => {
                if let Some(codec) = audio_track.codec() {
                    let headers = audio_track.headers();
                    let info = RegisteredAudioDecoder::get(&codec).unwrap().new(
                            &*headers,
                            audio_track.sampling_rate(),
                            audio_track.channels());
                    audio_codec = Some(info.create_decoder());
                }
            }
            _ => {}
        }
    }
    (video_codec, audio_codec)
}

fn decode_video_frame(codec: &mut VideoDecoder,
                      frame: &Frame,
                      frames: &mut Vec<Box<DecodedVideoFrame + 'static>>) {
    let mut data = Vec::new();
    data.resize(frame.len() as usize, 0u8);
    frame.read(&mut data).unwrap();

    let frame_presentation_time = frame.time() + frame.rendering_offset();
    if let Ok(image) = codec.decode_frame(&mut data, &frame_presentation_time) {
        frames.push(image)
    }
}

fn decode_audio_frame(codec: &mut AudioDecoder, frame: &Frame, samples: &mut [Vec<f32>]) {
    let mut data: Vec<u8> = iter::repeat(0).take(frame.len() as usize).collect();
    frame.read(&mut data).unwrap();
    if codec.decode(&data).is_err() {
        return
    }

    let sample_count = match codec.decoded_samples() {
        Ok(pcm_output) => {
            for channel in 0 .. samples.len() as i32 {
                samples[channel as usize].push_all(pcm_output.samples(channel).unwrap())
            }
            pcm_output.samples(0).unwrap().len()
        }
        Err(_) => return,
    };

    codec.acknowledge(sample_count as c_int);
}

