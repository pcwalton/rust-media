// Copyright 2015 The Servo Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

#![feature(collections, libc, rustc_private, thread_sleep, duration)]

extern crate clock_ticks;
extern crate libc;
extern crate rust_media as media;
extern crate sdl2;

#[macro_use]
extern crate log;

use media::audioformat::{ConvertAudioFormat, Float32Interleaved, Float32Planar};
use media::container::{AudioTrack, Frame, VideoTrack};
use media::pixelformat::{ConvertPixelFormat, PixelFormat, Rgb24};
use media::playback::Player;
use media::videodecoder::{DecodedVideoFrame, VideoDecoder};
use sdl2::audio::{AudioCallback, AudioDevice, AudioSpecDesired};
use sdl2::event::{Event, WindowEventId};
use sdl2::keycode::KeyCode;
use sdl2::pixels::PixelFormatEnum;
use sdl2::rect::Rect;
use sdl2::render::{Renderer, RendererParent};
use sdl2::render::{Texture, TextureAccess};
use sdl2::video::{Window, WindowBuilder};
use sdl2::init;
use std::cmp;
use std::env;
use std::mem;
use std::fs::File;
use std::thread;
use std::slice;
use std::time::Duration;

struct ExampleMediaPlayer {
    /// A reference timestamp at which playback began.
    playback_start_ticks: i64,
    /// A reference time in nanoseconds at which playback began.
    playback_start_wallclock_time: u64,
}

impl ExampleMediaPlayer {
    fn new() -> ExampleMediaPlayer {
        ExampleMediaPlayer {
            playback_start_ticks: 0,
            playback_start_wallclock_time: clock_ticks::precise_time_ns(),
        }
    }

    fn resync(&mut self, ticks: i64) {
        self.playback_start_ticks = ticks;
        self.playback_start_wallclock_time = clock_ticks::precise_time_ns()
    }

    /// Polls events so we can quit if the user wanted to. Returns true to continue or false to
    /// quit.
    fn poll_events(&mut self, sdl_context: &mut sdl2::Sdl, player: &mut Player) -> bool {
        let mut event_pump = sdl_context.event_pump();

        for event in event_pump.poll_iter() {
            match event {
                Event::Quit {
                    ..
                } | Event::KeyDown {
                    keycode: KeyCode::Escape,
                    ..
                } => {
                    return false
                }
                Event::Window {
                    win_event_id: WindowEventId::Resized,
                    ..
                } => {
                    if let Some(last_frame_time) = player.last_frame_presentation_time() {
                        self.resync(last_frame_time.ticks)
                    }
                }
                _ => {}
            }
        }

        true
    }
}

struct ExampleVideoRenderer<'a> {
    /// The SDL renderer.
    renderer: Renderer<'a>,
    /// The YUV texture we're using.
    texture: Texture,
}

impl<'a> ExampleVideoRenderer<'a> {
    fn new<'b>(renderer: Renderer<'b>, video_format: SdlVideoFormat, video_height: i32)
               -> ExampleVideoRenderer<'b> {
        let texture = renderer.create_texture(video_format.sdl_pixel_format,
                                             TextureAccess::Streaming,
                                             (video_format.sdl_width as i32,
                                              video_height))
                        .ok().expect("Could not create rendered texture");
        ExampleVideoRenderer {
            texture: texture,
            renderer: renderer,
        }
    }

    fn present(&mut self, 
                image: Box<DecodedVideoFrame + 'static>, 
                player: &mut Player, 
                sdl_context: &sdl2::Sdl) {

        let video_track = player.video_track().unwrap();

        let rect = if let &RendererParent::Window(ref window) = self.renderer.get_parent() {
            let (width, height) = window.properties_getters(&sdl_context.event_pump()).get_size();
            Rect::new(0, 0, width, height)
        } else {
            panic!("Renderer parent wasn't a window!")
        };

        self.upload(image, &*video_track);
        let mut drawer = self.renderer.drawer();
        drawer.copy(&self.texture, None, Some(rect));
        drawer.present();
    }

    fn upload(&mut self, image: Box<DecodedVideoFrame + 'static>, video_track: &VideoTrack) {
        drop(self.texture.with_lock(None, |pixels, stride| {
            // FIXME(pcwalton): Workaround for rust-sdl2#331: the pixels array may be too small.
            let output_video_format = SdlVideoFormat::from_video_track(video_track);
            let height = video_track.height() as usize;
            let real_length = match output_video_format.media_pixel_format {
                PixelFormat::I420 => {
                    stride as usize * height + 2 * ((stride / 2) as usize * (height / 2))
                }
                PixelFormat::Rgb24 => stride as usize * height,
                _ => {
                    panic!("SDL can't natively render in {:?}!",
                           output_video_format.media_pixel_format)
                }
            };
            let pixels = unsafe {
                mem::transmute::<&mut [u8],
                                 &mut [u8]>(slice::from_raw_parts_mut(pixels.as_mut_ptr(),
                                                                    real_length))
            };
            upload_image(video_track, &*image, pixels, stride as i32)
        }));
    }
}

