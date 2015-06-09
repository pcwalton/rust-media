// Copyright 2015 The Servo Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

use audiodecoder;
use codecs::aac::AacHeaders;
use container;
use pixelformat::PixelFormat;
use streaming::StreamReader;
use timing::Timestamp;
use videodecoder;
use utils;

use libc::{self, c_char, c_double, c_int, c_long, c_void};
use std::ffi::{CString, CStr};
use std::mem;
use std::io::SeekFrom;
use std::ptr;
use std::slice::bytes;
use std::slice;
use std::str::{self, FromStr};

pub struct Mp4FileHandle {
    handle: ffi::MP4FileHandle,
}

impl Drop for Mp4FileHandle {
    fn drop(&mut self) {
        unsafe {
            ffi::MP4Close(self.handle, 0)
        }
    }
}

impl Mp4FileHandle {
    pub fn read(reader: Box<StreamReader>) -> Result<Mp4FileHandle,()> {
        // Ugh. Just… ugh, ugh, ugh. The only thing we can pass to the open callback that
        // constructs the user data is a UTF-8 encoded string. So we encode the pointer to the
        // stream reader as a string and decode it in the callback.
        let handle = unsafe {
            let address = mem::transmute::<Box<Box<_>>,*mut c_void>(Box::new(reader));
            let fake_path = format!("{}", address as usize);
            let fake_path = CString::new(fake_path.as_bytes()).unwrap();
            ffi::MP4ReadProvider(fake_path.as_ptr(), &FILE_PROVIDER)
        };
        if !handle.is_null() {
            Ok(Mp4FileHandle {
                handle: handle,
            })
        } else {
            Err(())
        }
    }

    pub fn number_of_tracks(&self) -> u32 {
        unsafe {
            ffi::MP4GetNumberOfTracks(self.handle, ptr::null(), 0)
        }
    }

    pub fn find_track_id(&self, index: u16) -> ffi::MP4TrackId {
        unsafe {
            ffi::MP4FindTrackId(self.handle, index, ptr::null(), 0)
        }
    }

    pub fn have_track_atom(&self, track_id: ffi::MP4TrackId, atom_name: &[u8]) -> bool {
        unsafe {
            let atom_name = CString::new(atom_name).unwrap();
            ffi::MP4HaveTrackAtom(self.handle, track_id, atom_name.as_ptr())
        }
    }

    pub fn track_type(&self, track_id: ffi::MP4TrackId) -> [u8; 4] {
        unsafe {
            let track_type = ffi::MP4GetTrackType(self.handle, track_id);
            assert!(!track_type.is_null());
            assert!(libc::strlen(track_type) >= 4);
            [
                *track_type.offset(0) as u8,
                *track_type.offset(1) as u8,
                *track_type.offset(2) as u8,
                *track_type.offset(3) as u8,
            ]
        }
    }

    pub fn track_media_data_name(&self, track_id: ffi::MP4TrackId) -> [u8; 4] {
        unsafe {
            let track_media_data_name = ffi::MP4GetTrackType(self.handle, track_id);
            assert!(!track_media_data_name.is_null());
            assert!(libc::strlen(track_media_data_name) >= 4);
            [
                *track_media_data_name.offset(0) as u8,
                *track_media_data_name.offset(1) as u8,
                *track_media_data_name.offset(2) as u8,
                *track_media_data_name.offset(3) as u8,
            ]
        }
    }

    pub fn number_of_samples(&self, track_id: ffi::MP4TrackId) -> ffi::MP4SampleId {
        unsafe {
            ffi::MP4GetTrackNumberOfSamples(self.handle, track_id)
        }
    }

    pub fn width(&self, track_id: ffi::MP4TrackId) -> u16 {
        unsafe {
            ffi::MP4GetTrackVideoWidth(self.handle, track_id)
        }
    }

    pub fn height(&self, track_id: ffi::MP4TrackId) -> u16 {
        unsafe {
            ffi::MP4GetTrackVideoHeight(self.handle, track_id)
        }
    }

    pub fn frame_rate(&self, track_id: ffi::MP4TrackId) -> c_double {
        unsafe {
            ffi::MP4GetTrackVideoFrameRate(self.handle, track_id)
        }
    }

