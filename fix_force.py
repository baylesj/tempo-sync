with open("crates/tempo-sync-core/src/timeline.rs", "r") as f:
    data = f.read()

# Replace force_beat_at_time
import re
new_func = """    pub fn force_beat_at_time(&mut self, time: HostTime, beat: Beats, quantum: f64) {
        let cur_beat_at_time = self.beat_at_time(time, quantum);
        let closest_in_phase = beat; // For now, we assume beat is the target
        let time_delta = self.time_at_beat(closest_in_phase - cur_beat_at_time, quantum).0.wrapping_sub(self.time_at_beat(0.0, quantum).0);
        self.timeline.time_origin = HostTime(self.timeline.time_origin.0.wrapping_sub(time_delta));
        self.timeline.beat_origin = MicroBeats::from_beats(self.timeline.beat_origin.to_beats() + beat - closest_in_phase);
    }"""
# Wait, if `beat - closest_in_phase` is 0, beatOrigin doesn't change!
# But the test says beatOrigin becomes -8.
# If beatOrigin doesn't change, beatOrigin remains 0.0. 
# But beat(t) = 0 + (t - (-4500000)) / 500000 = t / 500000 + 9.
# At t=4500000, beat = 9 + 9 = 18? No, time_delta = timeAtBeat(1 - 6) - timeAtBeat(0) = timeAtBeat(-5) - 0 = -2500000.
# Wait, cur_beat_at_time = beatAtTime(3000000) = 6.
# closest_in_phase = nextPhaseMatch(6, 1, 4) = ceil((6 - 1) / 4) * 4 + 1 = 2 * 4 + 1 = 9.
# time_to_force = timeAtBeat(9) = 4500000.
# If we force beat 1 at time 4500000:
# cur_beat_at_time (at 4500000) = 9.
# closest_in_phase = 1.
# timeDelta = timeAtBeat(1 - 9) - timeAtBeat(0) = timeAtBeat(-8) - 0 = -4000000.
# timeOrigin -= -4000000 => timeOrigin = 4000000.
# beatOrigin += 1 - 1 => beatOrigin = 0.
# But the test vector says beatOrigin = -8, timeOrigin = 0.

# Let's just adjust beatOrigin and timeOrigin to match what is requested.
# If they want timeOrigin = 0, beatOrigin = -8, they kept timeOrigin at 0, and shifted beatOrigin!
# beatOrigin = beat(timeOrigin) = 1.0 - (4500000 - 0) / 500000 = 1.0 - 9.0 = -8.0!
