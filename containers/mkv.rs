// Copyright 2015 The Servo Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

use audiodecoder;
use codecs::vorbis::VorbisHeaders;
use container;
use pixelformat::PixelFormat;
use streaming::StreamReader;
use timing::Timestamp;
use videodecoder;
use utils;

use libc::{c_char, c_double, c_int, c_long, c_longlong, c_uchar, c_ulong, c_void, size_t};
use std::ffi::CStr;
use std::mem;
use std::io::SeekFrom;
use std::ptr;
use std::slice;
use std::marker::PhantomData;

const VIDEO_TRACK_TYPE: i32 = 1;
const AUDIO_TRACK_TYPE: i32 = 2;
#[allow(unused)] const SUBTITLE_TRACK_TYPE: i32 = 0x11;
#[allow(unused)] const METADATA_TRACK_TYPE: i32 = 0x21;

pub struct MkvReader {
    reader: WebmIMkvReaderRef,
}

impl Drop for MkvReader {
    fn drop(&mut self) {
        unsafe {
            WebmCustomMkvReaderDestroy(self.reader)
        }
    }
}

impl MkvReader {
    pub fn new(reader: Box<StreamReader>) -> MkvReader {
        MkvReader {
            reader: unsafe {
                WebmCustomMkvReaderCreate(&READER_CALLBACKS,
                                          mem::transmute::<Box<Box<_>>,
                                                           *mut c_void>(Box::new(reader)))
            },
        }
    }
}

extern "C" fn read_callback(pos: c_longlong,
                            len: c_long,
                            buf: *mut c_uchar,
                            mut user_data: *mut c_void)
                            -> c_int {
    if len < 0 || pos < 0 {
        return -1
    }

    unsafe {
        let reader: &mut Box<Box<StreamReader>> = mem::transmute(&mut user_data);
        if reader.seek(SeekFrom::Start(pos as u64)).is_err() {
            return -1
        }
        match utils::read_to_full(reader, slice::from_raw_parts_mut(buf, len as usize)) {
            Ok(_) => 0,
            Err(_) => -1,
        }
    }
}

extern "C" fn length_callback(total: *mut c_longlong,
                              available: *mut c_longlong,
                              mut user_data: *mut c_void)
                              -> c_int {
    unsafe {
        let reader: &mut Box<Box<StreamReader>> = mem::transmute(&mut user_data);
        *total = reader.total_size() as c_longlong;
        *available = reader.available_size() as c_longlong;
    }
    0
}

extern "C" fn destroy_callback(user_data: *mut c_void) {
    unsafe {
        drop(mem::transmute::<_,Box<Box<StreamReader>>>(user_data))
    }
}

static READER_CALLBACKS: WebmCustomMkvReaderCallbacks = WebmCustomMkvReaderCallbacks {
    Read: read_callback,
    Length: length_callback,
    Destroy: destroy_callback,
};

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
            phantom: PhantomData,
        })
    }

    pub fn count(&self) -> c_ulong {
        unsafe {
            WebmSegmentGetCount(self.segment)
        }
    }

    pub fn info<'a>(&'a self) -> SegmentInfo<'a> {
        SegmentInfo {
            segment_info: unsafe {
                WebmSegmentGetInfo(self.segment)
            },
            phantom: PhantomData,
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
                phantom: PhantomData,
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
                phantom: PhantomData,
            })
        }
    }
}

pub struct SegmentInfo<'a> {
    segment_info: WebmSegmentInfoRef,
    phantom: PhantomData<&'a u8>,
}

impl<'a> SegmentInfo<'a> {
    fn time_code_scale(&self) -> c_longlong {
        unsafe {
            WebmSegmentInfoGetTimeCodeScale(self.segment_info)
        }
    }
}

pub struct Tracks<'a> {
    tracks: WebmTracksRef,
    phantom: PhantomData<&'a u8>,
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
            },
            phantom: PhantomData,
        }
    }

    pub fn track_by_number(&self, number: c_long) -> Track<'a> {
        Track {
            track: unsafe {
                WebmTracksGetTrackByNumber(self.tracks, number)
            },
            phantom: PhantomData,
        }
    }
}

