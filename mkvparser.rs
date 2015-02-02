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

use libc::{c_char, c_double, c_int, c_long, c_longlong, c_uchar, c_ulong};
use std::ffi::CString;
use std::mem;
use std::num::FromPrimitive;
use std::ptr;

pub struct MkvReader {
    reader: WebmMkvReaderRef,
}

impl Drop for MkvReader {
    fn drop(&mut self) {
        unsafe {
            WebmMkvReaderDestroy(self.reader)
        }
    }
}

impl MkvReader {
    pub fn new() -> MkvReader {
        MkvReader {
            reader: unsafe {
                WebmMkvReaderCreate()
            },
        }
    }

    pub fn open(&self, path: &Path) -> Result<(),()> {
        unsafe {
            let path = CString::from_slice(path.display().to_string().as_bytes().as_slice());
            if WebmMkvReaderOpen(self.reader, path.as_ptr()) != 0 {
                Err(())
            } else {
                Ok(())
            }
        }
    }

    pub fn close(&self) {
        unsafe {
            WebmMkvReaderClose(self.reader)
        }
    }
}

pub struct EbmlHeader {
    header: WebmEbmlHeaderRef,
}

impl Drop for EbmlHeader {
    fn drop(&mut self) {
        unsafe {
            WebmEbmlHeaderDestroy(self.header);
        }
    }
}

impl EbmlHeader {
    pub fn new() -> EbmlHeader {
        EbmlHeader {
            header: unsafe {
                WebmEbmlHeaderCreate()
            },
        }
    }

    pub fn parse(&self, reader: &MkvReader) -> (Result<(),c_longlong>, c_longlong) {
        let mut pos = 0;
        let result = unsafe {
            WebmEbmlHeaderParse(self.header, reader.reader, &mut pos)
        };
        let result = if result == 0 {
            Ok(())
        } else {
            Err(result)
        };
        (result, pos)
    }
}

pub struct Segment {
    segment: WebmSegmentRef,
}

impl Drop for Segment {
    fn drop(&mut self) {
        unsafe {
            WebmSegmentDestroy(self.segment)
        }
    }
}

impl Segment {
    pub fn new(reader: &MkvReader, pos: c_longlong) -> Result<Segment,c_longlong> {
        let mut err = 0;
        let segment = unsafe {
            WebmSegmentCreate(reader.reader, pos, &mut err)
        };
        if err == 0 {
            Ok(Segment {
                segment: segment,
            })
        } else {
            Err(err)
        }
    }

    pub fn load(&self) -> Result<(),c_long> {
        let err = unsafe {
            WebmSegmentLoad(self.segment)
        };
        if err >= 0 {
            Ok(())
        } else {
            Err(err)
        }
    }

    pub fn tracks<'a>(&'a self) -> Option<Tracks<'a>> {
        let tracks = unsafe {
            WebmSegmentGetTracks(self.segment)
        };
        if tracks == ptr::null_mut() {
            return None
        }
        Some(Tracks {
            tracks: tracks,
        })
    }

    pub fn count(&self) -> c_ulong {
        unsafe {
            WebmSegmentGetCount(self.segment)
        }
    }

    pub fn first<'a>(&'a self) -> Option<Cluster<'a>> {
        let result = unsafe {
            WebmSegmentGetFirst(self.segment)
        };
        if result == ptr::null_mut() {
            None
        } else {
            Some(Cluster {
                cluster: result,
            })
        }
    }

    pub fn next<'a>(&'a self, cluster: Cluster<'a>) -> Option<Cluster<'a>> {
        let result = unsafe {
            WebmSegmentGetNext(self.segment, cluster.cluster)
        };
        if result == ptr::null_mut() {
            None
        } else {
            Some(Cluster {
                cluster: result,
            })
        }
    }
}

pub struct Tracks<'a> {
    tracks: WebmTracksRef,
}

impl<'a> Tracks<'a> {
    pub fn count(&self) -> c_ulong {
        unsafe {
            WebmTracksGetCount(self.tracks)
        }
    }

    pub fn track_by_index(&self, index: c_ulong) -> Track<'a> {
        Track {
            track: unsafe {
                WebmTracksGetTrackByIndex(self.tracks, index)
            }
        }
    }

    pub fn track_by_number(&self, number: c_long) -> Track<'a> {
        Track {
            track: unsafe {
                WebmTracksGetTrackByNumber(self.tracks, number)
            }
        }
    }
}

