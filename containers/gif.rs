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
use pixelformat::{Palette, PixelFormat, RgbColor};
use streaming::StreamReader;
use timing::Timestamp;
use videodecoder;

use libc::{self, c_double, c_int, c_long, c_uchar, c_uint, c_void, size_t};
use std::cell::RefCell;
use std::i32;
use std::mem;
use std::io::{BufReader, BufWriter, Read, Write};
use std::io::SeekFrom;
use std::ptr;
use std::slice;
use std::marker::PhantomData;

use byteorder::{LittleEndian, WriteBytesExt, ReadBytesExt};

#[repr(C)]
#[unsafe_no_drop_flag]
pub struct FileType {
    /// The underlying file.
    file: *mut ffi::GifFileType,
    /// The byte position in the stream of the next record we have yet to read.
    next_record_byte_offset: u64,
}

impl Drop for FileType {
    fn drop(&mut self) {
        unsafe {
            ffi::DGifCloseFile(self.file, &mut 0);
        }
    }
}

impl FileType {
    pub fn new(reader: Box<StreamReader>) -> Result<FileType, c_int> {
        let mut error = 0;
        let file = unsafe {
            ffi::DGifOpen(mem::transmute::<Box<Box<_>>, *mut c_void>(Box::new(reader)),
                          read_func,
                          &mut error)
        };
        if !file.is_null() {
            let mut file = FileType {
                file: file,
                next_record_byte_offset: 0,
            };
            file.next_record_byte_offset = file.reader().seek(SeekFrom::Current(0)).unwrap();
            Ok(file)
        } else {
            Err(error)
        }
    }