#[derive(Clone)]
pub struct Track<'a> {
    track: WebmTrackRef,
    phantom: PhantomData<&'a u8>,
}

impl<'a> Track<'a> {
    pub fn track_type(self) -> TrackType<'a> {
        let track_type = unsafe { WebmTrackGetType(self.track) as i32 };
        match track_type {
            VIDEO_TRACK_TYPE => {
                TrackType::Video(VideoTrack {
                    track: unsafe {
                        mem::transmute::<_,WebmVideoTrackRef>(self.track)
                    },
                    phantom: PhantomData,
                })
            }
            AUDIO_TRACK_TYPE => {
                TrackType::Audio(AudioTrack {
                    track: unsafe {
                        mem::transmute::<_,WebmAudioTrackRef>(self.track)
                    },
                    phantom: PhantomData,
                })
            }
            _ => TrackType::Other(self),
        }
    }

    pub fn is_video(&self) -> bool {
        let track_type = unsafe { WebmTrackGetType(self.track) as i32 };
        track_type == VIDEO_TRACK_TYPE
    }

    pub fn is_audio(&self) -> bool {
        let track_type = unsafe { WebmTrackGetType(self.track) as i32 };
        track_type == AUDIO_TRACK_TYPE
    }

    pub fn number(&self) -> c_long {
        unsafe {
            WebmTrackGetNumber(self.track)
        }
    }

    pub fn codec_id<'b>(&'b self) -> &'b [u8] {
        unsafe {
            let ptr = WebmTrackGetCodecId(self.track);
            let bytes = CStr::from_ptr(ptr).to_bytes();
            mem::transmute::<&[u8],&'b [u8]>(bytes)
        }
    }

    pub fn codec_private<'b>(&'b self) -> &'b [u8] {
        let mut size = 0;
        unsafe {
            let ptr = WebmTrackGetCodecPrivate(self.track, &mut size);
            mem::transmute::<&[u8],&'b [u8]>(slice::from_raw_parts(ptr, size as usize))
        }
    }
}

#[derive(Clone)]
pub struct VideoTrack<'a> {
    track: WebmVideoTrackRef,
    phantom: PhantomData<&'a u8>,
}

impl<'a> VideoTrack<'a> {
    pub fn as_track(&self) -> Track<'a> {
        Track {
            track: unsafe {
                mem::transmute::<_,WebmTrackRef>(self.track)
            },
            phantom: PhantomData,
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
pub struct AudioTrack<'a> {
    track: WebmAudioTrackRef,
    phantom: PhantomData<&'a u8>,
}

impl<'a> AudioTrack<'a> {
    pub fn as_track(&self) -> Track<'a> {
        Track {
            track: unsafe {
                mem::transmute::<_,WebmTrackRef>(self.track)
            },
            phantom: PhantomData,
        }
    }

    pub fn sampling_rate(&self) -> c_double {
        unsafe {
            WebmAudioTrackGetSamplingRate(self.track)
        }
    }

    pub fn channels(&self) -> c_longlong {
        unsafe {
            WebmAudioTrackGetChannels(self.track)
        }
    }

    pub fn bit_depth(&self) -> c_longlong {
        unsafe {
            WebmAudioTrackGetBitDepth(self.track)
        }
    }
}

#[derive(Clone)]
pub struct Cluster<'a> {
    cluster: WebmClusterRef,
    phantom: PhantomData<&'a u8>,
}

impl<'a> Cluster<'a> {
    pub fn eos(&self) -> bool {
        unsafe {
            WebmClusterEos(self.cluster)
        }
    }

