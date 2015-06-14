#![feature(exit_status, path_ext)]

extern crate rust_media;
extern crate byteorder;

use std::env;
use std::fs::{self, PathExt, File};
use std::io::{Read, Write, Error};
use std::io::Result as IoResult;
use std::io::ErrorKind::InvalidInput;
use std::path::{Path, PathBuf};

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
        // get all the sub directories
        let entry = try!(entry);
        let dir_path = entry.path();
        if dir_path.is_dir() {
            run += 1;
            let mut buf = PathBuf::from(&dir_path);
            let dir_name = dir_path.file_stem().unwrap().to_str().unwrap();
            println!("Running GIF ref-test: {:?}", dir_name);
            buf.push("input.gif");
            let input = Box::new(try!(File::open(&buf)));

            let mut player = Player::new(input, "image/gif");

            let mut outputs = vec![];
            for entry in try!(fs::read_dir(&dir_path)) {
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
                    println!("  Test failed -- could not decode frame {}", frame_no);
                    continue 'main;
                }
                if let Ok(frame) = player.advance() {
                    let frame = frame.video_frame.unwrap();
                    let width = frame.width();
                    let height = frame.height();
                    let lock = frame.lock();
                    let data = lock.pixels(0);
                    if ref_frame.width != width || ref_frame.height != height {
                        println!("  Test failed -- incorrect dimensions of frame {}", frame_no);
                        println!("    Expected {}x{}", ref_frame.width, ref_frame.height);
                        println!("       Found {}x{}", width, height);
                        continue 'main;
                    }

                    if data.len() != ref_frame.data.len() {
                        println!("  Test failed -- pixel data didn't match for frame {}", frame_no);
                        let mut failure_path = PathBuf::from("tests/failures");
                        failure_path.push(&format!("{}-frame{:05}.tga", dir_name, frame_no));
                        try!(save_tga(width, height, data, &failure_path, ref_frame.id_len));
                        continue 'main;
                    }
                    for i in 0..data.len()/4 {
                        let idx = i * 4;
                        let data_pixel = &data[idx .. idx + 4];
                        let ref_pixel = &ref_frame.data[idx .. idx + 4];
                        // either the pixels are equal, or they're both transparent
                        if (data_pixel != ref_pixel)
                            && (data_pixel[3] != 0 || ref_pixel[3] != 0) {

                            println!("  Test failed -- pixel data didn't match for frame {}", frame_no);
                            let mut failure_path = PathBuf::from("tests/failures");
                            failure_path.push(&format!("{}-frame{:05}.tga", dir_name, frame_no));
                            try!(save_tga(width, height, data, &failure_path, ref_frame.id_len));
                            continue 'main;
                        }
                    }
                    if data != &*ref_frame.data {

                    }
                } else {
                    println!("  Test failed -- could not advance to frame {}", frame_no);
                    continue 'main;
                }
            }
            // If we didn't `continue` then the test passed
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
    id_len: u8,
}

fn parse_tga<R: Read>(mut input: R) -> IoResult<Tga> {
    let mut header = [0; 18];
    try!(read_to_full(&mut input, &mut header));

    let id_field_len = header[0];
    assert!(header[2] == 2, "only truecolor TGA accepted");
    let width = LittleEndian::read_u16(&header[12..14]);
    let height = LittleEndian::read_u16(&header[14..16]);
    let bits_per_pixel = header[16];
    assert!(bits_per_pixel == 24 || bits_per_pixel == 32, "unknown bits-per-pixel val");

    // id_field is junk to skip
    try!(read_to_full(&mut input, &mut vec![0; id_field_len as usize]));

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
        id_len: id_field_len,
    })
}

fn save_tga(width: u32, height: u32, data: &[u8], file_name: &Path, id_len: u8) -> IoResult<()> {
    let mut file = try!(File::create(file_name));

    let mut header = [0; 18];
    header[0] = id_len;
    header[2] = 2; // truecolor
    header[12] = width as u8 & 0xFF;
    header[13] = (width >> 8) as u8 & 0xFF;
    header[14] = height as u8 & 0xFF;
    header[15] = (height >> 8) as u8 & 0xFF;
    header[16] = 32; // bits per pixel

    try!(file.write_all(&header));
    try!(file.write_all(&vec![0x11; id_len as usize]));

    // The image data is stored bottom-to-top, left-to-right
    for y in (0..height).rev() {
        for x in 0..width {
            let idx = (x as usize + y as usize * width as usize) * 4;
            let r = data[idx + 0];
            let g = data[idx + 1];
            let b = data[idx + 2];
            let a = data[idx + 3];
            try!(file.write_all(&[b, g, r, a]));
        }
    }


    // The file footer
    let footer = b"\0\0\0\0\0\0\0\0TRUEVISION-XFILE.\0";

    try!(file.write_all(footer));

    Ok(())
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