    pub fn reader<'a>(&'a mut self) -> &'a mut StreamReader {
        unsafe {
            let reader: &mut Box<Box<StreamReader>> = mem::transmute(&mut (*self.file).UserData);
            &mut ***reader
        }
    }

    pub fn slurp(&mut self) -> Result<(),c_int> {
        unsafe {
            (*self.file).ExtensionBlocks = ptr::null_mut();
            (*self.file).ExtensionBlockCount = 0;
            loop {
                match self.read_record() {
                    Ok(true) => {}
                    Ok(false) => break,
                    Err(_) => return Err((*self.file).Error),
                }
            }
        }
        Ok(())
    }

    /// This function is a port of the inner loop of `DGifSlurp()`. Returns true if there are more
    /// records or false if we're done.
    pub fn read_record(&mut self) -> Result<bool,()> {
        let next_record_byte_offset = self.next_record_byte_offset;
        self.reader().seek(SeekFrom::Start(next_record_byte_offset)).unwrap();

        let mut record_type = 0;
        unsafe {
            if ffi::DGifGetRecordType(self.file, &mut record_type) == ffi::GIF_ERROR {
                return Err(())
            }
        }

        match record_type {
            ffi::IMAGE_DESC_RECORD_TYPE => {
                unsafe {
                    if ffi::DGifGetImageDesc(self.file) == ffi::GIF_ERROR {
                        return Err(())
                    }

                    {
                        let saved_images = self.mut_saved_images();
                        let saved_image = saved_images.last_mut().unwrap();
                        saved_image.RasterBits = ptr::null_mut();
                    }

                    if !(*self.file).ExtensionBlocks.is_null() {
                        let extension_blocks = (*self.file).ExtensionBlocks;
                        let extension_block_count = (*self.file).ExtensionBlockCount;
                        {
                            let saved_images = self.mut_saved_images();
                            let saved_image = saved_images.last_mut().unwrap();
                            saved_image.ExtensionBlocks = extension_blocks;
                            saved_image.ExtensionBlockCount = extension_block_count;
                        }
                        (*self.file).ExtensionBlocks = ptr::null_mut();
                        (*self.file).ExtensionBlockCount = 0;
                    }
                }

                if self.read_image().is_err() {
                    return Err(())
                }
            }
            ffi::EXTENSION_RECORD_TYPE => {
                unsafe {
                    let (mut ext_function, mut ext_data) = (0, ptr::null_mut());
                    if ffi::DGifGetExtension(self.file, &mut ext_function, &mut ext_data) ==
                            ffi::GIF_ERROR {
                        return Err(())
                    }

                    if !ext_data.is_null() {
                        if ffi::GifAddExtensionBlock(&mut (*self.file).ExtensionBlockCount,
                                                     &mut (*self.file).ExtensionBlocks,
                                                     ext_function,
                                                     *ext_data as c_uint,
                                                     ext_data.offset(1)) == ffi::GIF_ERROR {
                            return Err(())
                        }
                    }

                    while !ext_data.is_null() {
                        if ffi::DGifGetExtensionNext(self.file, &mut ext_data) == ffi::GIF_ERROR {
                            return Err(())
                        }
                        if !ext_data.is_null() {
                            if ffi::GifAddExtensionBlock(&mut (*self.file).ExtensionBlockCount,
                                                         &mut (*self.file).ExtensionBlocks,
                                                         ffi::CONTINUE_EXT_FUNC_CODE,
                                                         *ext_data as c_uint,
                                                         ext_data.offset(1)) == ffi::GIF_ERROR {
                                return Err(())
                            }
                        }
                    }
                }
            }
            _ => return Ok(false),
        }

        self.next_record_byte_offset = self.reader().seek(SeekFrom::Current(0)).unwrap();
        Ok(true)
    }

    /// A port of a section of `DGifSlurp`.
    fn read_image(&mut self) -> Result<(),()> {
        unsafe {
            let saved_image = self.mut_saved_images().last_mut().unwrap();
            if !saved_image.RasterBits.is_null() {
                // Already read!
                return Ok(())
            }

            let image_size = saved_image.ImageDesc.Width * saved_image.ImageDesc.Height;
            let image_byte_size = image_size as usize * mem::size_of::<ffi::GifPixelType>();
            assert!(image_byte_size <= i32::MAX as usize);
            saved_image.RasterBits = libc::malloc(image_size as size_t) as *mut c_uchar;
            if saved_image.RasterBits.is_null() {
                return Err(())
            }
            let mut raster_bits = slice::from_raw_parts_mut(saved_image.RasterBits,
                                                          image_size as usize);

            if (*saved_image).ImageDesc.Interlace {
                // From `DGifSlurp()`: "The way an interlaced image should be read: offsets and
                // jumps…"
                static INTERLACED_OFFSETS: [u8; 4] = [ 0, 4, 2, 1 ];
                static INTERLACED_JUMPS: [u8; 4] = [ 8, 8, 4, 2 ];

                // Perform four passes on the image.
                for (i, &j) in INTERLACED_OFFSETS.iter().enumerate() {
                    let mut j = j as c_int;
                    while j <= saved_image.ImageDesc.Height {
                        let width = saved_image.ImageDesc.Width;
                        let dest =
                            &mut raster_bits[((j * width) as usize)..((j + 1) * width) as usize];
                        assert!(dest.len() <= i32::MAX as usize);
                        if ffi::DGifGetLine(self.file, dest.as_mut_ptr(), dest.len() as i32) ==
                                ffi::GIF_ERROR {
                            return Err(())
                        }
                        j += INTERLACED_JUMPS[i] as c_int;
                    }
                }
            } else if ffi::DGifGetLine(self.file,
                                       raster_bits.as_mut_ptr(),
                                       raster_bits.len() as i32) ==
                    ffi::GIF_ERROR {
                return Err(())
            }
        }
        Ok(())
    }

    pub fn width(&self) -> ffi::GifWord {
        unsafe {
            (*self.file).SWidth
        }
    }

    pub fn height(&self) -> ffi::GifWord {
        unsafe {
            (*self.file).SHeight
        }
    }

    pub fn color_map<'a>(&'a self) -> Option<ColorMapObject<'a>> {
        unsafe {
            if !(*self.file).SColorMap.is_null() {
                Some(ColorMapObject {
                    map: (*self.file).SColorMap,
                    phantom: PhantomData,
                })
            } else {
                None
            }
        }
    }

    pub fn saved_images<'a>(&'a self) -> &'a [SavedImage] {
        unsafe {
            slice::from_raw_parts((*self.file).SavedImages, (*self.file).ImageCount as usize)
        }
    }

    pub unsafe fn mut_saved_images<'a>(&'a mut self) -> &'a mut [SavedImage] {
        slice::from_raw_parts_mut((*self.file).SavedImages, (*self.file).ImageCount as usize)
    }

    pub fn extension_block_count(&self) -> c_int {
        unsafe {
            (*self.file).ExtensionBlockCount
        }
    }

    pub fn extension_block<'a>(&'a self, index: c_int) -> ExtensionBlock<'a> {
        assert!(index >= 0 && index < self.extension_block_count());
        unsafe {
            ExtensionBlock::from_ptr((*self.file).ExtensionBlocks.offset(index as isize))
        }
    }
}

