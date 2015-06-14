// Copyright 2015 The Servo Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

//! (Animated) GIF support.
//!
//! GIF is treated as both a container and a video codec.

#![allow(non_snake_case)]

use container;
use pixelformat::PixelFormat;
use streaming::StreamReader;
use timing::Timestamp;
use videodecoder;

use libc::{c_int, c_long, c_uint};
use std::cell::RefCell;
use std::io::{Read, Error};
use std::io::Result as IoResult;
use std::io::ErrorKind::InvalidInput;

pub struct ContainerReaderImpl {
    gif: RefCell<Gif>,
}

impl ContainerReaderImpl {
    pub fn new(reader: Box<StreamReader>) -> Result<Box<container::ContainerReader + 'static>,()> {
        let gif = match Gif::new(reader) {
            Ok(gif) => gif,
            _ => return Err(()),
        };

        Ok(Box::new(ContainerReaderImpl {
            gif: RefCell::new(gif),
        }))
    }
}

impl container::ContainerReader for ContainerReaderImpl {
    fn track_count(&self) -> u16 {
        1
    }
    fn track_by_index<'a>(&'a self, _: u16) -> Box<container::Track + 'a> {
        Box::new(VideoTrackImpl {
            gif: &self.gif,
        })
    }
    fn track_by_number<'a>(&'a self, _: c_long) -> Box<container::Track + 'a> {
        self.track_by_index(0)
    }
}

struct VideoTrackImpl<'a> {
    gif: &'a RefCell<Gif>,
}

impl<'a> container::Track<'a> for VideoTrackImpl<'a> {
    fn track_type(self: Box<Self>) -> container::TrackType<'a> {
        container::TrackType::Video(self)
    }

    fn is_video(&self) -> bool { true }

    fn codec(&self) -> Option<Vec<u8>> {
        Some(b"GIFf".to_vec())
    }

    fn cluster<'b>(&'b self, cluster_index: i32) -> Result<Box<container::Cluster + 'b>,()> {
        // Read and decode frames until we get to the given cluster index.
        while self.gif.borrow().frames.len() < (cluster_index as usize + 1) {
            match self.gif.borrow_mut().parse_next_frame() {
                Err(_) | Ok(false) => return Err(()),
                Ok(true) => {}
            }
        }

        Ok(Box::new(ClusterImpl {
            gif: self.gif,
            frame_index: cluster_index as usize,
        }))
    }
}

impl<'a> container::VideoTrack<'a> for VideoTrackImpl<'a> {
    fn width(&self) -> u16 {
        self.gif.borrow().width as u16
    }

    fn height(&self) -> u16 {
        self.gif.borrow().height as u16
    }

    fn pixel_format(&self) -> PixelFormat<'static> {
        PIXEL_FORMAT
    }

    fn num_iterations(&self) -> u32 {
        self.gif.borrow().num_iterations as u32
    }

    fn headers(&self) -> Box<videodecoder::VideoHeaders> {
        Box::new(videodecoder::EmptyVideoHeadersImpl)
    }
}

struct ClusterImpl<'a> {
    gif: &'a RefCell<Gif>,
    frame_index: usize,
}

impl<'a> container::Cluster for ClusterImpl<'a> {
    fn read_frame<'b>(&'b self, frame_index: i32, _: c_long)
                  -> Result<Box<container::Frame + 'b>,()> {
        if frame_index == 0 {
            Ok(Box::new(FrameImpl {
                gif: self.gif,
                frame_index: self.frame_index,
            }) as Box<container::Frame + 'b>)
        } else {
            Err(())
        }
    }
}

struct FrameImpl<'a> {
    gif: &'a RefCell<Gif>,
    frame_index: usize,
}

impl<'a> container::Frame for FrameImpl<'a> {
    fn len(&self) -> c_long {
        self.gif.borrow().frames[self.frame_index].data.len() as c_long
    }

    fn read(&self, buffer: &mut [u8]) -> Result<(),()> {
        ::std::slice::bytes::copy_memory(&self.gif.borrow().frames[self.frame_index].data,
                                         buffer);
        Ok(())
    }

