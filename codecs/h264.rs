// Copyright 2015 The Servo Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

use videodecoder::VideoHeaders;

/// Constructs an AVCC chunk from a set of decoder headers.
pub fn create_avcc_chunk(headers: &VideoHeaders) -> Vec<u8> {
    let seq_headers = headers.h264_seq_headers().unwrap();
    let pict_headers = headers.h264_pict_headers().unwrap();

    let mut avcc = Vec::new();
    avcc.push_all(&[
        0x01,
        seq_headers[0][1],
        seq_headers[0][2],
        seq_headers[0][3],
        0xff,   // 4 bytes per NALU
        (seq_headers.len() as u8) | 0b11100000,
    ]);

    for seq_header in seq_headers.iter() {
        avcc.push_all(&[ (seq_header.len() >> 8) as u8, seq_header.len() as u8 ]);
        avcc.push_all(seq_header);
    }

    avcc.push(pict_headers.len() as u8);
    for pict_header in pict_headers.iter() {
        avcc.push_all(&[ (pict_header.len() >> 8) as u8, pict_header.len() as u8 ]);
        avcc.push_all(pict_header);
    }

    avcc
}

