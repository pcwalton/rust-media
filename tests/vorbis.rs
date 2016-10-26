// Copyright 2015 The Servo Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

extern crate hound;
extern crate rust_media;
extern crate lewton;
extern crate ogg;

use std::fs::File;
use hound::WavReader;
use ogg::PacketReader;
use lewton::inside_ogg::OggStreamReader;

#[test]
fn test_vorbis() {
    let mut wav_reader = WavReader::open("tests/samples/test.wav").unwrap();
    let mut wav_samples_iter = wav_reader.samples::<i16>();
    let ogg_packet_reader = PacketReader::new(File::open("tests/samples/test.ogg").unwrap());
    let mut ogg_reader = OggStreamReader::new(ogg_packet_reader).unwrap();
    let mut n = 0;
    loop {
        let pck = ogg_reader.read_dec_packet().unwrap();
        if pck.is_none() {
            break;
        }
        let pck = pck.unwrap();
        // We assume the test file has only one channel.
        // For test files with more than one channels you need to
        // extend the logic.
        //println!("Checking packet no {} ({} samples) ...", n, pck[0].len());
        let mut s_idx = 0;
        for &ogg_sample in &pck[0] {
            if n == 10 && s_idx >= 948 {
                // Ignore the last few samples of the last packet,
                // as they are not inside the wav file for some reason.
                break;
            }
            let wav_sample = wav_samples_iter.next().unwrap().unwrap();
            if (ogg_sample - wav_sample).abs() > 1 {
                panic!("Difference found in packet no {}, sample {}: was {} but expected {}",
                    n, s_idx, ogg_sample, wav_sample);
            }
            s_idx += 1;
        }
        n += 1;
    }
}