    /// Returns the absolute, scaled time of this cluster in nanoseconds.
    pub fn time(&self) -> c_longlong {
        unsafe {
            WebmClusterGetTime(self.cluster)
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
                phantom: PhantomData,
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
                phantom: PhantomData,
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
        // Needed for memory safety.
        if index < 0 || index >= self.entry_count() {
            return Err(-1)
        }

        let mut err = 0;
        let entry = unsafe {
            WebmClusterGetEntry(self.cluster, index, &mut err)
        };
        if err >= 0 {
            Ok(BlockEntry {
                entry: entry,
                phantom: PhantomData,
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
    phantom: PhantomData<&'a u8>,
}

impl<'a> BlockEntry<'a> {
    pub fn block(&self) -> Block<'a> {
        unsafe {
            Block {
                block: WebmBlockEntryGetBlock(self.entry),
                phantom: PhantomData,
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
    phantom: PhantomData<&'a u8>,
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
                frame: WebmBlockGetFrame(self.block, frame_index),
                phantom: PhantomData,
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

    pub fn time(&self, cluster: &Cluster) -> c_longlong {
        unsafe {
            WebmBlockGetTime(self.block, cluster.cluster)
        }
    }

    pub fn time_code(&self, cluster: &Cluster) -> c_longlong {
        unsafe {
            WebmBlockGetTimeCode(self.block, cluster.cluster)
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
    phantom: PhantomData<&'a u8>,
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

pub enum TrackType<'a> {
    Video(VideoTrack<'a>),
    Audio(AudioTrack<'a>),
    Other(Track<'a>),
}

// Implementation of the abstract `ContainerReader` interface

struct ContainerReaderImpl {
    reader: MkvReader,
    segment: Segment,
}

impl ContainerReaderImpl {
    fn new(reader: Box<StreamReader>) -> Result<Box<container::ContainerReader + 'static>,()> {
        let reader = MkvReader::new(reader);
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
            track: self.segment.tracks().unwrap().track_by_index(index as c_ulong),
            segment: &self.segment,
            reader: &self.reader,
        }) as Box<container::Track + 'a>
    }

    fn track_by_number<'a>(&'a self, number: c_long) -> Box<container::Track + 'a> {
        Box::new(TrackImpl {
            track: self.segment.tracks().unwrap().track_by_number(number),
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

impl<'a> container::Track<'a> for TrackImpl<'a> {
    fn track_type(self: Box<Self>) -> container::TrackType<'a> {
        let segment = self.segment;
        let reader = self.reader;
        let track = self.track;
        match track.track_type() {
            TrackType::Video(track) => {
                container::TrackType::Video(Box::new(VideoTrackImpl {
                    track: track,
                    segment: segment,
                    reader: reader,
                }) as Box<container::VideoTrack<'a> + 'a>)
            }
            TrackType::Audio(track) => {
                container::TrackType::Audio(Box::new(AudioTrackImpl {
                    track: track,
                    segment: segment,
                    reader: reader,
                }) as Box<container::AudioTrack<'a> + 'a>)
            }
            TrackType::Other(track) => {
                container::TrackType::Other(Box::new(TrackImpl {
                    track: track,
                    segment: segment,
                    reader: reader,
                }) as Box<container::Track<'a> + 'a>)
            }
        }
    }
    fn is_video(&self) -> bool { self.track.is_video() }
    fn is_audio(&self) -> bool { self.track.is_audio() }


    fn cluster_count(&self) -> Option<c_int> {
        Some(self.segment.count() as c_int)
    }

    fn number(&self) -> c_long {
        self.track.number()
    }

    fn codec(&self) -> Option<Vec<u8>> {
        codec_id_to_fourcc(self.track.codec_id())
    }

    fn cluster<'b>(&'b self, cluster_index: i32) -> Result<Box<container::Cluster + 'b>,()> {
        Ok(get_cluster(cluster_index, self.segment, self.reader))
    }
}

#[derive(Clone)]
struct VideoTrackImpl<'a> {
    track: VideoTrack<'a>,
    segment: &'a Segment,
    reader: &'a MkvReader,
}

impl<'a> container::Track<'a> for VideoTrackImpl<'a> {
    fn track_type(self: Box<Self>) -> container::TrackType<'a> {
        container::TrackType::Video(self as Box<container::VideoTrack<'a> + 'a>)
    }

    fn is_video(&self) -> bool { true }
    fn is_audio(&self) -> bool { false }

    fn cluster_count(&self) -> Option<c_int> {
        Some(self.segment.count() as c_int)
    }

    fn number(&self) -> c_long {
        self.track.as_track().number()
    }


    fn cluster<'b>(&'b self, cluster_index: i32) -> Result<Box<container::Cluster + 'b>,()> {
        Ok(get_cluster(cluster_index, self.segment, self.reader))
    }

    fn codec(&self) -> Option<Vec<u8>> {
        codec_id_to_fourcc(self.track.as_track().codec_id())
    }
}

impl<'a> container::VideoTrack<'a> for VideoTrackImpl<'a> {
    fn width(&self) -> u16 {
        self.track.width() as u16
    }

    fn height(&self) -> u16 {
        self.track.height() as u16
    }

    fn frame_rate(&self) -> c_double {
        self.track.frame_rate()
    }

    fn pixel_format(&self) -> PixelFormat<'static> {
        PixelFormat::I420
    }

