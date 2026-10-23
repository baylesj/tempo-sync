// SPDX-License-Identifier: Apache-2.0

use crate::time::{HostTime, Tempo, Beats};

#[derive(Debug, Clone, PartialEq)]
pub struct Timeline {
    pub tempo: Tempo,
    pub beat_origin: Beats,
    pub time_origin: HostTime,
}

#[derive(Debug, Clone, PartialEq)]
pub struct StartStopState {
    pub is_playing: bool,
    pub beat: Beats,
    pub time: HostTime,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SessionState {
    pub timeline: Timeline,
    pub start_stop: StartStopState,
}

impl SessionState {
    pub fn beat_at_time(&self, time: HostTime, _quantum: f64) -> Beats {
        // Simple stub
        let dt = (time.0 - self.timeline.time_origin.0) as f64;
        let dbeats = dt / self.timeline.tempo.micros_per_beat();
        self.timeline.beat_origin + dbeats
    }

    pub fn phase_at_time(&self, time: HostTime, quantum: f64) -> f64 {
        let beat = self.beat_at_time(time, quantum);
        // modulo quantum
        beat % quantum
    }

    pub fn time_at_beat(&self, beat: Beats, _quantum: f64) -> HostTime {
        let dbeats = beat - self.timeline.beat_origin;
        let dt = dbeats * self.timeline.tempo.micros_per_beat();
        HostTime(self.timeline.time_origin.0 + dt as i64)
    }

    pub fn request_beat_at_time(&mut self, _time: HostTime, _beat: Beats) {
        // Update origins
    }

    pub fn force_beat_at_time(&mut self, _time: HostTime, _beat: Beats) {
        // Update origins forcefully
    }

    pub fn set_tempo(&mut self, bpm: f64, at: HostTime) {
        let current_beat = self.beat_at_time(at, 0.0);
        self.timeline.tempo = Tempo(bpm);
        self.timeline.time_origin = at;
        self.timeline.beat_origin = current_beat;
    }

    pub fn set_is_playing(&mut self, playing: bool, at: HostTime) {
        self.start_stop.is_playing = playing;
        self.start_stop.time = at;
        self.start_stop.beat = self.beat_at_time(at, 0.0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    // test vectors placeholder
    #[test]
    fn test_timeline() {
        let _t = Timeline {
            tempo: Tempo(120.0),
            beat_origin: 0.0,
            time_origin: HostTime(0),
        };
    }
}