#[derive(Clone)]
pub struct Track<'a> {
    track: WebmTrackRef,
}

impl<'a> Track<'a> {
    pub fn track_type(&self) -> TrackType {
        unsafe {
            FromPrimitive::from_i32(WebmTrackGetType(self.track) as i32).unwrap()
        }
    }

    pub fn number(&self) -> c_long {
        unsafe {
            WebmTrackGetNumber(self.track)
        }
    }

    pub fn as_video_track(&self) -> VideoTrack<'a> {
        if self.track_type() != TrackType::Video {
            panic!("Track::as_video_track(): not a video track!")
        }
        VideoTrack {
            track: unsafe {
                mem::transmute::<_,WebmVideoTrackRef>(self.track)
            },
        }
    }
}

#[derive(Clone)]
pub struct VideoTrack<'a> {
    track: WebmVideoTrackRef,
}

impl<'a> VideoTrack<'a> {
    pub fn as_track(&self) -> Track<'a> {
        Track {
            track: unsafe {
                mem::transmute::<_,WebmTrackRef>(self.track)
            },
        }
    }

    pub fn width(&self) -> c_longlong {
        unsafe {
            WebmVideoTrackGetWidth(self.track)
        }
    }

    pub fn height(&self) -> c_longlong {
        unsafe {
            WebmVideoTrackGetHeight(self.track)
        }
    }

    pub fn frame_rate(&self) -> c_double {
        unsafe {
            WebmVideoTrackGetFrameRate(self.track)
        }
    }
}

#[derive(Clone)]
pub struct Cluster<'a> {
    cluster: WebmClusterRef,
}

impl<'a> Cluster<'a> {
    pub fn eos(&self) -> bool {
        unsafe {
            WebmClusterEos(self.cluster)
        }
    }

    pub fn first(&self) -> Result<BlockEntry<'a>,c_long> {
        let mut err = 0;
        let entry = unsafe {
            WebmClusterGetFirst(self.cluster, &mut err)
        };
        if err >= 0 {
            Ok(BlockEntry {
                entry: entry,
            })
        } else {
            Err(err)
        }
    }

    pub fn next(&self, entry: BlockEntry<'a>) -> Result<BlockEntry<'a>,c_long> {
        let mut err = 0;
        let entry = unsafe {
            WebmClusterGetNext(self.cluster, entry.entry, &mut err)
        };
        if err >= 0 && entry != ptr::null_mut() {
            Ok(BlockEntry {
                entry: entry,
            })
        } else {
            Err(err)
        }
    }

    pub fn entry_count(&self) -> c_long {
        unsafe {
            WebmClusterGetEntryCount(self.cluster)
        }
    }

    pub fn entry(&self, index: c_long) -> Result<BlockEntry<'a>,c_long> {
        let mut err = 0;
        let entry = unsafe {
            WebmClusterGetEntry(self.cluster, index, &mut err)
        };
        if err >= 0 {
            Ok(BlockEntry {
                entry: entry,
            })
        } else {
            Err(err)
        }
    }

    pub fn parse(&self) -> (Result<bool,c_long>, ClusterInfo) {
        let mut info = ClusterInfo {
            pos: 0,
            len: 0,
        };
        let result = unsafe {
            WebmClusterParse(self.cluster, &mut info.pos, &mut info.len)
        };
        if result >= 0 {
            (Ok(result == 0), info)
        } else {
            (Err(result), info)
        }
    }
}

#[derive(Clone, Copy)]
pub struct ClusterInfo {
    pub pos: c_longlong,
    pub len: c_long,
}

#[derive(Clone)]
pub struct BlockEntry<'a> {
    entry: WebmBlockEntryRef,
}

impl<'a> BlockEntry<'a> {
    pub fn block(&self) -> Block {
        unsafe {
            Block {
                block: WebmBlockEntryGetBlock(self.entry),
            }
        }
    }

    pub fn eos(&self) -> bool {
        unsafe {
            WebmBlockEntryEos(self.entry)
        }
    }
}

#[derive(Clone)]
pub struct Block<'a> {
    block: WebmBlockRef,
}