extern "C" fn read_func(file: *mut ffi::GifFileType, buffer: *mut ffi::GifByteType, len: c_int)
                        -> c_int {
    if len < 0 {
        return -1
    }

    unsafe {
        let reader: &mut Box<Box<StreamReader>> = mem::transmute(&mut (*file).UserData);
        match reader.read(slice::from_raw_parts_mut(buffer, len as usize)) {
            Ok(number_read) => number_read as c_int,
            _ => -1
        }
    }
}

#[repr(C)]
pub struct SavedImage {
    ImageDesc: ffi::GifImageDesc,
    RasterBits: *mut ffi::GifByteType,
    ExtensionBlockCount: c_int,
    ExtensionBlocks: *mut ffi::ExtensionBlock,
}

impl SavedImage {
    pub fn raster_bits<'a>(&'a self) -> &'a [ffi::GifByteType] {
        unsafe {
            slice::from_raw_parts_mut(self.RasterBits,
                                    self.ImageDesc.Width as usize * self.ImageDesc.Height as usize)
        }
    }

    pub fn image_desc<'a>(&'a self) -> ImageDesc<'a> {
        ImageDesc {
            desc: &self.ImageDesc,
        }
    }

    pub fn extension_block_count(&self) -> c_int {
        self.ExtensionBlockCount
    }

    pub fn extension_block<'a>(&'a self, index: c_int) -> ExtensionBlock<'a> {
        assert!(index >= 0 && index < self.extension_block_count());
        unsafe {
            ExtensionBlock::from_ptr(self.ExtensionBlocks.offset(index as isize))
        }
    }
}

#[derive(Copy, Clone)]
pub struct ImageDesc<'a> {
    desc: &'a ffi::GifImageDesc,
}

impl<'a> ImageDesc<'a> {
    pub fn width(&self) -> ffi::GifWord {
        self.desc.Width
    }

    pub fn height(&self) -> ffi::GifWord {
        self.desc.Height
    }

    pub fn interlace(&self) -> bool {
        self.desc.Interlace
    }

    pub fn color_map(&self) -> Option<ColorMapObject<'a>> {
        if !self.desc.ColorMap.is_null() {
            Some(ColorMapObject {
                map: self.desc.ColorMap,
                phantom: PhantomData,
            })
        } else {
            None
        }
    }
}

pub struct ColorMapObject<'a> {
    map: *mut ffi::ColorMapObject,
    phantom: PhantomData<&'a u8>,
}

impl<'a> ColorMapObject<'a> {
    pub fn bits_per_pixel(&self) -> c_int {
        unsafe {
            (*self.map).BitsPerPixel
        }
    }