    pub fn bit_rate(&self, track_id: ffi::MP4TrackId) -> u32 {
        unsafe {
            ffi::MP4GetTrackBitRate(self.handle, track_id)
        }
    }

    pub fn time_scale(&self, track_id: ffi::MP4TrackId) -> u32 {
        unsafe {
            ffi::MP4GetTrackTimeScale(self.handle, track_id)
        }
    }

    pub fn audio_channels(&self, track_id: ffi::MP4TrackId) -> c_int {
        unsafe {
            ffi::MP4GetTrackAudioChannels(self.handle, track_id)
        }
    }

    pub fn integer_property(&self, track_id: ffi::MP4TrackId, property_name: &[u8])
                            -> Result<u64,()> {
        let property_name = CString::new(property_name).unwrap();
        let mut value = 0;
        unsafe {
            let ok = ffi::MP4GetTrackIntegerProperty(self.handle,
                                                     track_id,
                                                     property_name.as_ptr(),
                                                     &mut value);
            if ok {
                Ok(value)
            } else {
                Err(())
            }
        }
    }

    pub fn bytes_property<'a>(&'a self, track_id: ffi::MP4TrackId, property_name: &[u8])
                              -> Result<&'a [u8],()> {
        let property_name = CString::new(property_name).unwrap();
        let (mut value, mut value_size) = (ptr::null_mut(), 0);
        unsafe {
            let ok = ffi::MP4GetTrackBytesProperty(self.handle,
                                                   track_id,
                                                   property_name.as_ptr(),
                                                   &mut value,
                                                   &mut value_size);
            if ok {
                Ok(mem::transmute::<&mut [u8],
                                    &'a [u8]>(slice::from_raw_parts_mut(value,
                                                                      value_size as usize)))
            } else {
                Err(())
            }
        }
    }

    pub fn read_sample<'a>(&'a self, track_id: ffi::MP4TrackId, sample_id: ffi::MP4SampleId)
                           -> Result<Sample<'a>,()> {
        let mut bytes = ptr::null_mut();
        let mut num_bytes = 0;
        let mut start_time = 0;
        let mut duration = 0;
        let mut rendering_offset = 0;
        let mut is_sync_sample = false;
        unsafe {
            if !ffi::MP4ReadSample(self.handle,
                                   track_id,
                                   sample_id,
                                   &mut bytes,
                                   &mut num_bytes,
                                   &mut start_time,
                                   &mut duration,
                                   &mut rendering_offset,
                                   &mut is_sync_sample) {
                return Err(())
            }
            Ok(Sample {
                bytes: slice::from_raw_parts_mut(bytes, num_bytes as usize),
                start_time: start_time,
                duration: duration,
                rendering_offset: rendering_offset,
                is_sync_sample: is_sync_sample,
            })
        }
    }

	pub fn raw_es_configuration(&self, track_id: ffi::MP4TrackId) -> Result<AacHeaders,()> {
        let result: Vec<u8> = unsafe {
            let (mut value, mut value_size) = (ptr::null_mut(), 0);
            let ok = ffi::MP4GetTrackRawESConfiguration(self.handle,
                                                        track_id,
                                                        &mut value,
                                                        &mut value_size);
            if ok {
                slice::from_raw_parts_mut(value, value_size as usize).iter().map(|x| *x).collect()
            } else {
                return Err(())
            }
        };
        Ok(AacHeaders {
            esds_chunk: result,
        })
    }

	pub fn h264_headers(&self, track_id: ffi::MP4TrackId) -> Result<H264Headers,()> {
		unsafe {
			let (mut profile, mut level) = (0, 0);
			ffi::MP4GetTrackH264ProfileLevel(self.handle, track_id, &mut profile, &mut level);
		}

		let (mut seq_headers, mut pict_header) = (ptr::null_mut(), ptr::null_mut());
		let (mut seq_header_size, mut pict_header_size) = (ptr::null_mut(), ptr::null_mut());
		let ok = unsafe {
			ffi::MP4GetTrackH264SeqPictHeaders(self.handle,
										       track_id,
										       &mut seq_headers,
										       &mut seq_header_size,
										       &mut pict_header,
										       &mut pict_header_size)
		};
		if ok {
			Ok(H264Headers {
				seq_headers: seq_headers,
				seq_header_size: seq_header_size,
				pict_header: pict_header,
				pict_header_size: pict_header_size,
			})
		} else {
			Err(())
		}
	}

    fn time_to_timestamp(&self, ticks: i64, track_id: ffi::MP4TrackId) -> Timestamp {
        Timestamp {
            ticks: ticks,
            ticks_per_second: self.time_scale(track_id) as f64,
        }
    }
}

