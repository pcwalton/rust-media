// Copyright 2015 The Servo Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

use std::ops::{Add, Sub};
use std::time::Duration;

/// A timestamp relative to the beginning of playback. `ticks / ticks_per_second` represents the
/// number of seconds. Use `.duration()` to convert to a Rust duration.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Timestamp {
    pub ticks: i64,
    pub ticks_per_second: f64,
}

impl Timestamp {
    pub fn duration(&self) -> Duration {
        let secs = (self.ticks as f64 / self.ticks_per_second) as u64;
        let nanos = ((self.ticks * 1_000_000_000) as f64 / self.ticks_per_second) as u64;
        let nanos_less_secs = nanos - secs * 1_000_000_000;
        Duration::new(secs, nanos_less_secs as u32)
    }
}

impl Add<i64> for Timestamp {
    type Output = Timestamp;

    fn add(self, other: i64) -> Timestamp {
        Timestamp {
            ticks: self.ticks + other,
            ticks_per_second: self.ticks_per_second,
        }
    }
}

impl Sub<i64> for Timestamp {
    type Output = Timestamp;

    fn sub(self, other: i64) -> Timestamp {
        Timestamp {
            ticks: self.ticks - other,
            ticks_per_second: self.ticks_per_second,
        }
    }
}