    pub fn colors(&'a self) -> &'a [ffi::GifColorType] {
        unsafe {
            slice::from_raw_parts_mut((*self.map).Colors, (*self.map).ColorCount as usize)
        }
    }
}

pub enum ExtensionBlock<'a> {
    Continue,
    Comment(&'a [u8]),
    Graphics(GraphicsControlBlock),
    Plaintext(&'a [u8]),
    Application(&'a [u8]),
    Other,
}

impl<'a> ExtensionBlock<'a> {
    unsafe fn from_ptr(block: *mut ffi::ExtensionBlock) -> ExtensionBlock<'a> {
        return match (*block).Function {
            ffi::CONTINUE_EXT_FUNC_CODE => ExtensionBlock::Continue,
            ffi::COMMENT_EXT_FUNC_CODE => ExtensionBlock::Comment(to_byte_slice(block)),
            ffi::GRAPHICS_EXT_FUNC_CODE => {
                let mut graphics_control_block = ffi::GraphicsControlBlock {
                    DisposalMode: 0,
                    UserInputFlag: false,
                    DelayTime: 0,
                    TransparentColor: 0,
                };
                assert!(ffi::DGifExtensionToGCB((*block).ByteCount as size_t,
                                                (*block).Bytes,
                                                &mut graphics_control_block) == ffi::GIF_OK);
                ExtensionBlock::Graphics(GraphicsControlBlock {
                    block: graphics_control_block,
                })
            }
            ffi::PLAINTEXT_EXT_FUNC_CODE => ExtensionBlock::Plaintext(to_byte_slice(block)),
            ffi::APPLICATION_EXT_FUNC_CODE => ExtensionBlock::Application(to_byte_slice(block)),
            _ => ExtensionBlock::Other,
        };

        unsafe fn to_byte_slice<'a>(block: *mut ffi::ExtensionBlock) -> &'a [u8] {
            assert!((*block).ByteCount >= 0);
            slice::from_raw_parts_mut((*block).Bytes, (*block).ByteCount as usize)
        }
    }
}

pub struct GraphicsControlBlock {
    block: ffi::GraphicsControlBlock,
}

impl GraphicsControlBlock {
    /// Particular way to initialize the pixels of the frame
    pub fn disposal_mode(&self) -> DisposalMode {
        // FIXME(Gankro): didn't want to pull in a whole crate/syntex for FromPrimitive
        match self.block.DisposalMode {
            ffi::DISPOSAL_UNSPECIFIED => DisposalMode::Unspecified,
            ffi::DISPOSE_DO_NOT => DisposalMode::DoNot,
            ffi::DISPOSE_BACKGROUND => DisposalMode::Background,
            ffi::DISPOSE_PREVIOUS => DisposalMode::Previous,
            _ => unreachable!(),
        }
    }

    /// archaic; specifies that the gif should wait for user input before proceeding.
    pub fn user_input_flag(&self) -> bool {
        self.block.UserInputFlag
    }

    pub fn delay_time(&self) -> c_int {
        self.block.DelayTime
    }

    /// Which colour index to interpret as a transparent pixel.
    /// Note: this still overwrites a non-trasparent pixel.
    pub fn transparent_color(&self) -> Option<c_int> {
        let color = self.block.TransparentColor;
        if color >= 0 {
            Some(color)
        } else {
            None
        }
    }
}

#[repr(i32)]
#[derive(Copy, Clone)]
/// Specifies what state to initialize the frame's pixels in
pub enum DisposalMode {
    // Treat this like Background, I guess
    Unspecified = ffi::DISPOSAL_UNSPECIFIED,
    // Use the previous frame as the starting point
    DoNot = ffi::DISPOSE_DO_NOT,
    // Blank the frame to the background colour
    Background = ffi::DISPOSE_BACKGROUND,
    // Use the previous-previous frame as the starting point (archaic?)
    Previous = ffi::DISPOSE_PREVIOUS,
}

// Implementation of the abstract `ContainerReader` interface

pub struct ContainerReaderImpl {
    file: RefCell<FileType>,
}

impl ContainerReaderImpl {
    pub fn new(reader: Box<StreamReader>) -> Result<Box<container::ContainerReader + 'static>,()> {
        let file = match FileType::new(reader) {
            Ok(file) => file,
            Err(_) => return Err(()),
        };
        Ok(Box::new(ContainerReaderImpl {
            file: RefCell::new(file),
        }) as Box<container::ContainerReader + 'static>)
    }
}

impl container::ContainerReader for ContainerReaderImpl {
    fn track_count(&self) -> u16 {
        1
    }
    fn track_by_index<'a>(&'a self, _: u16) -> Box<container::Track + 'a> {
        Box::new(TrackImpl {
            file: &self.file,
        }) as Box<container::Track + 'a>
    }
    fn track_by_number<'a>(&'a self, _: c_long) -> Box<container::Track + 'a> {
        self.track_by_index(0)
    }
}

