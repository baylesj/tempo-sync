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

pub type Beats = f64; // f64 or fixed-point, based on PROTOCOL.md