/// SDL cannot natively display all pixel formats that `rust-media` supports. Therefore we may have
/// to do pixel format conversion ourselves. This structure contains the mapping from the pixel
/// format of the codec to the nearest matching SDL format.
///
/// Additionally, SDL is buggy with odd (as in, the opposite of even) video widths in some drivers.
/// So we have to store an "SDL width" for each video, which may be different from the real video
/// width. See:
///
///     https://trac.ffmpeg.org/attachment/ticket/1322/0001-ffplay-fix-odd-YUV-width-by-cropping-
///     the-video.patch
///
struct SdlVideoFormat {
    media_pixel_format: PixelFormat<'static>,
    sdl_pixel_format: PixelFormatEnum,
    sdl_width: u16,
}

impl SdlVideoFormat {
    fn from_video_track(video_track: &VideoTrack) -> SdlVideoFormat {
        let (media_pixel_format, sdl_pixel_format) = match video_track.pixel_format() {
            PixelFormat::I420 | PixelFormat::NV12 => (PixelFormat::I420, PixelFormatEnum::IYUV),
            PixelFormat::Indexed(_) | PixelFormat::Rgb24 => {
                (PixelFormat::Rgb24, PixelFormatEnum::RGB24)
            }
        };
        SdlVideoFormat {
            media_pixel_format: media_pixel_format,
            sdl_pixel_format: sdl_pixel_format,
            sdl_width: video_track.width() & !1,
        }
    }
}

pub struct ExampleAudioRenderer {
    samples: Vec<f32>,
    channels: u8,
}

impl AudioCallback for ExampleAudioRenderer {
    type Channel = f32;

    fn callback(&mut self, out: &mut [f32]) {
        if self.samples.len() < out.len() {
            // Zero out the buffer to avoid damaging the listener's eardrums.
            warn!("audio underrun");
            for value in out.iter_mut() {
                *value = 0.0
            }
        }

        let mut leftovers = Vec::new();
        for (i, sample) in mem::replace(&mut self.samples, Vec::new()).into_iter().enumerate() {
            if i < out.len() {
                out[i] = sample
            } else {
                leftovers.push(sample);
            }
        }
        self.samples = leftovers
    }
}

impl ExampleAudioRenderer {
    pub fn new(sample_rate: f64, channels: u16) -> AudioDevice<ExampleAudioRenderer> {
        let desired_spec = AudioSpecDesired {
            freq: Some(sample_rate as i32),
            channels: Some(cmp::min(channels, 2) as u8),
            samples: None,
        };
        AudioDevice::open_playback(None, desired_spec, |spec| {
            ExampleAudioRenderer {
                samples: Vec::new(),
                channels: spec.channels,
            }
        }).unwrap()
    }
}

fn enqueue_audio_samples(device: &mut AudioDevice<ExampleAudioRenderer>,
                         input_samples: &[Vec<f32>]) {
    // Gather up all the channels so we can perform audio format conversion.
    let input_samples: Vec<_> = input_samples.iter()
                                             .take(2)
                                             .map(|samples| &samples[..])
                                             .collect();

    // Make room for the samples in the output buffer.
    let mut output = device.lock();
    let output_channels = output.channels;
    let output_index = output.samples.len();
    let input_sample_count = input_samples[0].len();
    let output_length = input_sample_count * output_channels as usize;
    output.samples.resize(output_index + output_length, 0.0);

    // Perform audio format conversion.
    Float32Planar.convert(&Float32Interleaved,
                          &mut [&mut output.samples[output_index..]],
                          & input_samples,
                          output_channels as usize).unwrap();
}