struct TrackImpl<'a> {
    file: &'a RefCell<FileType>,
}

impl<'a> container::Track<'a> for TrackImpl<'a> {
    fn track_type(self: Box<Self>) -> container::TrackType<'a> {
        container::TrackType::Video(Box::new(VideoTrackImpl {
            file: self.file,
        }) as Box<container::VideoTrack<'a> + 'a>)
    }

    fn is_video(&self) -> bool { true }
    fn is_audio(&self) -> bool { false }

    fn cluster_count(&self) -> Option<c_int> {
        None
    }

    fn number(&self) -> c_long {
        0
    }

    fn codec(&self) -> Option<Vec<u8>> {
        Some(vec![b'G', b'I', b'F', b'f'])
    }

    fn cluster<'b>(&'b self, cluster_index: i32) -> Result<Box<container::Cluster + 'b>,()> {
        get_cluster(self.file, cluster_index)
    }
}

struct VideoTrackImpl<'a> {
    file: &'a RefCell<FileType>,
}

impl<'a> container::Track<'a> for VideoTrackImpl<'a> {
    fn track_type(self: Box<Self>) -> container::TrackType<'a> {
        container::TrackType::Video(Box::new(VideoTrackImpl {
            file: self.file,
        }) as Box<container::VideoTrack<'a> + 'a>)
    }

    fn is_video(&self) -> bool { true }
    fn is_audio(&self) -> bool { false }

    fn cluster_count(&self) -> Option<c_int> {
        None
    }

    fn number(&self) -> c_long {
        0
    }

    fn codec(&self) -> Option<Vec<u8>> {
        Some(vec![b'G', b'I', b'F', b'f'])
    }

    fn cluster<'b>(&'b self, cluster_index: i32) -> Result<Box<container::Cluster + 'b>,()> {
        get_cluster(self.file, cluster_index)
    }
}

impl<'a> container::VideoTrack<'a> for VideoTrackImpl<'a> {
    fn width(&self) -> u16 {
        self.file.borrow().width() as u16
    }

    fn height(&self) -> u16 {
        self.file.borrow().height() as u16
    }

    fn frame_rate(&self) -> c_double {
        // NB: This assumes a constant frame rate. Not all GIFs have one, however…
        for saved_image in self.file.borrow().saved_images().iter() {
            for i in 0..saved_image.extension_block_count() {
                if let ExtensionBlock::Graphics(block) = saved_image.extension_block(i) {
                    return 1.0 / ((block.delay_time() as c_double) * 0.01)
                }
            }
        }
        for i in 0..self.file.borrow().extension_block_count() {
            if let ExtensionBlock::Graphics(block) = self.file.borrow().extension_block(i) {
                return 1.0 / ((block.delay_time() as c_double) * 0.01)
            }
        }
        1.0
    }

    fn pixel_format(&self) -> PixelFormat<'static> {
        PixelFormat::Indexed(Palette::empty())
    }

    fn headers(&self) -> Box<videodecoder::VideoHeaders> {
        Box::new(videodecoder::EmptyVideoHeadersImpl) as Box<videodecoder::VideoHeaders>
    }
}