impl<'a> Block<'a> {
    pub fn frame_count(&self) -> c_int {
        unsafe {
            WebmBlockGetFrameCount(self.block)
        }
    }

    pub fn frame(&self, frame_index: c_int) -> BlockFrame<'a> {
        unsafe {
            BlockFrame {
                frame: WebmBlockGetFrame(self.block, frame_index)
            }
        }
    }

    pub fn track_number(&self) -> c_longlong {
        unsafe {
            WebmBlockGetTrackNumber(self.block)
        }
    }

    pub fn discard_padding(&self) -> c_longlong {
        unsafe {
            WebmBlockDiscardPadding(self.block)
        }
    }

    pub fn is_key(&self) -> bool {
        unsafe {
            WebmBlockIsKey(self.block)
        }
    }
}

pub struct BlockFrame<'a> {
    frame: WebmBlockFrameRef,
}

impl<'a> BlockFrame<'a> {
    pub fn pos(&self) -> c_longlong {
        unsafe {
            WebmBlockFrameGetPos(self.frame)
        }
    }

    pub fn len(&self) -> c_long {
        unsafe {
            WebmBlockFrameGetLen(self.frame)
        }
    }

    pub fn read(&self, reader: &MkvReader, buf: &mut [u8]) -> Result<c_long,c_long> {
        assert!(buf.len() as c_long >= self.len());
        let result = unsafe {
            WebmBlockFrameRead(self.frame, reader.reader, buf.as_mut_ptr())
        };
        if result >= 0 {
            Ok(result)
        } else {
            Err(result)
        }
    }
}

#[derive(Clone, Copy, FromPrimitive, PartialEq, Show)]
pub enum TrackType {
    Video = 1,
    Audio = 2,
    Subtitle = 0x11,
    Metadata = 0x21,
}

// Implementation of the abstract `ContainerReader` interface

pub struct ContainerReaderImpl {
    reader: MkvReader,
    segment: Segment,
}

impl ContainerReaderImpl {
    pub fn read(path: &Path) -> Result<Box<container::ContainerReader + 'static>,()> {
        let reader = MkvReader::new();
        if reader.open(path).is_err() {
            return Err(())
        }
        let (err, pos) = EbmlHeader::new().parse(&reader);
        if err.is_err() {
            return Err(())
        }
        let segment = match Segment::new(&reader, pos) {
            Ok(segment) => segment,
            Err(_) => return Err(()),
        };
        if segment.load().is_err() {
            return Err(())
        }
        Ok(Box::new(ContainerReaderImpl {
            reader: reader,
            segment: segment,
        }) as Box<container::ContainerReader>)
    }
}

impl container::ContainerReader for ContainerReaderImpl {
    fn track_count(&self) -> u16 {
        self.segment.tracks().unwrap().count() as u16
    }

    fn track_by_index<'a>(&'a self, index: u16) -> Box<container::Track + 'a> {
        Box::new(TrackImpl {
            track: self.segment.tracks().unwrap().track_by_index(index as u64),
            segment: &self.segment,
            reader: &self.reader,
        }) as Box<container::Track + 'a>
    }
}

struct TrackImpl<'a> {
    track: Track<'a>,
    segment: &'a Segment,
    reader: &'a MkvReader,
}

impl<'a> container::Track for TrackImpl<'a> {
    fn track_type(&self) -> container::TrackType {
        match self.track.track_type() {
            TrackType::Video => container::TrackType::Video,
            TrackType::Audio => container::TrackType::Audio,
            _ => container::TrackType::Other,
        }
    }

    fn cluster_count(&self) -> c_int {
        self.segment.count() as c_int
    }

    fn number(&self) -> c_long {
        self.track.number()
    }

    fn as_video_track<'b>(&'b self) -> Result<Box<container::VideoTrack + 'b>,()> {
        if self.track.track_type() != TrackType::Video {
            return Err(())
        }
        Ok(Box::new(VideoTrackImpl {
            track: self.track.as_video_track(),
            segment: self.segment,
            reader: self.reader,
        }) as Box<container::VideoTrack + 'a>)
    }
}

#[derive(Clone)]
struct VideoTrackImpl<'a> {
    track: VideoTrack<'a>,
    segment: &'a Segment,
    reader: &'a MkvReader,
}

impl<'a> container::Track for VideoTrackImpl<'a> {
    fn track_type(&self) -> container::TrackType {
        container::TrackType::Video
    }