static FILE_PROVIDER: ffi::MP4FileProvider = ffi::MP4FileProvider {
    open: file_provider_open,
    seek: file_provider_seek,
    read: file_provider_read,
    write: file_provider_write,
    close: file_provider_close,
    getSize: file_provider_get_size,
};

// See the comment in `Mp4FileHandle::read()` for an explanation of what's going on here.
extern "C" fn file_provider_open(name: *const c_char, _: ffi::MP4FileMode) -> *mut c_void {
    unsafe {
        mem::transmute::<usize,*mut c_void>(
            FromStr::from_str(str::from_utf8(CStr::from_ptr(name).to_bytes()).unwrap()).unwrap())
    }
}

extern "C" fn file_provider_seek(mut handle: *mut c_void, pos: i64) -> c_int {
    unsafe {
        let reader: &mut Box<Box<StreamReader>> = mem::transmute(&mut handle);
        if reader.seek(SeekFrom::Start(pos as u64)).is_ok() {
            0
        } else {
            1
        }
    }
}

extern "C" fn file_provider_read(mut handle: *mut c_void,
                                 buffer: *mut c_void,
                                 size: i64,
                                 nin: *mut i64,
                                 _: i64)
                                 -> c_int {
    if size < 0 {
        return 1
    }

    unsafe {
        let reader: &mut Box<Box<StreamReader>> = mem::transmute(&mut handle);
        match utils::read_to_full(reader,
                                  slice::from_raw_parts_mut(buffer as *mut u8, size as usize)) {
            Ok(_) => {
                *nin = size;
                0
            }
            Err(_) => 1,
        }
    }
}

extern "C" fn file_provider_write(_: *mut c_void, _: *const c_void, _: i64, _: *mut i64, _: i64)
                                  -> c_int {
    1
}

extern "C" fn file_provider_close(handle: *mut c_void) -> c_int {
    unsafe {
        drop(mem::transmute::<_,Box<Box<StreamReader>>>(handle))
    }
    0
}

extern "C" fn file_provider_get_size(mut handle: *mut c_void, nout: *mut i64) -> c_int {
    unsafe {
        let reader: &mut Box<Box<StreamReader>> = mem::transmute(&mut handle);
        *nout = reader.total_size() as i64;
    }
    0
}

pub struct Sample<'a> {
    pub bytes: &'a [u8],
    pub start_time: ffi::MP4Timestamp,
    pub duration: ffi::MP4Duration,
    pub rendering_offset: ffi::MP4Duration,
    pub is_sync_sample: bool,
}

pub struct H264Headers {
	seq_headers: *mut *mut u8,
	seq_header_size: *mut u32,
	pict_header: *mut *mut u8,
	pict_header_size: *mut u32,
}

impl Drop for H264Headers {
	fn drop(&mut self) {
		unsafe {
			ffi::MP4FreeH264SeqPictHeaders(self.seq_headers,
									       self.seq_header_size,
									       self.pict_header,
									       self.pict_header_size)
		}
	}
}

impl H264Headers {
	pub fn seq_headers<'a>(&'a self) -> Vec<&'a [u8]> {
		let mut result: Vec<&'a [u8]> = Vec::new();
		unsafe {
			let (mut header_ptr, mut header_size_ptr) = (self.seq_headers, self.seq_header_size);
			while !(*header_ptr).is_null() {
				result.push(slice::from_raw_parts_mut(*header_ptr, *header_size_ptr as usize));
				header_ptr = header_ptr.offset(1);
				header_size_ptr = header_size_ptr.offset(1);
			}
		}
		result
	}

	pub fn pict_headers<'a>(&'a self) -> Vec<&'a [u8]> {
		let mut result: Vec<&'a [u8]> = Vec::new();
		unsafe {
			let (mut header_ptr, mut header_size_ptr) = (self.pict_header, self.pict_header_size);
			while !(*header_ptr).is_null() {
				result.push(slice::from_raw_parts_mut(*header_ptr, *header_size_ptr as usize));
				header_ptr = header_ptr.offset(1);
				header_size_ptr = header_size_ptr.offset(1);
			}
		}
		result
	}
}

