#![feature(exit_status, path_ext)]

extern crate rust_media;
extern crate byteorder;

use std::env;
use std::fs::{self, PathExt, File};
use std::io::{Read, Error};
use std::io::Result as IoResult;
use std::io::ErrorKind::InvalidInput;
use std::path::PathBuf;

use byteorder::{LittleEndian, ByteOrder};

use rust_media::playback::Player;
use rust_media::videodecoder::*;

pub fn main() {
    match do_it() {
        Ok((passed, run)) => {
            println!("{}/{} tests passed", passed, run);
            if passed == run {
                println!("all tests passed");
            } else {
                println!("some tests failed");
                env::set_exit_status(1)
            }
        }
        Err(err) => {
            println!("Unexpected I/O error: {}", err);
            env::set_exit_status(2)
        }
    }
}

fn do_it() -> IoResult<(u32, u32)> {
    let mut passed = 0;
    let mut run = 0;
    'main: for entry in try!(fs::read_dir("tests/gif/")) {
        run += 1;
        // get all the sub directories
        let entry = try!(entry);
        let path = entry.path();
        if path.is_dir() {
            let mut buf = PathBuf::from(&path);
            println!("Running GIF ref-test: {:?}", path);
            buf.push("input.gif");
            let input = Box::new(try!(File::open(&buf)));

            let mut player = Player::new(input, "image/gif");

            let mut outputs = vec![];
            for entry in try!(fs::read_dir(&path)) {
                let entry = try!(entry);
                let file_path = entry.path();
                if file_path.extension().unwrap() != "gif" {
                    outputs.push(file_path);
                }
            }
            outputs.sort();
            for (frame_no, output) in outputs.into_iter().enumerate() {
                let output = try!(File::open(output));
                let ref_frame = try!(parse_tga(output));

                if player.decode_frame().is_err() {
                    println!("Test failed -- could not decode frame {}", frame_no);
                    continue 'main;
                }
                if let Ok(frame) = player.advance() {
                    let frame = frame.video_frame.unwrap();
                    if ref_frame.width != frame.width() || ref_frame.height != frame.height() {
                        println!("Test failed -- incorrect dimensions of frame {}", frame_no);
                        println!("    Expected {}x{}", ref_frame.width, ref_frame.height);
                        println!("       Found {}x{}", frame.width(), frame.height());
                        continue 'main;
                    }

                    if frame.lock().pixels(0) != &*ref_frame.data {
                        println!("Test failed -- pixel data didn't match for frame {}", frame_no);
                        continue 'main;
                    }
                } else {
                    println!("Test failed -- could not advance to frame {}", frame_no);
                    continue 'main;
                }
            }
            passed += 1;
        }
    }
    Ok((passed, run))
}

// RGBA32 format
struct Tga {
    width: u32,
    height: u32,
    data: Vec<u8>,
}

fn parse_tga<R: Read>(mut input: R) -> IoResult<Tga> {
    let mut header = [0; 18];
    try!(read_to_full(&mut input, &mut header));


    // header[2] = 2; // truecolor
    let width = LittleEndian::read_u16(&header[12..14]);
    let height = LittleEndian::read_u16(&header[14..16]);
    let bits_per_pixel = header[16];
    assert!(bits_per_pixel == 24 || bits_per_pixel == 32);

    let mut data = vec![0; height as usize * width as usize * 4];

    // The image data is stored bottom-to-top, left-to-right
    let mut pixel_buf = [0; 4];
    let pixel = &mut pixel_buf[0..bits_per_pixel as usize / 8];
    for y in (0..height).rev() {
        for x in 0..width {
            let idx = (x as usize + y as usize * width as usize) * 4 as usize;
            try!(read_to_full(&mut input, pixel));
            // BGRA -> RGBA (A optional)
            data[idx + 0] = pixel[2];
            data[idx + 1] = pixel[1];
            data[idx + 2] = pixel[0];
            data[idx + 3] = if bits_per_pixel == 32 {
                pixel[3]
            } else {
                0xFF
            };
        }
    }
    Ok(Tga {
        width: width as u32,
        height: height as u32,
        data: data,
    })
}

fn read_to_full<R: Read>(reader: &mut R, buf: &mut [u8]) -> IoResult<()> {
    let mut read = 0;
    loop {
        if read == buf.len() { return Ok(()) }

        let bytes = try!(reader.read(&mut buf[read..]));

        if bytes == 0 { return Err(Error::new(InvalidInput, "Unexpected end of stream")) }

        read += bytes;
    }
}