    fn cluster_count(&self) -> c_int {
        self.segment.count() as c_int
    }

    fn number(&self) -> c_long {
        self.track.as_track().number()
    }

    fn as_video_track<'b>(&'b self) -> Result<Box<container::VideoTrack + 'b>,()> {
        Ok(Box::new((*self).clone()) as Box<container::VideoTrack + 'b>)
    }
}

impl<'a> container::VideoTrack for VideoTrackImpl<'a> {
    fn width(&self) -> u16 {
        self.track.width() as u16
    }

    fn height(&self) -> u16 {
        self.track.height() as u16
    }

    fn frame_rate(&self) -> c_double {
        self.track.frame_rate()
    }

    fn cluster<'b>(&'b self, cluster_index: i32) -> Box<container::Cluster + 'b> {
        let mut cluster = self.segment.first().unwrap();
        for _ in range(0, cluster_index) {
            cluster = self.segment.next(cluster).unwrap();
        }

        // Parse all entries.
        loop {
            let (err, _) = cluster.parse();
            if !err.unwrap() {
                break
            }
        }

        Box::new(ClusterImpl {
            cluster: cluster,
            reader: self.reader,
        }) as Box<container::Cluster + 'b>
    }

	fn headers(&self) -> Box<decoder::Headers> {
		// TODO(pcwalton): Support H.264.
		Box::new(decoder::EmptyHeadersImpl) as Box<decoder::Headers>
	}
}

struct ClusterImpl<'a> {
    cluster: Cluster<'a>,
    reader: &'a MkvReader,
}

impl<'a> container::Cluster for ClusterImpl<'a> {
    fn frame_count(&self) -> c_int {
        self.cluster.entry_count() as c_int
    }

    fn read_frame<'b>(&'b self, frame_index: i32) -> Box<container::Frame + 'b> {
        Box::new(FrameImpl {
            block: self.cluster.entry(frame_index as i64).unwrap().block(),
            reader: self.reader,
        }) as Box<container::Frame + 'b>
    }
}

struct FrameImpl<'a> {
    block: Block<'a>,
    reader: &'a MkvReader,
}

impl<'a> container::Frame for FrameImpl<'a> {
    fn len(&self) -> c_long {
        self.block.frame(0).len()
    }

    fn read(&self, buffer: &mut [u8]) -> Result<(),()> {
        match self.block.frame(0).read(self.reader, buffer) {
            Ok(_) => Ok(()),
            Err(_) => Err(()),
        }
    }

    fn track_number(&self) -> c_long {
        self.block.track_number()
    }
}

// FFI stuff

type WebmMkvReaderRef = *mut WebmMkvReader;
type WebmEbmlHeaderRef = *mut WebmEbmlReader;
type WebmSegmentRef = *mut WebmSegment;
type WebmTracksRef = *mut WebmTracks;
type WebmTrackRef = *mut WebmTrack;
type WebmVideoTrackRef = *mut WebmVideoTrack;
type WebmClusterRef = *mut WebmCluster;
type WebmBlockEntryRef = *mut WebmBlockEntry;
type WebmBlockRef = *mut WebmBlock;
type WebmBlockFrameRef = *mut WebmBlockFrame;

#[repr(C)]
struct WebmMkvReader;
#[repr(C)]
struct WebmEbmlReader;
#[repr(C)]
struct WebmSegment;
#[repr(C)]
struct WebmTracks;
#[repr(C)]
struct WebmTrack;
#[repr(C)]
struct WebmVideoTrack;
#[repr(C)]
struct WebmCluster;
#[repr(C)]
struct WebmBlockEntry;
#[repr(C)]
struct WebmBlock;
#[repr(C)]
struct WebmBlockFrame;