// Implementation of the abstract `ContainerReader` interface

struct ContainerReaderImpl {
    handle: Mp4FileHandle,
}

impl ContainerReaderImpl {
    fn new(reader: Box<StreamReader>) -> Result<Box<container::ContainerReader + 'static>,()> {
        let handle = match Mp4FileHandle::read(reader) {
            Ok(handle) => handle,
            Err(_) => return Err(()),
        };

        Ok(Box::new(ContainerReaderImpl {
            handle: handle,
        }) as Box<container::ContainerReader + 'static>)
    }
}

impl container::ContainerReader for ContainerReaderImpl {
    fn track_count(&self) -> u16 {
        self.handle.number_of_tracks() as u16
    }

    fn track_by_index<'a>(&'a self, index: u16) -> Box<container::Track + 'a> {
        Box::new(TrackImpl {
            id: self.handle.find_track_id(index),
            handle: &self.handle,
        }) as Box<container::Track + 'a>
    }

    fn track_by_number<'a>(&'a self, number: c_long) -> Box<container::Track + 'a> {
        Box::new(TrackImpl {
            id: number as ffi::MP4TrackId,
            handle: &self.handle,
        }) as Box<container::Track + 'a>
    }
}

pub struct TrackImpl<'a> {
    id: ffi::MP4TrackId,
    handle: &'a Mp4FileHandle,
}

impl<'a> container::Track<'a> for TrackImpl<'a> {
    fn track_type(self: Box<Self>) -> container::TrackType<'a> {
        let track_type = self.handle.track_type(self.id);

        if track_type == ffi::MP4_VIDEO_TRACK_TYPE {
            container::TrackType::Video(Box::new(VideoTrackImpl {
                id: self.id,
                handle: self.handle,
            }) as Box<container::VideoTrack + 'a>)
        } else if track_type == ffi::MP4_AUDIO_TRACK_TYPE {
            container::TrackType::Audio(Box::new(AudioTrackImpl {
                id: self.id,
                handle: self.handle,
            }) as Box<container::AudioTrack + 'a>)
        } else {
            container::TrackType::Other(self as Box<container::Track<'a> + 'a>)
        }
    }

    fn is_video(&self) -> bool {
        self.handle.track_type(self.id) == ffi::MP4_VIDEO_TRACK_TYPE
    }

    fn is_audio(&self) -> bool {
        self.handle.track_type(self.id) == ffi::MP4_AUDIO_TRACK_TYPE
    }

    fn cluster_count(&self) -> Option<c_int> {
        Some(1)
    }

    fn number(&self) -> c_long {
        self.id as c_long
    }

    fn codec(&self) -> Option<Vec<u8>> {
        get_codec(self.handle, self.id)
    }

    fn cluster<'b>(&'b self, cluster_index: i32) -> Result<Box<container::Cluster + 'b>,()> {
        assert!(cluster_index == 0);
        Ok(Box::new(ClusterImpl {
            handle: self.handle,
        }) as Box<container::Cluster + 'a>)
    }
}

#[derive(Clone)]
pub struct VideoTrackImpl<'a> {
    id: ffi::MP4TrackId,
    handle: &'a Mp4FileHandle,
}

impl<'a> container::Track<'a> for VideoTrackImpl<'a> {
    fn track_type(self: Box<Self>) -> container::TrackType<'a> {
        container::TrackType::Video(Box::new((*self).clone()) as Box<container::VideoTrack + 'a>)
    }

    fn is_video(&self) -> bool { true }
    fn is_audio(&self) -> bool { false }

    fn cluster_count(&self) -> Option<c_int> {
        Some(1)
    }

    fn number(&self) -> c_long {
        self.id as c_long
    }

    fn codec(&self) -> Option<Vec<u8>> {
        get_codec(self.handle, self.id)
    }

    fn cluster<'b>(&'b self, cluster_index: i32) -> Result<Box<container::Cluster + 'b>,()> {
        if cluster_index != 0 {
            return Err(())
        }
        Ok(Box::new(ClusterImpl {
            handle: self.handle,
        }) as Box<container::Cluster + 'a>)
    }
}