    fn track_number(&self) -> c_long {
        0
    }

    fn time(&self) -> Timestamp {
        let time = self.gif.borrow().frames[self.frame_index].time;
        Timestamp {
            ticks: time as i64,
            ticks_per_second: 100.0,
        }
    }

    fn rendering_offset(&self) -> i64 {
        0
    }
}

pub const CONTAINER_READER: container::RegisteredContainerReader =
    container::RegisteredContainerReader {
        mime_types: &["image/gif"],
        read: ContainerReaderImpl::new,
    };

// Implementation of the abstract `VideoDecoder` interface

#[allow(missing_copy_implementations)]
struct VideoDecoderImpl {
    width: u32,
    height: u32,
}

impl VideoDecoderImpl {
    fn new(_: &videodecoder::VideoHeaders, width: i32, height: i32)
           -> Result<Box<videodecoder::VideoDecoder + 'static>,()> {
        Ok(Box::new(VideoDecoderImpl {
            width: width as u32,
            height: height as u32,
        }))
    }
}

impl videodecoder::VideoDecoder for VideoDecoderImpl {
    fn decode_frame(&self, data: &[u8], presentation_time: &Timestamp)
                    -> Result<Box<videodecoder::DecodedVideoFrame + 'static>,()> {
        Ok(Box::new(DecodedVideoFrameImpl {
            width: self.width,
            height: self.height,
            pixels: data.to_vec(),
            presentation_time: *presentation_time,
        }))
    }
}

struct DecodedVideoFrameImpl {
    width: u32,
    height: u32,
    pixels: Vec<u8>,
    presentation_time: Timestamp,
}

impl videodecoder::DecodedVideoFrame for DecodedVideoFrameImpl {
    fn width(&self) -> c_uint {
        self.width
    }

    fn height(&self) -> c_uint {
        self.height
    }

    fn stride(&self, _: usize) -> c_int {
        (BYTES_PER_COL * self.width as usize) as c_int
    }

    fn pixel_format<'a>(&'a self) -> PixelFormat<'a> {
        PIXEL_FORMAT
    }

    fn presentation_time(&self) -> Timestamp {
        self.presentation_time
    }

    fn lock<'a>(&'a self) -> Box<videodecoder::DecodedVideoFrameLockGuard + 'a> {
        Box::new(DecodedVideoFrameLockGuardImpl {
            pixels: &self.pixels,
        }) as Box<videodecoder::DecodedVideoFrameLockGuard + 'a>
    }
}

struct DecodedVideoFrameLockGuardImpl<'a> {
    pixels: &'a [u8],
}

impl<'a> videodecoder::DecodedVideoFrameLockGuard for DecodedVideoFrameLockGuardImpl<'a> {
    fn pixels<'b>(&'b self, _: usize) -> &'b [u8] {
        self.pixels
    }
}

pub const VIDEO_DECODER: videodecoder::RegisteredVideoDecoder =
    videodecoder::RegisteredVideoDecoder {
        id: *b"GIFf",
        constructor: VideoDecoderImpl::new,
    };










const HEADER_LEN: usize = 6;
const GLOBAL_DESCRIPTOR_LEN: usize = 7;
const LOCAL_DESCRIPTOR_LEN: usize = 9;
const GRAPHICS_EXTENSION_LEN: usize = 6;
const APPLICATION_EXTENSION_LEN: usize = 17;
const MAX_COLOR_TABLE_SIZE: usize = 3 * 256; // 2^8 RGB colours

const BLOCK_EOF: u8       = 0x3B;
const BLOCK_EXTENSION: u8 = 0x21;
const BLOCK_IMAGE: u8     = 0x2C;

const EXTENSION_PLAIN: u8       = 0x01;
const EXTENSION_COMMENT: u8     = 0xFE;
const EXTENSION_GRAPHICS: u8    = 0xF9;
const EXTENSION_APPLICATION: u8 = 0xFF;

const DISPOSAL_UNSPECIFIED: u8 = 0;
const DISPOSAL_CURRENT: u8 = 1;
const DISPOSAL_BG: u8 = 2;
const DISPOSAL_PREVIOUS: u8 = 3;

