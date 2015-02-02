// Copyright 2015 The Servo Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

#include <mkvparser.hpp>
#include <mkvreader.hpp>

using namespace mkvparser;

typedef MkvReader* WebmMkvReaderRef;
typedef IMkvReader* WebmIMkvReaderRef;
typedef EBMLHeader* WebmEbmlHeaderRef;
typedef Segment* WebmSegmentRef;
typedef SegmentInfo* WebmSegmentInfoRef;
typedef Tracks* WebmTracksRef;
typedef Cluster* WebmClusterRef;
typedef Track* WebmTrackRef;
typedef VideoTrack* WebmVideoTrackRef;
typedef AudioTrack* WebmAudioTrackRef;
typedef BlockEntry* WebmBlockEntryRef;
typedef Block* WebmBlockRef;
typedef Block::Frame* WebmBlockFrameRef;

struct WebmCustomMkvReaderCallbacks {
    int (*Read)(long long pos, long len, unsigned char* buf, void* userData);
    int (*Length)(long long* total, long long* available, void* userData);
    void (*Destroy)(void* userData);
};

class CustomMkvReader : public IMkvReader {
public:
    CustomMkvReader(const WebmCustomMkvReaderCallbacks* inCallbacks, void* inUserData)
        : userData(inUserData), callbacks(*inCallbacks) {}

    virtual ~CustomMkvReader() {
        callbacks.Destroy(userData);
    }

    virtual int Read(long long pos, long len, unsigned char* buf) {
        return callbacks.Read(pos, len, buf, userData);
    }

    virtual int Length(long long* total, long long* available) {
        return callbacks.Length(total, available, userData);
    }

protected:
    void* userData;
    WebmCustomMkvReaderCallbacks callbacks;
};

extern "C" WebmMkvReaderRef WebmMkvReaderCreate() {
    return new MkvReader();
}

extern "C" int WebmMkvReaderOpen(WebmMkvReaderRef reader, const char* path) {
    return reader->Open(path);
}

extern "C" void WebmMkvReaderClose(WebmMkvReaderRef reader) {
    reader->Close();
}

extern "C" WebmIMkvReaderRef WebmCustomMkvReaderCreate(const WebmCustomMkvReaderCallbacks*
                                                        callbacks,
                                                       void* userData) {
    return new CustomMkvReader(callbacks, userData);
}

extern "C" void WebmCustomMkvReaderDestroy(WebmIMkvReaderRef reader) {
    delete static_cast<CustomMkvReader*>(reader);
}

extern "C" WebmEbmlHeaderRef WebmEbmlHeaderCreate() {
    return new EBMLHeader();
}

extern "C" void WebmEbmlHeaderDestroy(WebmEbmlHeaderRef header) {
    delete header;
}

extern "C" long long WebmEbmlHeaderParse(WebmEbmlHeaderRef header,
                                         WebmIMkvReaderRef reader,
                                         long long* pos) {
    return header->Parse(reader, *pos);
}

extern "C" WebmSegmentRef WebmSegmentCreate(WebmIMkvReaderRef reader,
                                            long long pos,
                                            long long* err) {
    Segment* result = nullptr;
    *err = Segment::CreateInstance(reader, pos, result);
    return result;
}

extern "C" void WebmSegmentDestroy(WebmSegmentRef segment) {
    delete segment;
}

extern "C" long WebmSegmentLoad(WebmSegmentRef segment) {
    return segment->Load();
}

extern "C" WebmTracksRef WebmSegmentGetTracks(WebmSegmentRef segment) {
    return const_cast<WebmTracksRef>(segment->GetTracks());
}

extern "C" WebmSegmentInfoRef WebmSegmentGetInfo(WebmSegmentRef segment) {
    return const_cast<WebmSegmentInfoRef>(segment->GetInfo());
}

extern "C" unsigned long WebmSegmentGetCount(WebmSegmentRef segment) {
    return segment->GetCount();
}

extern "C" WebmClusterRef WebmSegmentGetFirst(WebmSegmentRef segment) {
    return const_cast<WebmClusterRef>(segment->GetFirst());
}

extern "C" WebmClusterRef WebmSegmentGetNext(WebmSegmentRef segment, WebmClusterRef cluster) {
    return const_cast<WebmClusterRef>(segment->GetNext(const_cast<const Cluster*>(cluster)));
}

extern "C" long long WebmSegmentInfoGetTimeCodeScale(WebmSegmentInfoRef segmentInfo) {
    return segmentInfo->GetTimeCodeScale();
}

extern "C" void WebmTracksDestroy(WebmTracksRef tracks) {
    delete tracks;
}

extern "C" unsigned long WebmTracksGetCount(WebmTracksRef tracks) {
    return tracks->GetTracksCount();
}

extern "C" WebmTrackRef WebmTracksGetTrackByIndex(WebmTracksRef tracks, unsigned long index) {
    return const_cast<WebmTrackRef>(tracks->GetTrackByIndex(index));
}

extern "C" WebmTrackRef WebmTracksGetTrackByNumber(WebmTracksRef tracks, long number) {
    return const_cast<WebmTrackRef>(tracks->GetTrackByNumber(number));
}

