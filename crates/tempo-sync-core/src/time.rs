// SPDX-License-Identifier: Apache-2.0

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct HostTime(pub i64);

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Duration(pub i64);

#[derive(Debug, Clone, Copy, PartialEq, PartialOrd)]
pub struct Tempo(pub f64);

impl Tempo {
    pub fn micros_per_beat(&self) -> f64 {
        if self.0 <= 0.0 {
            return f64::INFINITY;
        }
        60_000_000.0 / self.0
    }
}

pub type Beats = f64;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct MicroBeats(pub i64);

impl MicroBeats {
    pub fn from_beats(beats: Beats) -> Self {
        Self((beats * 1_000_000.0).round() as i64)
    }
    pub fn to_beats(&self) -> Beats {
        self.0 as f64 / 1_000_000.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TempoWire(pub i64);

impl TempoWire {
    pub fn from_tempo(tempo: Tempo) -> Self {
        Self((60_000_000.0 / tempo.0).round() as i64)
    }
    pub fn to_tempo(&self) -> Tempo {
        if self.0 <= 0 {
            Tempo(f64::INFINITY)
        } else {
            Tempo(60_000_000.0 / self.0 as f64)
        }
    }
}
