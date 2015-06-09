// Copyright 2015 The Servo Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

//! Pixel format utility routines.

use std::cmp;
use std::io::{BufWriter, Write};
use std::slice::bytes;

/// 8-bit Y plane followed by 8-bit 2x2-subsampled U and V planes.
#[derive(Copy, Clone, Debug)]
pub struct I420;

/// 8-bit Y plane followed by an interleaved U/V plane containing 2x2 subsampled color difference
/// samples.
#[derive(Copy, Clone, Debug)]
pub struct NV12;

/// 8-bit indexes into a 24-bit color palette.
#[derive(Copy, Clone, Debug)]
pub struct Palette<'a> {
    pub palette: &'a [RgbColor],
}

impl<'a> Palette<'a> {
    pub fn empty() -> Palette<'static> {
        Palette {
            palette: &[],
        }
    }
}

/// 24-bit RGB.
#[derive(Copy, Clone, Debug)]
pub struct Rgb24;

#[derive(Copy, Clone)]
pub struct YuvColor {
    pub y: f64,
    pub u: f64,
    pub v: f64,
}

#[derive(Copy, Clone, Debug)]
pub struct RgbColor {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

/// Converts between pixel formats on the CPU.
pub trait ConvertPixelFormat<To> {
    fn convert(&self,
               to: &To,
               output_pixels: &mut [&mut [u8]],
               output_strides: &[usize],
               input_pixels: &[&[u8]],
               input_strides: &[usize],
               width: usize,
               height: usize)
               -> Result<(),()>;
}

impl ConvertPixelFormat<I420> for I420 {
    fn convert(&self,
               _: &I420,
               output_pixels: &mut [&mut [u8]],
               output_strides: &[usize],
               input_pixels: &[&[u8]],
               input_strides: &[usize],
               _: usize,
               height: usize)
               -> Result<(),()> {
        for plane in 0 .. 3 {
            let (y_input_pixels, y_input_stride) = (input_pixels[plane], input_strides[plane]);
            let y_output_pixels = &mut *output_pixels[plane];
            let y_output_stride = output_strides[plane];
            let minimum_stride = cmp::min(y_input_stride, y_output_stride);

            let effective_height = if plane == 0 {
                height
            } else {
                height / 2
            };

            let (mut input_index, mut output_index) = (0, 0);
            for _ in 0 .. effective_height {
                let input_row = &y_input_pixels[input_index..input_index + minimum_stride];
                let mut output_row =
                    &mut y_output_pixels[output_index..output_index + minimum_stride];
                bytes::copy_memory(input_row, output_row);
                input_index += y_input_stride;
                output_index += y_output_stride;
            }
        }
        Ok(())
    }
}

impl ConvertPixelFormat<I420> for NV12 {
    fn convert(&self,
               _: &I420,
               output_pixels: &mut [&mut [u8]],
               output_strides: &[usize],
               input_pixels: &[&[u8]],
               input_strides: &[usize],
               width: usize,
               height: usize)
               -> Result<(),()> {
        // Copy over the Y plane.
        let (y_input_pixels, y_input_stride) = (input_pixels[0], input_strides[0]);
        let (mut input_index, mut output_index) = (0, 0);
        for _ in 0 .. height {
            let input_row = &y_input_pixels[input_index..input_index + width];
            let mut output_row = &mut output_pixels[0][output_index..output_index + width];
            bytes::copy_memory(input_row, output_row);
            input_index += y_input_stride;
            output_index += output_strides[0];
        }

        // Interleave the U and V planes.
        let (y_input_pixels, y_input_stride) = (input_pixels[1], input_strides[1]);
        let (y_output_u_pixels, y_output_v_pixels) = output_pixels.split_at_mut(2);
        let y_output_u_pixels = &mut y_output_u_pixels[1];
        let y_output_v_pixels = &mut y_output_v_pixels[0];
        let y_output_u_stride = output_strides[1];
        let y_output_v_stride = output_strides[2];
        let effective_height = height / 2;

        let (mut input_index, mut output_u_index, mut output_v_index) = (0, 0, 0);
        for _ in 0 .. effective_height {
            let input_row = &y_input_pixels[input_index..input_index + y_input_stride];
            let output_u_row =
                &mut y_output_u_pixels[output_u_index..output_u_index + width / 2];
            let output_v_row =
                &mut y_output_v_pixels[output_v_index..output_v_index + width / 2];

            let mut u_writer = BufWriter::new(output_u_row);
            let mut v_writer = BufWriter::new(output_v_row);
            for x in 0 .. width / 2 {
                drop(u_writer.write_all(&[input_row[x * 2]]));
                drop(v_writer.write_all(&[input_row[x * 2 + 1]]));
            }

            input_index += y_input_stride;
            output_u_index += y_output_u_stride;
            output_v_index += y_output_v_stride;
        }

        Ok(())
    }
}

impl ConvertPixelFormat<Rgb24> for I420 {
    fn convert(&self,
               _: &Rgb24,
               output_pixels: &mut [&mut [u8]],
               output_strides: &[usize],
               input_pixels: &[&[u8]],
               input_strides: &[usize],
               width: usize,
               height: usize)
               -> Result<(),()> {
        // FIXME(pcwalton): This does not convert the chroma yet. Dorothy has not yet left Kansas.
        let (y_input_pixels, y_input_stride) = (input_pixels[0], input_strides[0]);
        let (mut input_index, mut output_index) = (0, 0);
        for _ in 0 .. height {
            let input_row = &y_input_pixels[input_index..input_index + width * 3];
            let output_row =
                &mut output_pixels[0][output_index..output_index + output_strides[0]];
            let mut writer = BufWriter::new(output_row);
            for x in 0 .. width {
                drop(writer.write_all(&[input_row[x], input_row[x], input_row[x]]));
            }
            input_index += y_input_stride;
            output_index += output_strides[0];
        }
        Ok(())
    }
}

impl<'a> ConvertPixelFormat<Rgb24> for Palette<'a> {
    fn convert(&self,
               _: &Rgb24,
               output_pixels: &mut [&mut [u8]],
               output_strides: &[usize],
               input_pixels: &[&[u8]],
               input_strides: &[usize],
               width: usize,
               height: usize)
               -> Result<(),()> {
        let (y_input_pixels, y_input_stride) = (input_pixels[0], input_strides[0]);
        let (mut input_index, mut output_index) = (0, 0);
        for _ in 0 .. height {
            let input_row = &y_input_pixels[input_index..input_index + width];
            let output_row = &mut output_pixels[0][output_index..output_index + width * 3];
            let mut writer = BufWriter::new(output_row);
            for x in 0 .. width {
                let color = self.palette[input_row[x] as usize];
                drop(writer.write_all(&[color.r, color.g, color.b]));
            }
            input_index += y_input_stride;
            output_index += output_strides[0];
        }
        Ok(())
    }
}

impl ConvertPixelFormat<Rgb24> for Rgb24 {
    fn convert(&self,
               _: &Rgb24,
               output_pixels: &mut [&mut [u8]],
               output_strides: &[usize],
               input_pixels: &[&[u8]],
               input_strides: &[usize],
               width: usize,
               height: usize)
               -> Result<(),()> {
        let (y_input_pixels, y_input_stride) = (input_pixels[0], input_strides[0]);
        let (mut input_index, mut output_index) = (0, 0);
        for _ in 0 .. height {
            let input_row = &y_input_pixels[input_index..input_index + width * 3];
            let mut output_row = &mut output_pixels[0][output_index..output_index + width * 3];
            bytes::copy_memory(input_row, output_row);
            input_index += y_input_stride;
            output_index += output_strides[0];
        }
        Ok(())
    }
}

/// Converts between color formats on the CPU.
pub trait ConvertColorFormat<To> {
    fn convert(&self) -> To;
}

impl ConvertColorFormat<RgbColor> for YuvColor {
    fn convert(&self) -> RgbColor {
        const W_R: f64 = 0.299;
        const W_G: f64 = 1.0 - W_R - W_B;
        const W_B: f64 = 0.114;
        const U_MAX: f64 = 1.0;
        const V_MAX: f64 = 1.0;
        let r = self.y + self.v * (1.0 - W_R) / V_MAX;
        let g = self.y - self.u * (W_B * (1.0 - W_B)) / (U_MAX * W_G) -
            self.v * (W_R * (1.0 - W_R)) / (V_MAX * W_G);
        let b = self.y + self.u * (1.0 - W_B) / U_MAX;
        RgbColor {
            r: (r / 255.0) as u8,
            g: (g / 255.0) as u8,
            b: (b / 255.0) as u8,
        }
    }
}

/// Generic pixel format conversion with the pixel formats determined at runtime.
///
/// We follow the same nomenclature as the document here: http://www.fourcc.org/yuv.php
#[derive(Copy, Clone, Debug)]
pub enum PixelFormat<'a> {
    I420,
    NV12,
    Indexed(Palette<'a>),
    Rgb24,
}

impl<'a> ConvertPixelFormat<PixelFormat<'a>> for PixelFormat<'a> {
    fn convert(&self,
               to: &PixelFormat<'a>,
               output_pixels: &mut [&mut [u8]],
               output_strides: &[usize],
               input_pixels: &[&[u8]],
               input_strides: &[usize],
               width: usize,
               height: usize)
               -> Result<(),()> {
        match (*self, *to) {
            (PixelFormat::I420, PixelFormat::I420) => {
                I420.convert(&I420,
                             output_pixels,
                             output_strides,
                             input_pixels,
                             input_strides,
                             width,
                             height)
            }
            (PixelFormat::NV12, PixelFormat::I420) => {
                NV12.convert(&I420,
                             output_pixels,
                             output_strides,
                             input_pixels,
                             input_strides,
                             width,
                             height)
            }
            (PixelFormat::I420, PixelFormat::Rgb24) => {
                I420.convert(&Rgb24,
                             output_pixels,
                             output_strides,
                             input_pixels,
                             input_strides,
                             width,
                             height)
            }
            (PixelFormat::Indexed(palette), PixelFormat::Rgb24) => {
                palette.convert(&Rgb24,
                                output_pixels,
                                output_strides,
                                input_pixels,
                                input_strides,
                                width,
                                height)
            }
            (PixelFormat::Rgb24, PixelFormat::Rgb24) => {
                Rgb24.convert(&Rgb24,
                              output_pixels,
                              output_strides,
                              input_pixels,
                              input_strides,
                              width,
                              height)
            }
            (_, _) => Err(()),
        }
    }
}

impl<'a> PixelFormat<'a> {
    /// Returns the number of planes in this pixel format.
    pub fn planes(&self) -> usize {
        match *self {
            PixelFormat::I420 => 3,
            PixelFormat::NV12 => 2,
            PixelFormat::Indexed(_) | PixelFormat::Rgb24 => 1,
        }
    }
}