fn upload_image(video_track: &VideoTrack,
                image: &DecodedVideoFrame,
                output_pixels: &mut [u8],
                output_stride: i32) {
    let height = video_track.height();
    let pixel_format = image.pixel_format();

    // Gather up all the input pixels and strides so we can do pixel format conversion.
    let lock = image.lock();
    let (mut input_pixels, mut input_strides) = (Vec::new(), Vec::new());
    for plane in 0 .. pixel_format.planes() {
        input_pixels.push(lock.pixels(plane));
        input_strides.push(image.stride(plane) as usize);
    }

    // Gather up the output pixels and strides.
    let output_video_format = SdlVideoFormat::from_video_track(&*video_track);
    let (mut output_pixels, output_strides) = match output_video_format.media_pixel_format {
        PixelFormat::I420 => {
            let (output_luma, output_chroma) =
                output_pixels.split_at_mut(output_stride as usize * height as usize);
            let output_chroma_stride = output_stride as usize / 2;
            let (output_u, output_v) =
                output_chroma.split_at_mut(output_chroma_stride as usize * (height / 2) as usize);
            (vec![output_luma, output_u, output_v],
             vec![output_stride as usize, output_chroma_stride, output_chroma_stride])
        }
        PixelFormat::Rgb24 => (vec![output_pixels], vec![output_stride as usize]),
        _ => panic!("SDL can't natively render in {:?}!", output_video_format.media_pixel_format),
    };

    // Perform pixel format conversion.
    pixel_format.convert(&output_video_format.media_pixel_format,
                         &mut output_pixels,
                         & output_strides,
                         & input_pixels,
                         & input_strides,
                         output_video_format.sdl_width as usize,
                         height as usize).unwrap();
}

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() < 3 {
        println!("usage: example path-to-video-or-audio-file mime-type");
        return
    }

    let mut sdl_context = sdl2::init().video().audio().build().ok().expect("Could not start SDL");
    let file = Box::new(File::open(&args[1])
                        .ok().expect("Could not open media file"));

    let mut player = Player::new(file, &args[2]);
    let mut media_player = ExampleMediaPlayer::new();

    let renderer = player.video_track().map(|video_track| {
        let window = WindowBuilder::new(&sdl_context,
                                "rust-media example",
                                 video_track.width() as u32,
                                 video_track.height() as u32)
                                .position_centered()
                                .opengl()
                                .resizable()
                                .build()
                                .ok().expect("Could not create window");
        window.renderer()
              .present_vsync()
              .build()
              .ok().expect("could not render window")
    });
    let mut video_renderer = player.video_track().map(|video_track| {
        let video_format = SdlVideoFormat::from_video_track(&*video_track);
        ExampleVideoRenderer::new(renderer.expect("Could not get renderer"),
                                  video_format,
                                  video_track.height() as i32)
    });

    let mut audio_renderer = player.audio_track().map(|audio_track| {
        let renderer = ExampleAudioRenderer::new(audio_track.sampling_rate(),
                                                 audio_track.channels());
        renderer.resume();
        renderer
    });

    loop {
        if player.decode_frame().is_err() {
            break
        }

        let target_time_since_playback_start = (player.next_frame_presentation_time().unwrap() -
                                                media_player.playback_start_ticks).duration();
        let target_time = duration_from_nanos(media_player.playback_start_wallclock_time)
            + target_time_since_playback_start;
        let cur_time = duration_from_nanos(clock_ticks::precise_time_ns());
        
        if cur_time < target_time {
            thread::sleep(target_time - cur_time);
        }

        let frame = match player.advance() {
            Ok(frame) => frame,
            Err(_) => break,
        };

        if let Some(ref mut video_renderer) = video_renderer {
            video_renderer.present(frame.video_frame.unwrap(), &mut player, &sdl_context);
        }
        if let Some(ref mut audio_renderer) = audio_renderer {
            enqueue_audio_samples(audio_renderer, &frame.audio_samples.unwrap());
        }

        if !media_player.poll_events(&mut sdl_context, &mut player) {
            break
        }
    }
}


fn duration_from_nanos(nanos: u64) -> Duration {
    let secs = nanos / 1_000_000_000;
    let rounded_nanos = secs * 1_000_000_000;
    let extra_nanos = nanos - rounded_nanos;
    Duration::new(secs, extra_nanos as u32)
}
