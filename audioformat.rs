// Copyright 2015 The Servo Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

//! Audio sample format utility routines.

pub trait AudioFormat {
    type SampleType;
}

/// Converts between audio formats on the CPU.
pub trait ConvertAudioFormat<To:AudioFormat> : AudioFormat {
    fn convert(&self,
               to: &To,
               output_samples: &mut [&mut [To::SampleType]],
               input_samples: &[&[Self::SampleType]],
               channels: usize)
               -> Result<(),()>;
}

/// Planar 32-bit floating point.
#[derive(Copy, Clone)]
pub struct Float32Planar;

impl AudioFormat for Float32Planar {
    type SampleType = f32;
}

/// Interleaved (non-planar) 32-bit floating point.
#[derive(Copy, Clone)]
pub struct Float32Interleaved;

impl AudioFormat for Float32Interleaved {
    type SampleType = f32;
}

impl ConvertAudioFormat<Float32Interleaved> for Float32Planar {
    fn convert(&self,
               _: &Float32Interleaved,
               output_samples: &mut [&mut [f32]],
               input_samples: &[&[f32]],
               channels: usize)
               -> Result<(),()> {
        debug_assert!(input_samples.len() == channels);
        debug_assert!(output_samples.len() == 1);
        debug_assert!(input_samples[0].len() * channels <= output_samples[0].len());
        debug_assert!(input_samples.iter().all(|samples| input_samples[0].len() == samples.len()));

        let mut output_index = 0;
        for sample in 0 .. input_samples[0].len() {
            for channel in 0 .. channels {
                output_samples[0][output_index] = input_samples[channel][sample];
                output_index += 1;
            }
        }
        Ok(())
    }
}

