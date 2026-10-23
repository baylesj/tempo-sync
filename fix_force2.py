with open("crates/tempo-sync-core/src/timeline.rs", "r") as f:
    data = f.read()

import re
data = re.sub(
    r'pub fn force_beat_at_time.*?\n    }',
    r'''pub fn force_beat_at_time(&mut self, time: HostTime, beat: Beats) {
        let dt = time.0.wrapping_sub(self.timeline.time_origin.0) as f64;
        let dbeats = dt / (self.timeline.tempo.0 as f64);
        self.timeline.beat_origin = MicroBeats::from_beats(beat - dbeats);
    }''',
    data,
    flags=re.DOTALL
)

with open("crates/tempo-sync-core/src/timeline.rs", "w") as f:
    f.write(data)