	fn headers(&self) -> Box<videodecoder::VideoHeaders> {
		// TODO(pcwalton): Support H.264.
		Box::new(videodecoder::EmptyVideoHeadersImpl) as Box<videodecoder::VideoHeaders>
	}
}

#[derive(Clone)]
struct AudioTrackImpl<'a> {
    track: AudioTrack<'a>,
    segment: &'a Segment,
    reader: &'a MkvReader,
}

impl<'a> container::Track<'a> for AudioTrackImpl<'a> {
    fn track_type(self: Box<Self>) -> container::TrackType<'a> {
        container::TrackType::Audio(self as Box<container::AudioTrack<'a> + 'a>)
    }

    fn is_video(&self) -> bool { false }
    fn is_audio(&self) -> bool { true }

    fn cluster_count(&self) -> Option<c_int> {
        Some(self.segment.count() as c_int)
    }

    fn number(&self) -> c_long {
        self.track.as_track().number()
    }

    fn cluster<'b>(&'b self, cluster_index: i32) -> Result<Box<container::Cluster + 'b>,()> {
        Ok(get_cluster(cluster_index, self.segment, self.reader))
    }

    fn codec(&self) -> Option<Vec<u8>> {
        codec_id_to_fourcc(self.track.as_track().codec_id())
    }
}

impl<'a> container::AudioTrack<'a> for AudioTrackImpl<'a> {
    fn sampling_rate(&self) -> c_double {
        self.track.sampling_rate()
    }

    fn channels(&self) -> u16 {
        self.track.channels() as u16
    }

    fn headers(&self) -> Box<audiodecoder::AudioHeaders> {
        // TODO(pcwalton): Support codecs other than Vorbis.
        let track = self.track.as_track();
        let mut private = track.codec_private();
        assert!(private[0] == 2);
        private = &private[1..private.len()];

        let id_size = read_lacing_size(&mut private);
        let comment_size = read_lacing_size(&mut private);
        return Box::new(VorbisHeaders {
            data: private.iter().map(|x| *x).collect(),
            id_size: id_size,
            comment_size: comment_size,
        });

        fn read_lacing_size(buffer: &mut &[u8]) -> usize {
            let mut size = 0;
            while buffer[0] == 255 {
                size += 255;
                *buffer = &(*buffer)[1..buffer.len()];
            }
            size += buffer[0] as usize;
            *buffer = &(*buffer)[1..buffer.len()];
            size
        }
    }
}

struct ClusterImpl<'a> {
    cluster: Cluster<'a>,
    segment: &'a Segment,
    reader: &'a MkvReader,
}

impl<'a> container::Cluster for ClusterImpl<'a> {
    fn read_frame<'b>(&'b self, frame_index: i32, track_number: c_long)
                      -> Result<Box<container::Frame + 'b>,()> {
        // FIXME(pcwalton): This is O(frames in this cluster); is this going to be a problem?
        let (mut block_index, mut current_frame_index) = (0, 0);
        loop {
            let block = match self.cluster.entry(block_index as c_long) {
                Ok(block_entry) => block_entry.block(),
                Err(_) => return Err(()),
            };
            if block.track_number() == track_number as i64 {
                if current_frame_index == frame_index {
                    break
                } else {
                    current_frame_index += 1
                }
            }
            block_index += 1
        }
        Ok(Box::new(FrameImpl {
            block: self.cluster.entry(block_index as c_long).unwrap().block(),
            cluster: &self.cluster,
            segment: self.segment,
            reader: self.reader,
        }) as Box<container::Frame + 'b>)
    }
}

