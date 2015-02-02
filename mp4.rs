// Copyright 2015 The Servo Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

use container;
use decoder;

use libc::{c_double, c_int, c_long};
use std::ffi::CString;
use std::mem;
use std::ptr;
use std::slice::bytes;
use std::slice;

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
    pub fn read(path: &Path) -> Result<Mp4FileHandle,()> {
        let path = CString::from_slice(path.display().to_string().as_bytes().as_slice());
        let handle = unsafe {
            ffi::MP4Read(path.as_ptr())
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

    pub fn track_type(&self, track_id: ffi::MP4TrackId) -> [u8; 4] {
        unsafe {
            let track_type = ffi::MP4GetTrackType(self.handle, track_id);
            assert!(!track_type.is_null());
            [
                *track_type.offset(0) as u8,
                *track_type.offset(1) as u8,
                *track_type.offset(2) as u8,
                *track_type.offset(3) as u8,
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
                bytes: slice::from_raw_mut_buf(mem::transmute::<&_,&_>(&bytes),
                                               num_bytes as usize),
                start_time: start_time,
                duration: duration,
                rendering_offset: rendering_offset,
                is_sync_sample: is_sync_sample,
            })
        }
    }

	pub fn h264_headers(&self, track_id: ffi::MP4TrackId) -> Result<H264Headers,()> {
		unsafe {
			let (mut profile, mut level) = (0, 0);
			ffi::MP4GetTrackH264ProfileLevel(self.handle, track_id, &mut profile, &mut level);
			println!("profile {}, level {}", profile, level);
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
				result.push(slice::from_raw_mut_buf(&mut *header_ptr, *header_size_ptr as usize));
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
				result.push(slice::from_raw_mut_buf(&mut *header_ptr, *header_size_ptr as usize));
				header_ptr = header_ptr.offset(1);
				header_size_ptr = header_size_ptr.offset(1);
			}
		}
		result
	}
}

// Implementation of the abstract `ContainerReader` interface

pub struct ContainerReaderImpl {
    handle: Mp4FileHandle,
}

impl ContainerReaderImpl {
    pub fn read(path: &Path) -> Result<Box<container::ContainerReader + 'static>,()> {
        match Mp4FileHandle::read(path) {
            Ok(handle) => {
                Ok(Box::new(ContainerReaderImpl {
                    handle: handle,
                }) as Box<container::ContainerReader + 'static>)
            }
            Err(_) => Err(()),
        }
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
}

pub struct TrackImpl<'a> {
    id: ffi::MP4TrackId,
    handle: &'a Mp4FileHandle,
}

impl<'a> container::Track for TrackImpl<'a> {
    fn track_type(&self) -> container::TrackType {
        let track_type = self.handle.track_type(self.id);
        if track_type == ffi::MP4_VIDEO_TRACK_TYPE {
            container::TrackType::Video
        } else if track_type == ffi::MP4_AUDIO_TRACK_TYPE {
            container::TrackType::Audio
        } else {
            container::TrackType::Other
        }
    }

    fn cluster_count(&self) -> c_int {
        1
    }

    fn number(&self) -> c_long {
        self.id as c_long
    }

    fn as_video_track<'b>(&'b self) -> Result<Box<container::VideoTrack + 'b>,()> {
        if self.handle.track_type(self.id) != ffi::MP4_VIDEO_TRACK_TYPE {
            return Err(())
        }
        Ok(Box::new(VideoTrackImpl {
            id: self.id,
            handle: self.handle,
        }) as Box<container::VideoTrack + 'a>)
    }
}

#[derive(Clone)]
pub struct VideoTrackImpl<'a> {
    id: ffi::MP4TrackId,
    handle: &'a Mp4FileHandle,
}

impl<'a> container::Track for VideoTrackImpl<'a> {
    fn track_type(&self) -> container::TrackType {
        container::TrackType::Video
    }

    fn cluster_count(&self) -> c_int {
        1
    }

    fn number(&self) -> c_long {
        self.id as c_long
    }

    fn as_video_track<'b>(&'b self) -> Result<Box<container::VideoTrack + 'b>,()> {
        Ok(Box::new((*self).clone()) as Box<container::VideoTrack + 'a>)
    }
}

impl<'a> container::VideoTrack for VideoTrackImpl<'a> {
    fn width(&self) -> u16 {
        self.handle.width(self.id)
    }

    fn height(&self) -> u16 {
        self.handle.height(self.id)
    }

    fn frame_rate(&self) -> f64 {
        self.handle.frame_rate(self.id)
    }

    fn cluster<'b>(&'b self, cluster_index: i32) -> Box<container::Cluster + 'b> {
        assert!(cluster_index == 0);
        Box::new(ClusterImpl {
            id: self.id,
            handle: self.handle,
        }) as Box<container::Cluster + 'a>
    }

	fn headers(&self) -> Box<decoder::Headers> {
		match self.handle.h264_headers(self.id) {
			Ok(headers) => {
				Box::new(HeadersH264Impl {
					headers: headers,
				}) as Box<decoder::Headers>
			}
			Err(_) => Box::new(decoder::EmptyHeadersImpl) as Box<decoder::Headers>,
		}
	}
}

pub struct ClusterImpl<'a> {
    id: ffi::MP4TrackId,
    handle: &'a Mp4FileHandle,
}

impl<'a> container::Cluster for ClusterImpl<'a> {
    fn frame_count(&self) -> c_int {
        self.handle.number_of_samples(self.id) as c_int
    }

    fn read_frame<'b>(&'b self, frame_index: i32) -> Box<container::Frame + 'b> {
        Box::new(FrameImpl {
            sample: self.handle.read_sample(self.id, (frame_index + 1) as u32).unwrap(),
            track_id: self.id,
        }) as Box<container::Frame + 'b>
    }
}

pub struct FrameImpl<'a> {
    sample: Sample<'a>,
    track_id: ffi::MP4TrackId,
}

impl<'a> container::Frame for FrameImpl<'a> {
    fn len(&self) -> c_long {
        self.sample.bytes.len() as c_long
    }

    fn read(&self, buffer: &mut [u8]) -> Result<(),()> {
        bytes::copy_memory(buffer, self.sample.bytes);
        Ok(())
    }

    fn track_number(&self) -> c_long {
        self.track_id as c_long
    }
}

pub struct HeadersH264Impl {
	headers: H264Headers,
}

impl decoder::Headers for HeadersH264Impl {
	fn h264_seq_headers<'a>(&'a self) -> Option<Vec<&'a [u8]>> {
		Some(self.headers.seq_headers())
	}

	fn h264_pict_headers<'a>(&'a self) -> Option<Vec<&'a [u8]>> {
		Some(self.headers.pict_headers())
	}
}

#[allow(missing_copy_implementations)]
pub mod ffi {
    use libc::{c_char, c_double};

    #[repr(C)]
    pub struct MP4FileHandleStruct;

    pub type MP4FileHandle = *mut MP4FileHandleStruct;
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
        pub fn MP4GetTrackType(hFile: MP4FileHandle, trackId: MP4TrackId) -> *const c_char;
        pub fn MP4GetTrackNumberOfSamples(hFile: MP4FileHandle, trackId: MP4TrackId)
                                          -> MP4SampleId;
        pub fn MP4GetTrackVideoWidth(hFile: MP4FileHandle, trackId: MP4TrackId) -> u16;
        pub fn MP4GetTrackVideoHeight(hFile: MP4FileHandle, trackId: MP4TrackId) -> u16;
        pub fn MP4GetTrackVideoFrameRate(hFile: MP4FileHandle, trackId: MP4TrackId) -> c_double;
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