#[allow(dead_code)]
#[link(name="rustmedia")]
#[link(name="webm")]
#[link(name="vpx")]
#[link(name="stdc++")]
extern {
    fn WebmMkvReaderCreate() -> WebmMkvReaderRef;
    fn WebmMkvReaderDestroy(reader: WebmMkvReaderRef);
    fn WebmMkvReaderOpen(reader: WebmMkvReaderRef, path: *const c_char) -> c_int;
    fn WebmMkvReaderClose(reader: WebmMkvReaderRef);

    fn WebmEbmlHeaderCreate() -> WebmEbmlHeaderRef;
    fn WebmEbmlHeaderDestroy(header: WebmEbmlHeaderRef);
    fn WebmEbmlHeaderParse(header: WebmEbmlHeaderRef,
                           reader: WebmMkvReaderRef,
                           pos: *mut c_longlong)
                           -> c_longlong;

    fn WebmSegmentCreate(reader: WebmMkvReaderRef, pos: c_longlong, err: *mut c_longlong)
                         -> WebmSegmentRef;
    fn WebmSegmentDestroy(segment: WebmSegmentRef);
    fn WebmSegmentLoad(segment: WebmSegmentRef) -> c_long;
    fn WebmSegmentGetTracks(segment: WebmSegmentRef) -> WebmTracksRef;
    fn WebmSegmentGetCount(segment: WebmSegmentRef) -> c_ulong;
    fn WebmSegmentGetFirst(segment: WebmSegmentRef) -> WebmClusterRef;
    fn WebmSegmentGetNext(segment: WebmSegmentRef, cluster: WebmClusterRef) -> WebmClusterRef;

    fn WebmTracksDestroy(tracks: WebmTracksRef);
    fn WebmTracksGetCount(tracks: WebmTracksRef) -> c_ulong;
    fn WebmTracksGetTrackByIndex(tracks: WebmTracksRef, index: c_ulong) -> WebmTrackRef;
    fn WebmTracksGetTrackByNumber(tracks: WebmTracksRef, number: c_long) -> WebmTrackRef;

    fn WebmTrackDestroy(track: WebmTrackRef);
    fn WebmTrackGetType(track: WebmTrackRef) -> c_long;
    fn WebmTrackGetNumber(track: WebmTrackRef) -> c_long;

    fn WebmVideoTrackDestroy(track: WebmVideoTrackRef);
    fn WebmVideoTrackGetWidth(track: WebmVideoTrackRef) -> c_longlong;
    fn WebmVideoTrackGetHeight(track: WebmVideoTrackRef) -> c_longlong;
    fn WebmVideoTrackGetFrameRate(track: WebmVideoTrackRef) -> c_double;

    fn WebmClusterDestroy(cluster: WebmClusterRef);
    fn WebmClusterEos(cluster: WebmClusterRef) -> bool;
    fn WebmClusterGetFirst(cluster: WebmClusterRef, err: *mut c_long) -> WebmBlockEntryRef;
    fn WebmClusterGetNext(cluster: WebmClusterRef, entry: WebmBlockEntryRef, err: *mut c_long)
                          -> WebmBlockEntryRef;
    fn WebmClusterGetEntryCount(cluster: WebmClusterRef) -> c_long;
    fn WebmClusterGetEntry(cluster: WebmClusterRef, index: c_long, err: *mut c_long)
                           -> WebmBlockEntryRef;
    fn WebmClusterParse(cluster: WebmClusterRef, pos: *mut c_longlong, size: *mut c_long)
                        -> c_long;

    fn WebmBlockEntryDestroy(entry: WebmBlockEntryRef);
    fn WebmBlockEntryGetBlock(entry: WebmBlockEntryRef) -> WebmBlockRef;
    fn WebmBlockEntryEos(entry: WebmBlockEntryRef) -> bool;

    fn WebmBlockDestroy(block: WebmBlockRef);
    fn WebmBlockGetFrameCount(block: WebmBlockRef) -> c_int;
    fn WebmBlockGetFrame(block: WebmBlockRef, frameIndex: c_int) -> WebmBlockFrameRef;
    fn WebmBlockGetTrackNumber(block: WebmBlockRef) -> c_longlong;
    fn WebmBlockDiscardPadding(block: WebmBlockRef) -> c_longlong;
    fn WebmBlockIsKey(block: WebmBlockRef) -> bool;

    fn WebmBlockFrameDestroy(blockFrame: WebmBlockFrameRef);
    fn WebmBlockFrameGetPos(blockFrame: WebmBlockFrameRef) -> c_longlong;
    fn WebmBlockFrameGetLen(blockFrame: WebmBlockFrameRef) -> c_long;
    fn WebmBlockFrameRead(blockFrame: WebmBlockFrameRef,
                          reader: WebmMkvReaderRef,
                          buffer: *mut c_uchar)
                          -> c_long;
}