const BYTES_PER_COL: usize = 4;
const PIXEL_FORMAT: PixelFormat<'static> = PixelFormat::Rgba32;

pub struct Gif {
    pub width: usize,
    pub height: usize,
    /// Defaults to 1, but at some point we may discover a new value.
    /// Presumably this should only happen once.
    pub num_iterations: u16,
    pub frames: Vec<Frame>,
    gct_bg: usize,
    gct: Option<Box<[u8; MAX_COLOR_TABLE_SIZE]>>,
    data: Box<StreamReader>,
}

pub struct Frame {
    pub time: u32,
    pub duration: u32,
    pub data: Vec<u8>
}


impl Gif {
    /// Interpret the given Reader as an entire Gif file. Parses out the
    /// prelude to get most metadata (some will show up later, maybe).
    fn new (mut data: Box<StreamReader>) -> IoResult<Gif> {

        // ~~~~~~~~~ Image Prelude ~~~~~~~~~~
        let mut buf = [0; HEADER_LEN + GLOBAL_DESCRIPTOR_LEN];
        try!(read_to_full(&mut data, &mut buf));

        let (header, descriptor) = buf.split_at(HEADER_LEN);
        if header != b"GIF87a" && header != b"GIF89a" { return Err(malformed()); }

        let full_width = le_u16(descriptor[0], descriptor[1]);
        let full_height = le_u16(descriptor[2], descriptor[3]);
        let gct_mega_field = descriptor[4];
        let gct_background_color_index = descriptor[5] as usize;
        let gct_flag = (gct_mega_field & 0b1000_0000) != 0;
        let gct_size_exponent = gct_mega_field & 0b0000_0111;
        let gct_size = 1usize << (gct_size_exponent + 1); // 2^(k+1)

        let gct = if gct_flag {
            let mut gct_buf = Box::new([0; MAX_COLOR_TABLE_SIZE]);
            {
                let gct = &mut gct_buf[.. 3 * gct_size];
                try!(read_to_full(&mut data, gct));
            }
            Some(gct_buf)
        } else {
            None
        };

        Ok(Gif {
            width: full_width as usize,
            height: full_height as usize,
            num_iterations: 1, // This may be changed as we parse more
            frames: vec![],
            gct_bg: gct_background_color_index,
            gct: gct,
            data: data,
        })
    }


