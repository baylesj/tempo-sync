# Ableton Link Protocol Specification

This document details the Ableton Link protocol, enabling a clean-room interoperable implementation.

## 1. Transport and Network

Ableton Link operates over UDP. It runs on IPv4 and IPv6 simultaneously. 

### Sockets and Multicast Groups
* **IPv4 Multicast**: `224.76.78.75`, port `20808`.
* **IPv6 Multicast**: `ff12::8080%<scope_id>`, port `20808`, where `<scope_id>` is the interface index (link-local).
* **Socket Options**: Sockets must set loopback enabled (`IP_MULTICAST_LOOP`), reuse address (`SO_REUSEADDR`), and bind to all addresses (`0.0.0.0` or `::`). On IPv4, outbound multicasts should be explicitly configured per-interface.
* **Interface Scanning**: A peer must scan for new interfaces and drop stale interfaces every 5 seconds (`rescanPeriod = 5s`). Dropped interfaces close their sockets; newly discovered interfaces initialize fresh sockets using the peer's global `NodeId`.
* **Loopback**: When a peer receives a multicast packet whose `NodeId` matches its own, it discards the packet.

### Unicast Measurement
Clock measurement (ping/pong) is sent via **unicast** UDP. The target address is determined by the payload of Discovery messages:
* If the discovery packet arrived via IPv4, the receiver uses the `mep4` entry advertised by the sender.
* If the discovery packet arrived via IPv6, the receiver uses the `mep6` entry, substituting its own receiving interface's `scope_id` to reach the sender's link-local address.

## 2. Message Framing

All Link messages consist of a fixed-size header followed by an encoded payload. Max message size is 512 bytes. All multi-byte integers are serialized in Big Endian (Network Byte Order).

### 2.1. Discovery Messages (`Alive`, `Response`, `ByeBye`)
Discovery messages use the magic header `_asdp_v1` (8 bytes) followed by a 12-byte header:
* `MessageType` (uint8): `1` (Alive), `2` (Response), or `3` (ByeBye).
* `TTL` (uint8): Expected time-to-live in seconds for this peer (usually `5`).
* `SessionGroupId` (uint16): The group ID (default is `0`). Messages from other groups are ignored.
* `NodeId` (8 bytes): A randomly generated 8-byte array uniquely identifying the sender.

Following this header is the payload (a sequence of TLV-like entries). For `Alive` and `Response`, this contains the peer's state. For `ByeBye`, the payload is entirely empty (0 entries) and TTL is set to 0.

### 2.2. Measurement Messages (`Ping`, `Pong`)
Measurement messages use the magic header `_link_v1` (8 bytes) followed by a 1-byte header:
* `MessageType` (uint8): `1` (Ping) or `2` (Pong).

Following this is the payload.

## 3. Payload Encoding

A payload is a sequence of entries. Each entry consists of an 8-byte header followed by the value. If an entry's serialized value is 0 bytes (e.g. an unconfigured endpoint), the entry (and its 8-byte header) is omitted completely. All entries are optional, though dropping core state entries will break interop. Unknown payload entries are ignored.
* `Key` (uint32): A 4-character ASCII tag (e.g., `'tmln'` = `0x746d6c6e`).
* `Size` (uint32): The size in bytes of the serialized value.

### Core Data Types
* **Beats**: An `int64` representing micro-beats (beats * 1,000,000). e.g., 4.5 beats is `4500000`.
* **Microseconds**: An `int64` representing microseconds.
* **Tempo**: An `int64` representing microseconds per beat. Calculated as `60,000,000 / BPM`.
* **NodeId / SessionId**: An 8-byte array (8 `uint8` values).
* **Bool**: A `uint8` (`1` = true, `0` = false).

### Exact Payload Entries & Clock References

1. **Timeline** (Key: `'tmln'`) - Size: 24 bytes
   * `Tempo` (8 bytes): Microseconds per beat (`int64`).
   * `BeatOrigin` (8 bytes): The reference beat in micro-beats (`int64`).
   * `TimeOrigin` (8 bytes): The reference time in microseconds (`int64`). **Clock: Ghost Time** (Session clock).

2. **StartStopState** (Key: `'stst'`) - Size: 17 bytes
   * `IsPlaying` (1 byte): `bool`.
   * `Beats` (8 bytes): micro-beats (`int64`).
   * `Timestamp` (8 bytes): microseconds (`int64`). **Clock: Ghost Time** (Session clock).

3. **SessionMembership** (Key: `'sess'`) - Size: 8 bytes
   * `SessionId` (8 bytes): The Node ID of the session's founding peer.