fn get_cluster<'a>(file: &'a RefCell<FileType>, cluster_index: i32)
                   -> Result<Box<container::Cluster + 'a>,()> {
    // Read and decode frames until we get to the given cluster index.
    while file.borrow().saved_images().len() < (cluster_index as usize + 1) {
        match file.borrow_mut().read_record() {
            Err(_) | Ok(false) => return Err(()),
            Ok(true) => {}
        }
    }

    Ok(Box::new(ClusterImpl {
        file: file,
        image_index: cluster_index as usize,
    }) as Box<container::Cluster + 'a>)
}

struct ClusterImpl<'a> {
    file: &'a RefCell<FileType>,
    image_index: usize,
}

impl<'a> container::Cluster for ClusterImpl<'a> {
    fn read_frame<'b>(&'b self, frame_index: i32, _: c_long)
                  -> Result<Box<container::Frame + 'b>,()> {
        if frame_index == 0 {
            Ok(Box::new(FrameImpl {
                file: self.file,
                image_index: self.image_index,
            }) as Box<container::Frame + 'b>)
        } else {
            Err(())
        }
    }
}

struct FrameImpl<'a> {
    file: &'a RefCell<FileType>,
    image_index: usize,
}

impl<'a> container::Frame for FrameImpl<'a> {
    fn len(&self) -> c_long {
        let file = self.file.borrow();
        let saved_image = &file.saved_images()[self.image_index];
        let color_map = match saved_image.image_desc().color_map() {
            Some(map) => map,
            None => file.color_map().unwrap(),
        };
        (2 + (color_map.colors().len() * 3) + saved_image.raster_bits().len()) as c_long
    }

    fn read(&self, buffer: &mut [u8]) -> Result<(),()> {
        let file = self.file.borrow();
        let saved_image = &file.saved_images()[self.image_index];
        let mut writer = BufWriter::new(buffer);
        let color_map = match saved_image.image_desc().color_map() {
            Some(map) => map,
            None => file.color_map().unwrap(),
        };

        if writer.write_u16::<LittleEndian>(color_map.colors().len() as u16).is_err() {
            return Err(())
        }
        for color in color_map.colors().iter() {
            if writer.write_all(&[color.Red, color.Green, color.Blue]).is_err() {
                return Err(())
            }
        }
        match writer.write_all(saved_image.raster_bits()) {
            Ok(_) => Ok(()),
            Err(_) => Err(()),
        }
    }

    fn track_number(&self) -> c_long {
        0
    }

    fn time(&self) -> Timestamp {
        get_time(self.file, self.image_index)
    }

    fn rendering_offset(&self) -> i64 {
        0
    }
}