    /// Reads more of the stream until an entire new frame has been computed.
    /// Returns `false` if the file ends, and `true` otherwise.
    fn parse_next_frame(&mut self) -> IoResult<bool> {
        // ~~~~~~~~~ Image Body ~~~~~~~~~~~

        // local to this frame of the gif, but may be obtained at any time.
        let mut transparent_index = None;
        let mut frame_delay = 0;
        let mut disposal_method = 0;

        loop {
            match try!(read_byte(&mut self.data)) {
                BLOCK_EOF => {
                    // TODO: check if this was a sane place to stop?
                    return Ok(false)
                }
                BLOCK_EXTENSION => {
                    // 3 to coalesce some checks we'll have to make in any branch
                    match try!(read_byte(&mut self.data)) {
                        EXTENSION_PLAIN | EXTENSION_COMMENT => {
                            // This is legacy garbage, but has a variable length so
                            // we need to parse it a bit to get over it.
                            try!(skip_blocks(&mut self.data));
                        }
                        EXTENSION_GRAPHICS => {
                            // Frame delay and transparency settings
                            let mut ext = [0; GRAPHICS_EXTENSION_LEN];
                            try!(read_to_full(&mut self.data, &mut ext));

                            let rendering_mega_field = ext[1];
                            let transparency_flag = (rendering_mega_field & 0b0000_0001) != 0;

                            disposal_method = (rendering_mega_field & 0b0001_1100) >> 2;
                            frame_delay = le_u16(ext[2], ext[3]);
                            transparent_index = if transparency_flag {
                                Some(ext[4])
                            } else {
                                None
                            };
                        }
                        EXTENSION_APPLICATION => {
                            // NETSCAPE 2.0 Looping Extension

                            let mut ext = [0; APPLICATION_EXTENSION_LEN];
                            try!(read_to_full(&mut self.data, &mut ext));

                            // TODO: Verify this is the NETSCAPE 2.0 extension?

                            self.num_iterations = le_u16(ext[14], ext[15]);
                        }
                        _ => { return Err(Error::new(InvalidInput, "Unknown extension type found")); }
                    }
                }
                BLOCK_IMAGE => {
                    let mut descriptor = [0; LOCAL_DESCRIPTOR_LEN];
                    try!(read_to_full(&mut self.data, &mut descriptor));

                    let x      = le_u16(descriptor[0], descriptor[1]) as usize;
                    let y      = le_u16(descriptor[2], descriptor[3]) as usize;
                    let width  = le_u16(descriptor[4], descriptor[5]) as usize;
                    let height = le_u16(descriptor[6], descriptor[7]) as usize;

                    let lct_mega_field = descriptor[8];
                    let lct_flag = (lct_mega_field & 0b1000_0000) != 0;
                    let interlace = (lct_mega_field & 0b0100_0000) != 0;
                    let lct_size_exponent = (lct_mega_field & 0b0000_1110) >> 1;
                    let lct_size = 1usize << (lct_size_exponent + 1); // 2^(k+1)


                    let mut lct_buf = [0; MAX_COLOR_TABLE_SIZE];

                    let lct = if lct_flag {
                        let lct = &mut lct_buf[.. 3 * lct_size];
                        try!(read_to_full(&mut self.data, lct));
                        Some(&*lct)
                    } else {
                        None
                    };

                    let minimum_code_size = try!(read_byte(&mut self.data));

                    let mut indices = vec![0; width * height]; //TODO: not this

                    let mut parse_state = create_parse_state(minimum_code_size, width * height);

                    /* For debugging
                    println!("");
                    println!("starting frame decoding: {}", self.frames.len());
                    println!("x: {}, y: {}, w: {}, h: {}", x, y, width, height);
                    println!("trans: {:?}, interlace: {}", transparent_index, interlace);
                    println!("delay: {}, disposal: {}, iters: {:?}",
                             frame_delay, disposal_method, self.num_iterations);
                    println!("lct: {}, gct: {}", lct_flag, self.gct.is_some());
                    */

                    // ~~~~~~~~~~~~~~ DECODE THE INDICES ~~~~~~~~~~~~~~~~

                    if interlace {
                        let interlaced_offset = [0, 4, 2, 1];
                        let interlaced_jumps = [8, 8, 4, 2];
                        for i in 0..4 {
                            let mut j = interlaced_offset[i];
                            while j < height {
                                try!(get_indices(&mut parse_state,
                                                 &mut indices[j * width..],
                                                 width,
                                                 &mut self.data));
                                j += interlaced_jumps[i];
                            }
                        }
                    } else {
                        try!(get_indices(&mut parse_state, &mut indices, width * height, &mut self.data));
                    }

                    // ~~~~~~~~~~~~~~ INITIALIZE THE BACKGROUND ~~~~~~~~~~~

                    let num_bytes = self.width * self.height * BYTES_PER_COL;

                    let mut pixels = match disposal_method {
                        // Firefox says unspecified == current
                        DISPOSAL_UNSPECIFIED | DISPOSAL_CURRENT => {
                            self.frames.last().map(|frame| frame.data.clone())
                                         .unwrap_or_else(|| vec![0; num_bytes])
                        }
                        DISPOSAL_BG => {
                            vec![0; num_bytes]
                            /*
                            println!("BG disposal {}", self.gct_bg);
                            let col_idx = self.gct_bg as usize;
                            let color_map = self.gct.as_ref().unwrap();
                            let is_transparent = transparent_index.map(|idx| idx as usize == col_idx)
                                                                  .unwrap_or(false);
                            let (r, g, b, a) = if is_transparent {
                                (0, 0, 0, 0)
                            } else {
                                let col_idx = col_idx as usize;
                                let r = color_map[col_idx * 3 + 0];
                                let g = color_map[col_idx * 3 + 1];
                                let b = color_map[col_idx * 3 + 2];
                                (r, g, b, 0xFF)
                            };

                            let mut buf = Vec::with_capacity(num_bytes);
                            while buf.len() < num_bytes {
                                buf.push(r);
                                buf.push(g);
                                buf.push(b);
                                buf.push(a);
                            }
                            buf
                            */
                        }
                        DISPOSAL_PREVIOUS => {
                            let num_frames = self.frames.len();
                            if num_frames > 1 {
                                self.frames[num_frames - 2].data.clone()
                            } else {
                                vec![0; num_bytes]
                            }
                        }
                        _ => {
                            return Err(Error::new(InvalidInput, "unsupported disposal method"));
                        }
                    };

                    // ~~~~~~~~~~~~~~~~~~~ MAP INDICES TO COLORS ~~~~~~~~~~~~~~~~~~
                    {
                        let color_map = lct.unwrap_or_else(|| &**self.gct.as_ref().unwrap());
                        for (pix_idx, col_idx) in indices.into_iter().enumerate() {
                            let is_transparent = transparent_index.map(|idx| idx == col_idx)
                                                                  .unwrap_or(false);

                            // A transparent pixel "shows through" to whatever pixels
                            // were drawn before. True transparency can only be set
                            // in the disposal phase, as far as I can tell.
                            if is_transparent { continue; }

                            let col_idx = col_idx as usize;
                            let r = color_map[col_idx * 3 + 0];
                            let g = color_map[col_idx * 3 + 1];
                            let b = color_map[col_idx * 3 + 2];
                            let a = 0xFF;

                            // we're blitting this frame on top of some perhaps larger
                            // canvas. We need to adjust accordingly.
                            let pix_idx = x + y * self.width +
                                if width == self.width {
                                    pix_idx
                                } else {
                                    let row = pix_idx / width;
                                    let col = pix_idx % width;
                                    row * self.width + col
                                };
                            pixels[pix_idx * BYTES_PER_COL + 0] = r;
                            pixels[pix_idx * BYTES_PER_COL + 1] = g;
                            pixels[pix_idx * BYTES_PER_COL + 2] = b;
                            pixels[pix_idx * BYTES_PER_COL + 3] = a;
                        }
                    }
                    // ~~~~~~~~~~~~~~~~~~ DONE!!! ~~~~~~~~~~~~~~~~~~~~

                    let time = self.frames.last()
                                          .map(|frame| frame.time + frame.duration)
                                          .unwrap_or(0);
                    self.frames.push(Frame {
                        data: pixels,
                        duration: frame_delay as u32,
                        time: time,
                    });
                    return Ok(true)
                }
                _ => {
                    return Err(Error::new(InvalidInput, "unknown block found"));
                }
            }
        }
    }
}