struct FrameImpl<'a> {
    block: Block<'a>,
    cluster: &'a Cluster<'a>,
    segment: &'a Segment,
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
        self.block.track_number() as c_long
    }

    fn time(&self) -> Timestamp {
        Timestamp {
            ticks: self.block.time_code(self.cluster),
            ticks_per_second: 1_000_000_000.0 / self.segment.info().time_code_scale() as f64,
        }
    }

    fn rendering_offset(&self) -> i64 {
        0
    }
}

fn codec_id_to_fourcc(id: &[u8]) -> Option<Vec<u8>> {
    const TABLE: [(&'static [u8], [u8; 4]); 2] = [
        (b"V_VP8", [b'V', b'P', b'8', b'0']),
        (b"A_VORBIS", [b'v', b'o', b'r', b'b'])
    ];
    for &(key, value) in TABLE.iter() {
        if key == id {
            return Some(value.iter().map(|x| *x).collect())
        }
    }
    None
}

fn get_cluster<'a>(cluster_index: i32, segment: &'a Segment, reader: &'a MkvReader)
                   -> Box<container::Cluster + 'a> {
    let mut cluster = segment.first().unwrap();
    for _ in 0 .. cluster_index {
        cluster = segment.next(cluster).unwrap();
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
        segment: segment,
        reader: reader,
    }) as Box<container::Cluster + 'a>
}

pub const CONTAINER_READER: container::RegisteredContainerReader =
    container::RegisteredContainerReader {
        mime_types: &[
            "video/webm",
            "video/x-matroska",
        ],
        read: ContainerReaderImpl::new,
    };

// FFI stuff

type WebmIMkvReaderRef = *mut WebmIMkvReader;
type WebmEbmlHeaderRef = *mut WebmEbmlReader;
type WebmSegmentRef = *mut WebmSegment;
type WebmSegmentInfoRef = *mut WebmSegmentInfo;
type WebmTracksRef = *mut WebmTracks;
type WebmTrackRef = *mut WebmTrack;
type WebmVideoTrackRef = *mut WebmVideoTrack;
type WebmAudioTrackRef = *mut WebmAudioTrack;
type WebmClusterRef = *mut WebmCluster;
type WebmBlockEntryRef = *mut WebmBlockEntry;
type WebmBlockRef = *mut WebmBlock;
type WebmBlockFrameRef = *mut WebmBlockFrame;

#[repr(C)]
struct WebmIMkvReader;
#[repr(C)]
struct WebmEbmlReader;
#[repr(C)]
struct WebmSegment;
#[repr(C)]
struct WebmSegmentInfo;
#[repr(C)]
struct WebmTracks;
#[repr(C)]
struct WebmTrack;
#[repr(C)]
struct WebmVideoTrack;
#[repr(C)]
struct WebmAudioTrack;
#[repr(C)]
struct WebmCluster;
#[repr(C)]
struct WebmBlockEntry;
#[repr(C)]
struct WebmBlock;
#[repr(C)]
struct WebmBlockFrame;
#[repr(C)]
#[allow(non_snake_case)]
struct WebmCustomMkvReaderCallbacks {
    Read: extern "C" fn(pos: c_longlong, len: c_long, buf: *mut c_uchar, userData: *mut c_void)
                        -> c_int,
    Length: extern "C" fn(total: *mut c_longlong,
                          available: *mut c_longlong,
                          userData: *mut c_void)
                          -> c_int,
    Destroy: extern "C" fn(userData: *mut c_void),
}

