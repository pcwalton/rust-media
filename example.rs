// Copyright 2015 The Servo Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

extern crate "rust-media" as media;

use media::container::{CONTAINER_READERS, TrackType};
use media::decoder::DECODERS;
use std::io::BufferedWriter;
use std::io::fs::File;
use std::iter;
use std::os;

fn main() {
    let input_path = &os::args()[1];
    let output_path = &os::args()[2];

    let reader = CONTAINER_READERS[1].read(&Path::new(input_path.as_slice())).unwrap();
    let mut codec = DECODERS[1].new().unwrap();

    for track_index in 0..reader.track_count() {
        let track = reader.track_by_index(track_index);
        println!("Track {}", track_index);
        println!("  Type: {:?}", track.track_type());
        if track.track_type() == TrackType::Video {
            let video_track = track.as_video_track().unwrap();
            println!("  Width: {}", video_track.width());
            println!("  Height: {}", video_track.height());
            println!("  Frame Rate: {}", video_track.frame_rate());
            println!("  Cluster Count: {}", video_track.cluster_count());

            let headers = video_track.headers();
            codec.set_headers(&*headers,
                              video_track.width() as i32,
                              video_track.height() as i32).unwrap();
        }
    }

    let mut global_frame_index = 0u32;
    for track_index in 0..reader.track_count() {
        let track = reader.track_by_index(track_index);
        if track.track_type() == TrackType::Video {
            let video_track = track.as_video_track().unwrap();
            let (width, height) = (video_track.width(), video_track.height());
            for cluster_index in 0..track.cluster_count() {
                let cluster = video_track.cluster(cluster_index);
                for frame_index in 0..cluster.frame_count() {
                    let frame = cluster.read_frame(frame_index);
                    if frame.track_number() != track.number() {
                        continue
                    }

                    let mut data: Vec<u8> = iter::repeat(0).take(frame.len() as usize).collect();
                    frame.read(data.as_mut_slice()).unwrap();
                    println!("frame {} len={}", frame_index, frame.len());

                    let image = match codec.decode_frame(data.as_mut_slice()) {
                        Ok(image) => image,
                        Err(()) => {
                            println!("failed to decode frame! skipping!");
                            continue
                        }
                    };
                    let lock = image.lock();
                    let (y_data, y_stride) = (lock.pixels(0), image.stride(0) as usize);
                    //let (u_data, u_stride) = (image.plane(1), image.stride(1) as usize);
                    //let (v_data, v_stride) = (image.plane(2), image.stride(2) as usize);
                    let mut rgb_data = Vec::new();
                    for y in 0..(height as usize) {
                        let y_row = y_data.slice(y * y_stride, (y + 1) * y_stride);
                        //let u_row = u_data.slice(y * u_stride, (y + 1) * u_stride);
                        //let v_row = v_data.slice(y * v_stride, (y + 1) * v_stride);
                        for x in 0..(width as usize) {
                            push_pixel(&mut rgb_data,
                                       y_row[x],
                                       0,
                                       0);
                        }
                    }

                    let filename = format!("frame{}.tga", global_frame_index);
                    let path = Path::new(output_path).join(filename.as_slice());
                    let mut output = File::create(&path).unwrap();
                    write_tga(&mut BufferedWriter::new(output),
                              video_track.width() as u32,
                              video_track.height() as u32,
                              rgb_data.as_slice());
                    global_frame_index += 1;
                }
            }
        }
    }
}

struct Rgb {
    r: f64,
    g: f64,
    b: f64,
}

fn yuv_to_rgb(y: f64, u: f64, v: f64) -> Rgb {
    const W_R: f64 = 0.299;
    const W_G: f64 = 1.0 - W_R - W_B;
    const W_B: f64 = 0.114;
    const U_MAX: f64 = 1.0;
    const V_MAX: f64 = 1.0;
    Rgb {
        r: y + v * (1.0 - W_R) / V_MAX,
        g: y - u * (W_B * (1.0 - W_B)) / (U_MAX * W_G) - v * (W_R * (1.0 - W_R)) / (V_MAX * W_G),
        b: y + u * (1.0 - W_B) / U_MAX,
    }
}

fn push_pixel(rgb_data: &mut Vec<u8>, y: u8, u: u8, v: u8) {
    let y = (y as f64) / (0xff as f64);                 // [0, 1]
    //let u = (u as f64) / (0xff as f64) * 2.0 - 1.0;     // [-1, 1]
    //let v = (v as f64) / (0xff as f64) * 2.0 - 1.0;     // [-1, 1]
    let (u, v) = (0.0, 0.0);
    let rgb = yuv_to_rgb(y, u, v);
    rgb_data.push((rgb.r * 255.0) as u8);
    rgb_data.push((rgb.g * 255.0) as u8);
    rgb_data.push((rgb.b * 255.0) as u8);
}

fn write_tga(file: &mut BufferedWriter<File>, width: u32, height: u32, rgb_data: &[u8]) {
    file.write(&[
        0,
        0,
        2,      // uncompressed RGB
        0, 0, 
        0, 0, 
        0,
        0, 0,   // X origin
        0, 0,   // Y origin
        (width & 0xff) as u8,
        ((width >> 8) & 0xff) as u8,
        (height & 0xff) as u8,
        ((height >> 8) & 0xff) as u8,
        24,
        0,
    ]).unwrap();
    for y in (0..(height as usize)).rev() {
        for x in (0..(width as usize)) {
            let color = rgb_data.slice((y * (width as usize) + x) * 3,
                                       (y * (width as usize) + x) * 3 + 3);
            file.write(&[ color[2], color[1], color[0] ]).unwrap();
        }
    }
}