// ~~~~~~~~~~~~~~~~~ utilities for decoding LZW data ~~~~~~~~~~~~~~~~~~~

const LZ_MAX_CODE: usize = 4095;
const LZ_BITS: usize = 12;

const NO_SUCH_CODE: usize = 4098;    // Impossible code, to signal empty.

struct ParseState {
    bits_per_pixel: usize,
    clear_code: usize,
    eof_code: usize,
    running_code: usize,
    running_bits: usize,
    max_code_1: usize,
    last_code: usize,
    stack_ptr: usize,
    current_shift_state: usize,
    current_shift_dword: usize,
    pixel_count: usize,
    buf: [u8; 256], // [0] = len, [1] = cur_index
    stack: [u8; LZ_MAX_CODE],
    suffix: [u8; LZ_MAX_CODE + 1],
    prefix: [usize; LZ_MAX_CODE + 1],
}

fn create_parse_state(code_size: u8, pixel_count: usize) -> ParseState {
    let bits_per_pixel = code_size as usize;
    let clear_code = 1 << bits_per_pixel;

    ParseState {
        buf: [0; 256], // giflib only inits the first byte to 0
        bits_per_pixel: bits_per_pixel,
        clear_code: clear_code,
        eof_code: clear_code + 1,
        running_code: clear_code + 2,
        running_bits: bits_per_pixel + 1,
        max_code_1: 1 << (bits_per_pixel + 1),
        stack_ptr: 0,
        last_code: NO_SUCH_CODE,
        current_shift_state: 0,
        current_shift_dword: 0,
        prefix: [NO_SUCH_CODE; LZ_MAX_CODE + 1],
        suffix: [0; LZ_MAX_CODE + 1],
        stack: [0; LZ_MAX_CODE],
        pixel_count: pixel_count,
    }
}

