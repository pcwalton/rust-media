# rust-media

## Introduction

`rust-media` is a media player framework for Rust, similar in spirit to `libvlc` or GStreamer. It's designed for use in Servo but is intended to be widely useful for all sorts of projects. Possible use cases are background music and FMVs for video games, as well as media player applications.

The `master` branch of `rust-media` is currently pinned to the same version of Rust that Servo uses. The `nightly` branch is intended to track the current Rust nightly; however, like many Rust projects, it may be out of date.

The library is currently in very early stages; contributions are welcome!

## Design goals

Uniquely, `rust-media` is designed to be *free*, *comprehensive*, and *portable* (in that order):

* *Free*—`rust-media` is designed to be freely distributable, even in regions where many codecs have intellectual property restrictions.

* *Comprehensive*—`rust-media` is designed to handle widely used codecs and container formats, even for codecs that are patent-encumbered. It does so by using the system codec implementations where available. If you wish to use FFmpeg, you may opt into its use with a Cargo feature.

* *Portable*—`rust-media` is designed to be embeddable and portable. It should work on desktop platforms, mobile platforms, and embedded systems, and it should not be tied to any one OS, graphics library, or audio library.

Other design goals include:

* Support streaming.

* Support low-level access to the codecs.

* Scale up to hundreds of videos playing simultaneously.

  - Leave thread management up to the user of the library; don't require that playback happen on a dedicated thread.

* Use hardware decoders where available.

* Be easy to use.

## Supported formats

* *Containers*—MP4/QuickTime, Matroska/MKV/WebM, animated GIF, Ogg (low-level support only).

* *Video codecs*—VP8 (via `libvpx`), H.264/AVC (via the OS X `VideoToolbox.framework` or FFmpeg), animated GIF.

* *Audio codecs*—Vorbis (via `libvorbis`), AAC (via the OS X `AudioUnit.framework` or FFmpeg).

## Building the example

    $ cd example
    $ cargo build

## Try out the example

* Play a WebM video:

        $ cargo run ~/Movies/big_buck_bunny_480p.webm video/webm

* Play a YouTube video:

        $ youtube-dl https://www.youtube.com/watch?v=dQw4w9WgXcQ --exec "target/release/example {} video/mp4"

# License

Licensed under the same terms as Rust itself.