extern "C" void WebmTrackDestroy(WebmTrackRef track) {
    delete track;
}

extern "C" long WebmTrackGetType(WebmTrackRef track) {
    return track->GetType();
}

extern "C" long WebmTrackGetNumber(WebmTrackRef track) {
    return track->GetNumber();
}

extern "C" const char* WebmTrackGetCodecId(WebmTrackRef track) {
    return track->GetCodecId();
}

extern "C" const unsigned char* WebmTrackGetCodecPrivate(WebmTrackRef track, size_t* size) {
    return track->GetCodecPrivate(*size);
}

extern "C" void WebmVideoTrackDestroy(WebmVideoTrackRef track) {
    delete track;
}

extern "C" long long WebmVideoTrackGetWidth(WebmVideoTrackRef track) {
    return track->GetWidth();
}

extern "C" long long WebmVideoTrackGetHeight(WebmVideoTrackRef track) {
    return track->GetHeight();
}

extern "C" double WebmVideoTrackGetFrameRate(WebmVideoTrackRef track) {
    return track->GetFrameRate();
}

extern "C" void WebmAudioTrackDestroy(WebmAudioTrackRef track) {
    delete track;
}

extern "C" double WebmAudioTrackGetSamplingRate(WebmAudioTrackRef track) {
    return track->GetSamplingRate();
}

extern "C" long long WebmAudioTrackGetChannels(WebmAudioTrackRef track) {
    return track->GetChannels();
}

extern "C" long long WebmAudioTrackGetBitDepth(WebmAudioTrackRef track) {
    return track->GetBitDepth();
}

extern "C" void WebmClusterDestroy(WebmClusterRef cluster) {
    delete cluster;
}

extern "C" bool WebmClusterEos(WebmClusterRef cluster) {
    return cluster->EOS();
}

extern "C" long long WebmClusterGetTime(WebmClusterRef cluster) {
    return cluster->GetTime();
}

extern "C" WebmBlockEntryRef WebmClusterGetFirst(WebmClusterRef cluster, long* err) {
    const BlockEntry* result = nullptr;
    *err = cluster->GetFirst(result);
    return const_cast<WebmBlockEntryRef>(result);
}

extern "C" WebmBlockEntryRef WebmClusterGetNext(WebmClusterRef cluster,
                                                WebmBlockEntryRef entry,
                                                long* err) {
    const BlockEntry* result = nullptr;
    *err = cluster->GetNext(entry, result);
    return const_cast<WebmBlockEntryRef>(result);
}

extern "C" long WebmClusterGetEntryCount(WebmClusterRef cluster) {
    return cluster->GetEntryCount();
}

extern "C" long WebmClusterParse(WebmClusterRef cluster, long long* pos, long* size) {
    return cluster->Parse(*pos, *size);
}

extern "C" WebmBlockEntryRef WebmClusterGetEntry(WebmClusterRef cluster, long index, long* err) {
    const BlockEntry* result = nullptr;
    *err = cluster->GetEntry(index, result);
    return const_cast<WebmBlockEntryRef>(result);
}

extern "C" void WebmBlockEntryDestroy(WebmBlockEntryRef entry) {
    delete entry;
}

extern "C" WebmBlockRef WebmBlockEntryGetBlock(WebmBlockEntryRef entry) {
    return const_cast<WebmBlockRef>(entry->GetBlock());
}

extern "C" bool WebmBlockEntryEos(WebmBlockEntryRef entry) {
    return entry->EOS();
}

extern "C" void WebmBlockDestroy(WebmBlockRef block) {
    delete block;
}

extern "C" int WebmBlockGetFrameCount(WebmBlockRef block) {
    return block->GetFrameCount();
}

extern "C" WebmBlockFrameRef WebmBlockGetFrame(WebmBlockRef block, int frameIndex) {
    return const_cast<WebmBlockFrameRef>(&block->GetFrame(frameIndex));
}

extern "C" long long WebmBlockGetTrackNumber(WebmBlockRef block) {
    return block->GetTrackNumber();
}

extern "C" long long WebmBlockDiscardPadding(WebmBlockRef block) {
    return block->GetDiscardPadding();
}

extern "C" long long WebmBlockGetTimeCode(WebmBlockRef block, WebmClusterRef cluster) {
    return block->GetTimeCode(cluster);
}

extern "C" long long WebmBlockGetTime(WebmBlockRef block, WebmClusterRef cluster) {
    return block->GetTime(cluster);
}

extern "C" bool WebmBlockIsKey(WebmBlockRef block) {
    return block->IsKey();
}

extern "C" void WebmBlockFrameDestroy(WebmBlockFrameRef blockFrame) {
    delete blockFrame;
}

extern "C" long long WebmBlockFrameGetPos(WebmBlockFrameRef blockFrame) {
    return blockFrame->pos;
}

extern "C" long WebmBlockFrameGetLen(WebmBlockFrameRef blockFrame) {
    return blockFrame->len;
}

extern "C" long WebmBlockFrameRead(WebmBlockFrameRef blockFrame,
                                   WebmIMkvReaderRef reader,
                                   unsigned char* buffer) {
    return blockFrame->Read(reader, buffer);
}