fn get_indices<R: Read>(state: &mut ParseState, indices: &mut[u8], index_count: usize, data: &mut R)
        -> IoResult<()> {
    state.pixel_count -= index_count;
    if state.pixel_count > 0xffff0000 {
        return Err(Error::new(InvalidInput, "Gif has too much pixel data"));
    }

    try!(decompress_indices(state, indices, index_count, data));

    if state.pixel_count == 0 {
        // There might be some more data hanging around. Finish walking through
        // the data section.
        try!(skip_blocks(data));
    }

    Ok(())
}

fn decompress_indices<R: Read>(state: &mut ParseState, indices: &mut[u8], index_count: usize, data: &mut R)
        -> IoResult<()> {
    let mut i = 0;
    let mut current_prefix; // This is uninit in dgif
    let &mut ParseState {
        mut stack_ptr,
        eof_code,
        clear_code,
        mut last_code,
        ..
    } = state;

    if stack_ptr > LZ_MAX_CODE { return Err(malformed()); }
    while stack_ptr != 0 && i < index_count {
        stack_ptr -= 1;
        indices[i] = state.stack[stack_ptr];
        i += 1;
    }

    while i < index_count {
        let current_code = try!(decompress_input(state, data));

        let &mut ParseState {
            ref mut prefix,
            ref mut suffix,
            ref mut stack,
            ..
        } = state;

        if current_code == eof_code { return Err(eof()); }

        if current_code == clear_code {
            // Reset all the sweet codez we learned
            for j in 0..LZ_MAX_CODE {
                prefix[j] = NO_SUCH_CODE;
            }

            state.running_code = state.eof_code + 1;
            state.running_bits = state.bits_per_pixel + 1;
            state.max_code_1 = 1 << state.running_bits;
            state.last_code = NO_SUCH_CODE;
            last_code = state.last_code;
        } else {
            // Regular code
            if current_code < clear_code {
                // single index code, direct mapping to a colour index
                indices[i] = current_code as u8;
                i += 1;
            } else {
                // MULTI-CODE MULTI-CODE ENGAGE -- DASH DASH DASH!!!!

                if prefix[current_code] == NO_SUCH_CODE {
                    current_prefix = last_code;

                    let code = if current_code == state.running_code - 2 {
                        last_code
                    } else {
                        current_code
                    };

                    let prefix_char = get_prefix_char(&*prefix, code, clear_code);
                    stack[stack_ptr] = prefix_char;
                    suffix[state.running_code - 2] = prefix_char;
                    stack_ptr += 1;
                } else {
                    current_prefix = current_code;
                }

                while stack_ptr < LZ_MAX_CODE
                        && current_prefix > clear_code
                        && current_prefix <= LZ_MAX_CODE {

                    stack[stack_ptr] = suffix[current_prefix];
                    stack_ptr += 1;
                    current_prefix = prefix[current_prefix];
                }

                if stack_ptr >= LZ_MAX_CODE || current_prefix > LZ_MAX_CODE {
                    return Err(malformed());
                }

                stack[stack_ptr] = current_prefix as u8;
                stack_ptr += 1;

                while stack_ptr != 0 && i < index_count {
                    stack_ptr -= 1;
                    indices[i] = stack[stack_ptr];
                    i += 1;
                }

            }

            if last_code != NO_SUCH_CODE && prefix[state.running_code - 2] == NO_SUCH_CODE {
                prefix[state.running_code - 2] = last_code;

                let code = if current_code == state.running_code - 2 {
                    last_code
                } else {
                    current_code
                };

                suffix[state.running_code - 2] = get_prefix_char(&*prefix, code, clear_code);
            }

            last_code = current_code;
        }
    }

    state.last_code = last_code;
    state.stack_ptr = stack_ptr;

    Ok(())
}