#[allow(dead_code)]
#[link(name="rustmedia")]
#[link(name="webm")]
#[link(name="vpx")]
#[link(name="stdc++")]
extern {
    fn WebmCustomMkvReaderCreate(callbacks: *const WebmCustomMkvReaderCallbacks,
                                 userData: *mut c_void)
                                 -> WebmIMkvReaderRef;
    fn WebmCustomMkvReaderDestroy(reader: WebmIMkvReaderRef);

    fn WebmEbmlHeaderCreate() -> WebmEbmlHeaderRef;
    fn WebmEbmlHeaderDestroy(header: WebmEbmlHeaderRef);
    fn WebmEbmlHeaderParse(header: WebmEbmlHeaderRef,
                           reader: WebmIMkvReaderRef,
                           pos: *mut c_longlong)
                           -> c_longlong;

    fn WebmSegmentCreate(reader: WebmIMkvReaderRef, pos: c_longlong, err: *mut c_longlong)
                         -> WebmSegmentRef;
    fn WebmSegmentDestroy(segment: WebmSegmentRef);
    fn WebmSegmentLoad(segment: WebmSegmentRef) -> c_long;
    fn WebmSegmentGetTracks(segment: WebmSegmentRef) -> WebmTracksRef;
    fn WebmSegmentGetInfo(segment: WebmSegmentRef) -> WebmSegmentInfoRef;
    fn WebmSegmentGetCount(segment: WebmSegmentRef) -> c_ulong;
    fn WebmSegmentGetFirst(segment: WebmSegmentRef) -> WebmClusterRef;
    fn WebmSegmentGetNext(segment: WebmSegmentRef, cluster: WebmClusterRef) -> WebmClusterRef;

    fn WebmSegmentInfoGetTimeCodeScale(segmentInfo: WebmSegmentInfoRef) -> c_longlong;

    fn WebmTracksDestroy(tracks: WebmTracksRef);
    fn WebmTracksGetCount(tracks: WebmTracksRef) -> c_ulong;
    fn WebmTracksGetTrackByIndex(tracks: WebmTracksRef, index: c_ulong) -> WebmTrackRef;
    fn WebmTracksGetTrackByNumber(tracks: WebmTracksRef, number: c_long) -> WebmTrackRef;

    fn WebmTrackDestroy(track: WebmTrackRef);
    fn WebmTrackGetType(track: WebmTrackRef) -> c_long;
    fn WebmTrackGetNumber(track: WebmTrackRef) -> c_long;
    fn WebmTrackGetCodecId(track: WebmTrackRef) -> *const c_char;
    fn WebmTrackGetCodecPrivate(track: WebmTrackRef, size: *mut size_t) -> *const c_uchar;

    fn WebmVideoTrackDestroy(track: WebmVideoTrackRef);
    fn WebmVideoTrackGetWidth(track: WebmVideoTrackRef) -> c_longlong;
    fn WebmVideoTrackGetHeight(track: WebmVideoTrackRef) -> c_longlong;
    fn WebmVideoTrackGetFrameRate(track: WebmVideoTrackRef) -> c_double;

    fn WebmAudioTrackDestroy(track: WebmAudioTrackRef);
    fn WebmAudioTrackGetSamplingRate(track: WebmAudioTrackRef) -> c_double;
    fn WebmAudioTrackGetChannels(track: WebmAudioTrackRef) -> c_longlong;
    fn WebmAudioTrackGetBitDepth(track: WebmAudioTrackRef) -> c_longlong;

    fn WebmClusterDestroy(cluster: WebmClusterRef);
    fn WebmClusterEos(cluster: WebmClusterRef) -> bool;
    fn WebmClusterGetTime(cluster: WebmClusterRef) -> c_longlong;
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
    fn WebmBlockGetTimeCode(block: WebmBlockRef, cluster: WebmClusterRef) -> c_longlong;
    fn WebmBlockGetTime(block: WebmBlockRef, cluster: WebmClusterRef) -> c_longlong;
    fn WebmBlockIsKey(block: WebmBlockRef) -> bool;

    fn WebmBlockFrameDestroy(blockFrame: WebmBlockFrameRef);
    fn WebmBlockFrameGetPos(blockFrame: WebmBlockFrameRef) -> c_longlong;
    fn WebmBlockFrameGetLen(blockFrame: WebmBlockFrameRef) -> c_long;
    fn WebmBlockFrameRead(blockFrame: WebmBlockFrameRef,
                          reader: WebmIMkvReaderRef,
                          buffer: *mut c_uchar)
                          -> c_long;
}