impl<'a> container::VideoTrack<'a> for VideoTrackImpl<'a> {
    fn width(&self) -> u16 {
        self.handle.width(self.id)
    }

    fn height(&self) -> u16 {
        self.handle.height(self.id)
    }

    fn frame_rate(&self) -> f64 {
        self.handle.frame_rate(self.id)
    }

    fn pixel_format(&self) -> PixelFormat<'static> {
        PixelFormat::I420
    }

	fn headers(&self) -> Box<videodecoder::VideoHeaders> {
		match self.handle.h264_headers(self.id) {
			Ok(headers) => {
				Box::new(VideoHeadersH264Impl {
					headers: headers,
				}) as Box<videodecoder::VideoHeaders>
			}
			Err(_) => {
                Box::new(videodecoder::EmptyVideoHeadersImpl) as Box<videodecoder::VideoHeaders>
            }
		}
	}
}

#[derive(Clone)]
pub struct AudioTrackImpl<'a> {
    id: ffi::MP4TrackId,
    handle: &'a Mp4FileHandle,
}

impl<'a> container::Track<'a> for AudioTrackImpl<'a> {
    fn track_type(self: Box<Self>) -> container::TrackType<'a> {
        container::TrackType::Audio(Box::new((*self).clone()) as Box<container::AudioTrack<'a> + 'a>)
    }

    fn is_video(&self) -> bool { false }
    fn is_audio(&self) -> bool { false }

    fn cluster_count(&self) -> Option<c_int> {
        Some(1)
    }

    fn number(&self) -> c_long {
        self.id as c_long
    }

    fn codec(&self) -> Option<Vec<u8>> {
        get_codec(self.handle, self.id)
    }

    fn cluster<'b>(&'b self, cluster_index: i32) -> Result<Box<container::Cluster + 'b>,()> {
        assert!(cluster_index == 0);
        Ok(Box::new(ClusterImpl {
            handle: self.handle,
        }) as Box<container::Cluster + 'a>)
    }
}

impl<'a> container::AudioTrack<'a> for AudioTrackImpl<'a> {
    fn channels(&self) -> u16 {
        // FIXME(pcwalton): This was determined experimentally and I was unable to find
        // documentation that matches the MP4 examples I have. Is it right?
        match self.handle.audio_channels(self.id) {
            3 => {
                // Surround sound
                6
            }
            count => count as u16,
        }
    }

    fn sampling_rate(&self) -> f64 {
        self.handle.time_scale(self.id) as f64
    }

	fn headers(&self) -> Box<audiodecoder::AudioHeaders> {
        let esds_chunk = self.handle.raw_es_configuration(self.id).unwrap();
		Box::new(esds_chunk) as Box<audiodecoder::AudioHeaders>
	}
}

pub struct ClusterImpl<'a> {
    handle: &'a Mp4FileHandle,
}

impl<'a> container::Cluster for ClusterImpl<'a> {
    fn read_frame<'b>(&'b self, frame_index: i32, track_number: c_long)
                      -> Result<Box<container::Frame + 'b>,()> {
        let sample = try!(self.handle.read_sample(track_number as ffi::MP4TrackId,
                                                  frame_index as u32 + 1));
        Ok(Box::new(FrameImpl {
            track_id: track_number as ffi::MP4TrackId,
            sample: sample,
            handle: self.handle,
        }) as Box<container::Frame + 'b>)
    }
}

pub struct FrameImpl<'a> {
    sample: Sample<'a>,
    handle: &'a Mp4FileHandle,
    track_id: ffi::MP4TrackId,
}

impl<'a> container::Frame for FrameImpl<'a> {
    fn len(&self) -> c_long {
        self.sample.bytes.len() as c_long
    }

    fn read(&self, buffer: &mut [u8]) -> Result<(),()> {
        bytes::copy_memory(self.sample.bytes, buffer);
        Ok(())
    }