/// FIXME(pcwalton): This is O(n)!
fn get_time(file: &RefCell<FileType>, image_index: usize) -> Timestamp {
    let mut time_so_far = 0;
    for (i, saved_image) in file.borrow().saved_images().iter().enumerate() {
        if i >= image_index {
            break
        }
        for j in 0..saved_image.extension_block_count() {
            if let ExtensionBlock::Graphics(block) = saved_image.extension_block(j) {
                time_so_far = time_so_far + block.delay_time() as i64
            }
        }
    }
    if time_so_far == 0 {
        for i in 0..file.borrow().extension_block_count() {
            if let ExtensionBlock::Graphics(block) = file.borrow().extension_block(i) {
                time_so_far = time_so_far + block.delay_time() as i64
            }
        }
    }
    Timestamp {
        ticks: time_so_far,
        ticks_per_second: 100.0,
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
    width: c_int,
    height: c_int,
}

impl VideoDecoderImpl {
    fn new(_: &videodecoder::VideoHeaders, width: i32, height: i32)
           -> Result<Box<videodecoder::VideoDecoder + 'static>,()> {
        Ok(Box::new(VideoDecoderImpl {
            width: width,
            height: height,
        }) as Box<videodecoder::VideoDecoder + 'static>)
    }
}

impl videodecoder::VideoDecoder for VideoDecoderImpl {
    fn decode_frame(&self, data: &[u8], presentation_time: &Timestamp)
                    -> Result<Box<videodecoder::DecodedVideoFrame + 'static>,()> {
        let mut reader = BufReader::new(data);
        let palette_size = match reader.read_u16::<LittleEndian>() {
            Ok(size) => size,
            Err(_) => return Err(()),
        };
        let mut palette = Vec::new();
        let mut color_bytes = [0, 0, 0];
        for _ in 0 .. palette_size {
            match reader.read(&mut color_bytes) {
                Ok(3) => {
                    palette.push(RgbColor {
                        r: color_bytes[0],
                        g: color_bytes[1],
                        b: color_bytes[2],
                    })
                }
                _ => return Err(()),
            }
        }

        let mut pixels = vec![];
        match reader.read_to_end(&mut pixels) {
            Ok(_) => {}, // Should we check anything here?
            Err(_) => return Err(()),
        }

        Ok(Box::new(DecodedVideoFrameImpl {
            width: self.width,
            height: self.height,
            palette: palette,
            pixels: pixels,
            presentation_time: *presentation_time,
        }) as Box<videodecoder::DecodedVideoFrame>)
    }
}

struct DecodedVideoFrameImpl {
    width: i32,
    height: i32,
    palette: Vec<RgbColor>,
    pixels: Vec<u8>,
    presentation_time: Timestamp,
}

impl videodecoder::DecodedVideoFrame for DecodedVideoFrameImpl {
    fn width(&self) -> c_uint {
        self.width as c_uint
    }

    fn height(&self) -> c_uint {
        self.height as c_uint
    }

    fn stride(&self, _: usize) -> c_int {
        self.width
    }

    fn pixel_format<'a>(&'a self) -> PixelFormat<'a> {
        PixelFormat::Indexed(Palette {
            palette: &self.palette,
        })
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
        id: [ b'G', b'I', b'F', b'f' ],
        constructor: VideoDecoderImpl::new,
    };

pub mod ffi {
    use libc::{c_char, c_int, c_uchar, c_uint, c_void, size_t};

    use containers::gif::SavedImage;

    pub const GIF_ERROR: c_int = 0;
    pub const GIF_OK: c_int = 1;

    pub const CONTINUE_EXT_FUNC_CODE: c_int = 0x00;
    pub const COMMENT_EXT_FUNC_CODE: c_int = 0xfe;
    pub const GRAPHICS_EXT_FUNC_CODE: c_int = 0xf9;
    pub const PLAINTEXT_EXT_FUNC_CODE: c_int = 0x01;
    pub const APPLICATION_EXT_FUNC_CODE: c_int = 0xff;

    pub const DISPOSAL_UNSPECIFIED: c_int = 0;
    pub const DISPOSE_DO_NOT: c_int = 1;
    pub const DISPOSE_BACKGROUND: c_int = 2;
    pub const DISPOSE_PREVIOUS: c_int = 3;

    pub const NO_TRANSPARENT_COLOR: c_int = -1;

    pub const UNDEFINED_RECORD_TYPE: GifRecordType = 0;
    pub const SCREEN_DESC_RECORD_TYPE: GifRecordType = 1;
    pub const IMAGE_DESC_RECORD_TYPE: GifRecordType = 2;
    pub const EXTENSION_RECORD_TYPE: GifRecordType = 3;
    pub const TERMINATE_RECORD_TYPE: GifRecordType = 4;

    pub type GifPixelType = c_uchar;
    pub type GifRowType = *mut c_uchar;
    pub type GifByteType = c_uchar;
    pub type GifPrefixType = c_uint;
    pub type GifWord = c_int;
    pub type GifRecordType = c_int;
    pub type InputFunc = extern "C" fn(*mut GifFileType, *mut GifByteType, c_int) -> c_int;

