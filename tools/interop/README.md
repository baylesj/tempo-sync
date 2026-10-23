# Interoperability Test Harness

This directory contains the automated interoperability test harness for `tempo-sync`.

The test harness verifies that the clean-room Rust implementation communicates correctly with the official Ableton Link C++ implementation.

## Overview

Ableton Link uses UDP multicast discovery and a peer-to-peer timeline protocol over UDP. To verify protocol compatibility without licensing issues, we run two separate processes:
1. `tools/linkpeer/linkpeer`: A test oracle built against official Ableton Link C++ source headers.
2. `tempohut`: The native Rust CLI binary from this repository.

Both binaries speak the same JSON-over-stdout streaming format and accept newline-delimited commands on standard input.

## Line Protocol

### Periodic Status Line

Each peer periodically outputs one JSON line to standard output:

```json
{
  "host_time_us": 177171170130,
  "tempo": 120.0,
  "beat": 4.125,
  "phase": 0.125,
  "quantum": 4.0,
  "num_peers": 1,
  "is_playing": false,
  "time_for_is_playing_us": 177171069702
}
```

Fields:
- `host_time_us`: Host clock timestamp in microseconds. On Linux, this is monotonic steady clock time.
- `tempo`: Session tempo in beats per minute (BPM).
- `beat`: Current beat position computed at `host_time_us` for the given quantum.
- `phase`: Current phase within the quantum, in `[0, quantum)`.
- `quantum`: Beat quantum (default 4.0).
- `num_peers`: Number of other Link peers connected in the session.
- `is_playing`: Transport playback state boolean.
- `time_for_is_playing_us`: Host timestamp of the most recent start or stop transport transition.

### Standard Input Commands

Peers accept single-line commands on standard input and reply with an acknowledgment JSON object:

- `tempo <bpm>`: Updates session tempo. Acknowledged with `{"ack": "tempo"}`.
- `play`: Starts transport playback. Acknowledged with `{"ack": "play"}`.
- `stop`: Stops transport playback. Acknowledged with `{"ack": "stop"}`.
- `beat <beat> at <host_time_us>`: Requests beat alignment at host time. Acknowledged with `{"ack": "beat"}`.
- `force <beat> at <host_time_us>`: Forces beat alignment at host time. Acknowledged with `{"ack": "force"}`.
- `quit`: Closes the session and exits cleanly. Acknowledged with `{"ack": "quit"}`.

## Test Workflow

The script `run_interop.py` runs the following sequence:

1. **Process Launch**: Starts `linkpeer` (Peer 1) and `tempohut` (Peer 2).
2. **Peer Discovery**: Asserts both peers report `num_peers >= 1` within the timeout.
3. **Timeline Alignment**: Asserts initial tempos match within 0.01 BPM. Projects Peer 1's beat value to Peer 2's timestamp using monotonic host time and verifies beat and phase agreement.
4. **Peer 1 Tempo Change**: Peer 1 sets tempo to 136.0 BPM. Asserts Peer 2 follows to 136.0 BPM within 0.01 BPM.
5. **Peer 2 Tempo Change**: Peer 2 sets tempo to 114.0 BPM. Asserts Peer 1 follows to 114.0 BPM within 0.01 BPM.
6. **Peer 1 Transport Start**: Peer 1 sends `play`. Asserts Peer 2 observes `is_playing == true`.
7. **Peer 2 Transport Stop**: Peer 2 sends `stop`. Asserts Peer 1 observes `is_playing == false`.
8. **Clean Shutdown**: Sends `quit` to both peers and asserts zero exit status.

## Usage

### 1. Build the C++ Oracle

Ensure the Ableton Link headers are present, then build `linkpeer`:

```bash
tools/linkpeer/build.sh
```

You can customize the Link source directory by setting `LINK_DIR`:

```bash
LINK_DIR=/path/to/ableton/link tools/linkpeer/build.sh
```

### 2. Run Interop in Standalone Oracle Mode

You can verify the test harness today without the Rust binary by running two C++ `linkpeer` processes against each other:

```bash
./tools/interop/run_interop.py --skip-rust
```

### 3. Run Interop with Rust Tempohut

Once the Rust workspace is present:

```bash
./tools/interop/run_interop.py
```

To specify a custom path to the Rust executable or command line flags:

```bash
./tools/interop/run_interop.py --tempohut-cmd "cargo run -p tempohut -- --json"
```