// Prefix is a virtual linked list or something.
fn get_prefix_char(prefix: &[usize], mut code: usize, clear_code: usize) -> u8 {
    let mut i = 0;

    loop {
        if code <= clear_code { break; }
        i += 1;
        if i > LZ_MAX_CODE { break; }
        if code > LZ_MAX_CODE { return NO_SUCH_CODE as u8; }
        code = prefix[code];
    }

    code as u8
}

fn decompress_input<R: Read>(state: &mut ParseState, src: &mut R) -> IoResult<usize> {
    let code_masks: [usize; 13] = [
        0x0000, 0x0001, 0x0003, 0x0007,
        0x000f, 0x001f, 0x003f, 0x007f,
        0x00ff, 0x01ff, 0x03ff, 0x07ff,
        0x0fff
    ];

    if state.running_bits > LZ_BITS { return Err(malformed()) }

    while state.current_shift_state < state.running_bits {
        // Get the next byte, which is either in this block or the next one
        let next_byte = if state.buf[0] == 0 {

            // This block is done, get the next one
            let len = try!(read_block(src, &mut state.buf[1..]));
            state.buf[0] = len as u8;

            // Reaching the end is not expected here
            if len == 0 { return Err(eof()); }

            let next_byte = state.buf[1];
            state.buf[1] = 2;
            state.buf[0] -= 1;
            next_byte
        } else {
            // Still got bytes in this block
            let next_byte = state.buf[state.buf[1] as usize];
            // this overflows when the line is 255 bytes long, and that's ok
            state.buf[1] = state.buf[1].wrapping_add(1);
            state.buf[0] -= 1;
            next_byte
        };

        state.current_shift_dword |= (next_byte as usize) << state.current_shift_state;
        state.current_shift_state += 8;
    }

    let code = state.current_shift_dword & code_masks[state.running_bits];
    state.current_shift_dword >>= state.running_bits;
    state.current_shift_state -= state.running_bits;

    if state.running_code < LZ_MAX_CODE + 2 {
        state.running_code += 1;
        if state.running_code > state.max_code_1 && state.running_bits < LZ_BITS {
            state.max_code_1 <<= 1;
            state.running_bits += 1;
        }
    }

    Ok(code)
}


// ~~~~~~~~~~~~ Streaming reading utils ~~~~~~~~~~~~~~~

fn read_byte<R: Read>(reader: &mut R) -> IoResult<u8> {
    let mut buf = [0];
    let bytes_read = try!(reader.read(&mut buf));
    if bytes_read != 1 { return Err(eof()); }
    Ok(buf[0])
}

fn read_to_full<R: Read>(reader: &mut R, buf: &mut [u8]) -> IoResult<()> {
    let mut read = 0;
    loop {
        if read == buf.len() { return Ok(()) }

        let bytes = try!(reader.read(&mut buf[read..]));

        if bytes == 0 { return Err(eof()) }

        read += bytes;
    }
}

/// A few places where you need to skip through some variable length region
/// without evaluating the results. This does that.
fn skip_blocks<R: Read>(reader: &mut R) -> IoResult<()> {
    let mut black_hole = [0; 255];
    loop {
        let len = try!(read_block(reader, &mut black_hole));
        if len == 0 { return Ok(()) }
    }
}

/// There are several variable length encoded regions in a GIF,
/// that look like [len, ..len]. This is a convenience for grabbing the next
/// block. Returns `len`.
fn read_block<R: Read>(reader: &mut R, buf: &mut [u8]) -> IoResult<usize> {
    debug_assert!(buf.len() >= 255);
    let len = try!(read_byte(reader)) as usize;
    if len == 0 { return Ok(0) } // read_to_full will probably freak out
    try!(read_to_full(reader, &mut buf[..len]));
    Ok(len)
}

fn le_u16(first: u8, second: u8) -> u16 {
    ((second as u16) << 8) | (first as u16)
}

fn malformed() -> Error {
    Error::new(InvalidInput, "Malformed GIF")
}

fn eof() -> Error {
    Error::new(InvalidInput, "Unexpected end of GIF")
}