    #[repr(C)]
    pub struct GifFileType {
        pub SWidth: GifWord,
        pub SHeight: GifWord,
        pub SColorResolution: GifWord,
        pub SBackGroundColor: GifWord,
        pub AspectByte: GifByteType,
        pub SColorMap: *mut ColorMapObject,
        pub ImageCount: c_int,
        pub Image: GifImageDesc,
        pub SavedImages: *mut SavedImage,
        pub ExtensionBlockCount: c_int,
        pub ExtensionBlocks: *mut ExtensionBlock,
        pub Error: c_int,
        pub UserData: *mut c_void,
        pub Private: *mut c_void,
    }

    #[repr(C)]
    #[allow(missing_copy_implementations)]
    pub struct GifImageDesc {
        pub Left: GifWord,
        pub Top: GifWord,
        pub Width: GifWord,
        pub Height: GifWord,
        pub Interlace: bool,
        pub ColorMap: *mut ColorMapObject,
    }

    #[repr(C)]
    #[allow(missing_copy_implementations)]
    pub struct ColorMapObject {
        pub ColorCount: c_int,
        pub BitsPerPixel: c_int,
        pub SortFlag: bool,
        pub Colors: *mut GifColorType,
    }

    #[repr(C)]
    #[allow(missing_copy_implementations)]
    pub struct GifColorType {
        pub Red: GifByteType,
        pub Green: GifByteType,
        pub Blue: GifByteType,
    }

    #[repr(C)]
    #[allow(missing_copy_implementations)]
    pub struct ExtensionBlock {
        pub ByteCount: c_int,
        pub Bytes: *mut GifByteType,
        pub Function: c_int,
    }

    #[repr(C)]
    #[allow(missing_copy_implementations)]
    pub struct GraphicsControlBlock {
        pub DisposalMode: c_int,
        pub UserInputFlag: bool,
        pub DelayTime: c_int,
        pub TransparentColor: c_int,
    }

    #[link(name = "gif")]
    extern {
        pub fn DGifOpenFileName(GifFileType: *const c_char, Error: *mut c_int) -> *mut GifFileType;
        pub fn DGifOpenFileHandle(GifFileHandle: c_int, Error: *mut c_int) -> *mut GifFileType;
        pub fn DGifOpen(userPtr: *mut c_void, readFunc: InputFunc, Error: *mut c_int)
                        -> *mut GifFileType;
        pub fn DGifSlurp(GifFile: *mut GifFileType) -> c_int;
        pub fn DGifCloseFile(GifFile: *mut GifFileType, ErrorCode: *mut c_int) -> c_int;
        pub fn DGifGetRecordType(GifFile: *mut GifFileType, GifType: *mut GifRecordType) -> c_int;
        pub fn DGifGetImageDesc(GifFile: *mut GifFileType) -> c_int;
        pub fn DGifGetLine(GifFile: *mut GifFileType,
                           GifLine: *mut GifPixelType,
                           GifLineLen: c_int)
                           -> c_int;
        pub fn DGifGetExtension(GifFile: *mut GifFileType,
                                GifExtCode: *mut c_int,
                                GifExtension: *mut *mut GifByteType)
                                -> c_int;
        pub fn DGifGetExtensionNext(GifFile: *mut GifFileType, GifExtension: *mut *mut GifByteType)
                                    -> c_int;
        pub fn GifAddExtensionBlock(ExtensionBlock_Count: *mut c_int,
                                    ExtensionBlocks: *mut *mut ExtensionBlock,
                                    Function: c_int,
                                    Len: c_uint,
                                    ExtData: *mut c_uchar)
                                    -> c_int;
        pub fn DGifExtensionToGCB(GifExtensionLength: size_t,
                                  GifExtension: *const GifByteType,
                                  GCB: *mut GraphicsControlBlock)
                                  -> c_int;
    }
}