    fn track_number(&self) -> c_long {
        self.track_id as c_long
    }

    fn time(&self) -> Timestamp {
        self.handle.time_to_timestamp(self.sample.start_time as i64, self.track_id)
    }

    fn rendering_offset(&self) -> i64 {
        // NB: `mp4v2` seems to use the wrong type for rendering offsets. It's clearly a signed
        // 32-bit integer. Work around this oversight.
        self.sample.rendering_offset as i32 as i64
    }
}

pub struct VideoHeadersH264Impl {
	headers: H264Headers,
}

impl videodecoder::VideoHeaders for VideoHeadersH264Impl {
	fn h264_seq_headers<'a>(&'a self) -> Option<Vec<&'a [u8]>> {
		Some(self.headers.seq_headers())
	}

	fn h264_pict_headers<'a>(&'a self) -> Option<Vec<&'a [u8]>> {
		Some(self.headers.pict_headers())
	}
}

fn get_codec(handle: &Mp4FileHandle, id: ffi::MP4TrackId) -> Option<Vec<u8>> {
    static TABLE: [(&'static [u8], [u8; 4]); 3] = [
        (b"avc1", [b'a', b'v', b'c', b' ']),
        (b"mp4v", [b'a', b'v', b'c', b' ']),
        (b"mp4a", [b'a', b'a', b'c', b' ']),
    ];
    for &(key, value) in TABLE.iter() {
        let mut path: Vec<u8> = b"mdia.minf.stbl.stsd.".iter().map(|x| *x).collect();
        path.push_all(key);
        if handle.have_track_atom(id, &path) {
            return Some(value.iter().cloned().collect())
        }
    }
    None
}

pub const CONTAINER_READER: container::RegisteredContainerReader =
    container::RegisteredContainerReader {
        mime_types: &[
            "audio/mp4",
            "audio/quicktime",
            "video/mp4",
            "video/quicktime",
        ],
        read: ContainerReaderImpl::new,
    };

#[allow(missing_copy_implementations)]
#[allow(non_snake_case)]
pub mod ffi {
    use libc::{c_char, c_double, c_int, c_void};

    #[repr(C)]
    pub struct MP4FileHandleStruct;
    #[repr(C)]
    pub struct MP4FileProvider {
        pub open: extern "C" fn(name: *const c_char, mode: MP4FileMode) -> *mut c_void,
        pub seek: extern "C" fn(handle: *mut c_void, pos: i64) -> c_int,
        pub read: extern "C" fn(handle: *mut c_void,
                                buffer: *mut c_void,
                                size: i64,
                                nin: *mut i64,
                                maxChunkSize: i64)
                                -> c_int,
        pub write: extern "C" fn(handle: *mut c_void,
                                 buffer: *const c_void,
                                 size: i64,
                                 nout: *mut i64,
                                 maxChunkSize: i64)
                                 -> c_int,
        pub close: extern "C" fn(handle: *mut c_void) -> c_int,
        pub getSize: extern "C" fn(handle: *mut c_void, nout: *mut i64) -> c_int,
    }

    pub type MP4FileHandle = *mut MP4FileHandleStruct;
    pub type MP4FileMode = c_int;
    pub type MP4TrackId = u32;
    pub type MP4SampleId = u32;
    pub type MP4Timestamp = u64;
    pub type MP4Duration = u64;
    pub type MP4EditId = u32;

    pub const MP4_OD_TRACK_TYPE: &'static [u8] = b"odsm";
    pub const MP4_SCENE_TRACK_TYPE: &'static [u8] = b"sdsm";
    pub const MP4_AUDIO_TRACK_TYPE: &'static [u8] = b"soun";
    pub const MP4_VIDEO_TRACK_TYPE: &'static [u8] = b"vide";
    pub const MP4_HINT_TRACK_TYPE: &'static [u8] = b"hint";
    pub const MP4_CNTL_TRACK_TYPE: &'static [u8] = b"cntl";
    pub const MP4_TEXT_TRACK_TYPE: &'static [u8] = b"text";
    pub const MP4_SUBTITLE_TRACK_TYPE: &'static [u8] = b"sbtl";
    pub const MP4_SUBPIC_TRACK_TYPE: &'static [u8] = b"subp";

    #[link(name = "mp4v2")]
    extern {
        pub fn MP4Close(hFile: MP4FileHandle, flags: u32);
        pub fn MP4Read(fileName: *const c_char) -> MP4FileHandle;
        pub fn MP4ReadProvider(fileName: *const c_char, fileProvider: *const MP4FileProvider)
                               -> MP4FileHandle;

        pub fn MP4ReadSample(hFile: MP4FileHandle,
                             trackId: MP4TrackId,
                             sampleId: MP4SampleId,
                             ppBytes: *mut *mut u8,
                             pNumBytes: *mut u32,
                             pStartTime: *mut MP4Timestamp,
                             pDuration: *mut MP4Duration,
                             pRenderingOffset: *mut MP4Duration,
                             pIsSyncSample: *mut bool)
                             -> bool;

        pub fn MP4GetNumberOfTracks(hFile: MP4FileHandle, trackType: *const c_char, subType: u8)
                                    -> u32;
        pub fn MP4FindTrackId(hFile: MP4FileHandle,
                              index: u16,
                              trackType: *const c_char,
                              subType: u8)
                              -> MP4TrackId;
        pub fn MP4HaveTrackAtom(hFile: MP4FileHandle, trackId: MP4TrackId, atomName: *const c_char)
                                -> bool;
        pub fn MP4GetTrackType(hFile: MP4FileHandle, trackId: MP4TrackId) -> *const c_char;
        pub fn MP4GetTrackMediaDataName(hFile: MP4FileHandle, trackId: MP4TrackId)
                                        -> *const c_char;
        pub fn MP4GetTrackNumberOfSamples(hFile: MP4FileHandle, trackId: MP4TrackId)
                                          -> MP4SampleId;
        pub fn MP4GetTrackBitRate(hFile: MP4FileHandle, trackId: MP4TrackId) -> u32;
        pub fn MP4GetTrackTimeScale(hFile: MP4FileHandle, trackId: MP4TrackId) -> u32;
        pub fn MP4GetTrackVideoWidth(hFile: MP4FileHandle, trackId: MP4TrackId) -> u16;
        pub fn MP4GetTrackVideoHeight(hFile: MP4FileHandle, trackId: MP4TrackId) -> u16;
        pub fn MP4GetTrackVideoFrameRate(hFile: MP4FileHandle, trackId: MP4TrackId) -> c_double;
        pub fn MP4GetTrackAudioChannels(hFile: MP4FileHandle, trackId: MP4TrackId) -> c_int;
        pub fn MP4GetTrackIntegerProperty(hFile: MP4FileHandle,
                                          trackId: MP4TrackId,
                                          propName: *const c_char,
                                          retvalue: *mut u64)
                                          -> bool;
        pub fn MP4GetTrackBytesProperty(hFile: MP4FileHandle,
                                        trackId: MP4TrackId,
                                        propName: *const c_char,
                                        ppValue: *mut *mut u8,
                                        pValueSize: *mut u32)
                                        -> bool;
        pub fn MP4GetTrackRawESConfiguration(hFile: MP4FileHandle,
                                             trackId: MP4TrackId,
                                             ppValue: *mut *mut u8,
                                             pValueSize: *mut u32)
                                             -> bool;
		pub fn MP4GetTrackH264ProfileLevel(hFile: MP4FileHandle,
										   trackId: MP4TrackId,
										   pProfile: *mut u8,
										   pLevel: *mut u8)
										   -> bool;
		pub fn MP4GetTrackH264SeqPictHeaders(hFile: MP4FileHandle,
											 trackId: MP4TrackId,
											 pSeqHeaders: *mut *mut *mut u8,
											 pSeqHeaderSize: *mut *mut u32,
											 pPictHeader: *mut *mut *mut u8,
											 pPictHeaderSize: *mut *mut u32)
											 -> bool;
		pub fn MP4FreeH264SeqPictHeaders(pSeqHeaders: *mut *mut u8,
										 pSeqHeaderSize: *mut u32,
										 pPictHeader: *mut *mut u8,
										 pPictHeaderSize: *mut u32);
    }
}