4. **Measurement / Audio Endpoints** 
   * **IPv4** (Keys: `'mep4'`, `'aep4'`) - Size: 6 bytes. `Address` (4 bytes) + `Port` (2 bytes).
   * **IPv6** (Keys: `'mep6'`, `'aep6'`) - Size: 18 bytes. `Address` (16 bytes) + `Port` (2 bytes).

5. **Measurement Times** (Keys: `'__ht'`, `'__gt'`, `'_pgt'`) - Size: 8 bytes each (`int64`).
   * `__ht`: **Host Time** of the pinger.
   * `__gt`: **Ghost Time** of the ponger.
   * `_pgt`: **Ghost Time** of the ponger from the previous pong.

### Message Entry Compositions
* **Alive / Response**: Includes `tmln`, `sess`, `stst`, `mep4`, `mep6`, `aep4`, `aep6`. (`tmln` and `stst` timestamps are strictly in **Ghost Time**).
* **Ping**: Includes `__ht` (Host Time of sender). On follow-up pings, also includes `_pgt` (the Ghost Time the responder sent in the previous Pong).
* **Pong**: Includes `sess`, `__gt` (Ghost Time of responder), followed immediately by **appending the exact payload of the Ping it is replying to** (meaning the final Pong message ends with the pinger's `__ht` and `_pgt`).

## 4. Discovery State Machine

* **Broadcast**: A peer broadcasts an `Alive` message periodically to the multicast group. 
  * The nominal interval is `TTL * 1000 / TTL_Ratio`. Link uses `TTL = 5` and `TTL_Ratio = 20`, yielding a 250ms interval.
  * If a peer's internal state changes, it triggers an immediate broadcast. A hard floor of 50ms is enforced between broadcasts to prevent network flooding.
* **Response**: When a peer receives an `Alive` message from an unknown peer, or a known peer with a new state, it replies with a `Response` message sent via unicast to the sender's source UDP address (not the endpoint advertised in the payload).
* **Peer Timeout**: A peer is considered dead if no `Alive` or `Response` message is received from it within `TTL + 1` seconds (6 seconds total).

## 5. Measurement and Clock Sync

Peers exchange pings and pongs to measure the clock offset between their local "Host Time" and a peer's session "Ghost Time". 

### The Ping-Pong Process
1. **Peer A** sends a `Ping` containing its current time: `Payload(__ht=A_t1)`. It sets a 50ms timer.
2. **Peer B** receives the Ping at `B_t1`. It replies with a `Pong` containing: `Payload(sess, __gt=B_ghost_t1) + PingPayload(__ht=A_t1)`.
3. **Peer A** receives the `Pong` at `A_t2`. It *immediately* sends a follow-up `Ping` containing: `Payload(__ht=A_t2, _pgt=B_ghost_t1)`. It resets the 50ms timer.
4. **Peer B** receives this Ping at `B_t2` and sends a `Pong` containing: `Payload(sess, __gt=B_ghost_t2) + PingPayload(__ht=A_t2, _pgt=B_ghost_t1)`.
5. **Peer A** receives this second Pong at `A_t3`. It calculates two clock offset data points (assuming symmetric RTT):
   * `offset1 = B_ghost_t2 - (A_t3 + A_t2) / 2.0`
   * `offset2 = (B_ghost_t2 + B_ghost_t1) / 2.0 - A_t2`

### Lifecycle & Constants
* **Data Points**: A measurement run finishes successfully once `100` data points are collected (approx 51 ping-pong exchanges).
* **Timeouts**: If a ping is lost, the 50ms timer fires and sends another ping. A maximum of `5` timeouts are allowed before the measurement run is aborted as a failure.
* **Result**: The collected offsets are sorted, and the **median** is taken. This becomes the intercept of the `GhostXForm`. The slope is always hardcoded to `1.0`. `ghostTime = hostTime + median_offset`.
* **Remeasurement**: The active session is re-measured periodically every 30 seconds.

## 6. Session Management

* **SessionId Generation**: When starting solo, a peer's `SessionId` is its own randomly generated `NodeId`.
* **Adopting Sessions**: When a peer measures another session, it compares session precedence:
  * The session with the **older clock** wins. This is evaluated by comparing the Ghost Times at the current Host Time. If the incoming session's Ghost Time is greater than the current session's Ghost Time by more than `500,000` microseconds (0.5s), the incoming session wins.
  * If the Ghost Times are within 0.5s of each other, the session with the **lexicographically smaller** `SessionId` (byte-array comparison) wins.
* **Timeline Continuity**: When adopting a session timeline, the peer adjusts its local (Host Time) timeline so its `timeOrigin` exactly matches beat 0 of the session timeline (using the new `GhostXForm`). This preserves the continuous local beat magnitude without jarring jumps.
* **Timeline Updates (Tempo Changes)**: When a peer proposes a new timeline (e.g. tempo change), it sets the new `beatOrigin` to the current beat value (or strictly `current_beatOrigin + 1` if necessary). Other peers in the session will adopt incoming timelines if `incoming.beatOrigin > current.beatOrigin`. `beatOrigin` effectively acts as a monotonic sequence number.
* **Founder Leaves**: If the session's founding peer leaves, the `SessionId` does not change. The remaining peers continue to broadcast and use the same `SessionId`.

## 7. Start/Stop State

* **Adoption Rule**: An incoming `StartStopState` is adopted if its `SessionId` matches the current session AND its `timestamp` (in Ghost Time) is strictly greater than the current state's `timestamp`.
* **Sync Disabled**: If `startStopSyncEnabled` is off, the peer still receives, caches, and relays (broadcasts) the state to others, but it does not apply it to the application's local timeline.
* **Quantized Launch**: `requestBeatAtStartPlayingTime(beat, quantum)` evaluates to a no-op if `isPlaying` is false. If true, it calls `requestBeatAtTime(beat, startStopState.time, quantum)`.

## 8. Timeline Math and Phase Alignment

A `Timeline` establishes a bijection between time and beats. Tempo is limited to `[20.0, 999.0]` BPM. Rounding to micro-beats is done via `std::llround()`.

### Core Formulas
**Beat at Time:** `beats = beatOrigin + (time - timeOrigin) / tempo_micros_per_beat`
**Time at Beat:** `time = timeOrigin + (beats - beatOrigin) * tempo_micros_per_beat`

**Phase at Time:**
Given a `quantum` (e.g., 4.0 beats per bar):
```
phase = (beats + quantumBins * quantum) % quantum
```
Where `quantumBins` (integer arithmetic) is `(abs(beats) + quantum) / quantum`. This mathematically robust flooring modulo ensures negative beats wrap correctly into the `[0, quantum)` range.

**Quantized Launch (`requestBeatAtTime(beat, time, quantum)`):**
* **Solo Mode (0 peers)**: Forces the timeline to map `time` exactly to `beat`.
* **Session Mode (>0 peers)**: Modifies `time` to the *next* occurrence of the requested phase in the session clock:
  `time = timeAtBeat(nextPhaseMatch(beatAtTime(time, quantum), beat, quantum), quantum)`
  Then forces the beat at that new time.

**`forceBeatAtTime(beat, time, quantum)` (Internal helper):**
Calculates the required shift to align the timeline without breaking phase:
```
curBeatAtTime = toPhaseEncodedBeats(time, quantum)
closestInPhase = closestPhaseMatch(curBeatAtTime, beat, quantum)
timeDelta = timeAtBeat(closestInPhase - curBeatAtTime) - timeAtBeat(0)
timeOrigin -= timeDelta
beatOrigin += beat - closestInPhase
```

## 9. Compatibility and Versions
* The magic headers (`_asdp_v1` and `_link_v1`) embed the protocol version.
* Unknown message types (e.g., `MessageType` > 3 in discovery) and unknown TLV payload keys are gracefully ignored.
* Strict maximum message size of 512 bytes is enforced at serialization.

## 10. Timing Constants Table

| Constant | Value | Source File | Description |
| :--- | :--- | :--- | :--- |
| `kMaxMessageSize` | 512 bytes | `discovery/v1/Messages.hpp` | Maximum UDP packet size |
| `ttl` | 5 seconds | `discovery/PeerGateway.hpp` | Broadcast TTL |
| `ttlRatio` | 20 | `discovery/PeerGateway.hpp` | Divisor for nominal broadcast interval |
| `minBroadcastPeriod`| 50 ms | `discovery/UdpMessenger.hpp` | Minimum time between discovery broadcasts |
| `rescanPeriod` | 5 seconds | `discovery/Service.hpp` | Interface scanning interval |
| `kNumberMeasurements` | 5 | `link/Measurement.hpp` | Max ping timeouts before measurement fails |
| `kNumberDataPoints` | 100 | `link/Measurement.hpp` | Target offset samples to complete measurement |
| Measurement Timer | 50 ms | `link/Measurement.hpp` | Timeout waiting for a Pong |
| Remeasurement Timer | 30,000,000 µs (30s)| `link/Sessions.hpp` | Interval for periodically re-measuring session |
| `SESSION_EPS` | 500,000 µs (0.5s) | `link/Sessions.hpp` | Ghost time difference threshold for session tie-breaking |
| Min Tempo | 20.0 BPM | `link/ClientSessionTimelines.hpp`| Minimum allowed tempo |
| Max Tempo | 999.0 BPM | `link/ClientSessionTimelines.hpp`| Maximum allowed tempo |
