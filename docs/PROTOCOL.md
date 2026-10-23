# Ableton Link Protocol Specification

This document details the Ableton Link protocol, enabling a clean-room interoperable implementation.

## 1. Transport and Network

Ableton Link operates over UDP. It runs on IPv4 and IPv6 simultaneously. 

### Sockets and Multicast Groups
* **IPv4 Multicast**: `224.76.78.75`, port `20808`.
* **IPv6 Multicast**: `ff12::8080%<scope_id>`, port `20808`, where `<scope_id>` is the interface index (link-local).
* **Socket Options**: Sockets must set loopback enabled (`IP_MULTICAST_LOOP`), reuse address (`SO_REUSEADDR`), and bind to all addresses (`0.0.0.0` or `::`). On IPv4, outbound multicasts should be explicitly configured per-interface.
* **Interface Scanning**: A peer must scan for new interfaces and drop stale interfaces every 5 seconds (`rescanPeriod = 5s`).

### Measurement (Unicast)
Clock measurement (ping/pong) is sent via **unicast** UDP to the endpoints advertised by peers in the Discovery state machine.

## 2. Message Framing

All Link messages consist of a fixed-size header followed by an encoded payload. Max message size is 512 bytes. All multi-byte integers are serialized in Big Endian (Network Byte Order).

### 2.1. Discovery Messages
Discovery messages use the magic header `_asdp_v1` (8 bytes) followed by a 12-byte header:
* `MessageType` (uint8): `1` (Alive), `2` (Response), or `3` (ByeBye).
* `TTL` (uint8): Expected time-to-live in seconds for this peer (usually `5`).
* `SessionGroupId` (uint16): The group ID (default is `0`). Messages from other groups are ignored.
* `NodeId` (8 bytes): A randomly generated 8-byte array uniquely identifying the sender.

Following this header is the payload (a sequence of TLV-like entries).

### 2.2. Measurement Messages
Measurement messages use the magic header `_link_v1` (8 bytes) followed by a 1-byte header:
* `MessageType` (uint8): `1` (Ping) or `2` (Pong).

Following this is the payload.

## 3. Payload Encoding

A payload is a sequence of entries. Each entry consists of an 8-byte header followed by the value. If an entry's serialized value is 0 bytes, the entry (and its header) is omitted completely.
* `Key` (uint32): A 4-character ASCII tag (e.g., `'tmln'` = `0x746d6c6e`).
* `Size` (uint32): The size in bytes of the serialized value.

Unknown payload entries are ignored.

### Core Data Types
* **Beats**: An `int64` representing micro-beats (beats * 1,000,000). e.g., 4.5 beats is `4500000`.
* **Microseconds**: An `int64` representing microseconds.
* **Tempo**: An `int64` representing microseconds per beat. Calculated as `60,000,000 / BPM`.
* **NodeId / SessionId**: An 8-byte array (8 `uint8` values).
* **Bool**: A `uint8` (`1` = true, `0` = false).

### Payload Entries

1. **Timeline** (Key: `'tmln'`)
   * `Tempo` (8 bytes): Microseconds per beat (`int64`).
   * `BeatOrigin` (8 bytes): The reference beat in micro-beats (`int64`).
   * `TimeOrigin` (8 bytes): The reference time in microseconds (`int64`).
   * Total Size: 24 bytes.

2. **StartStopState** (Key: `'stst'`)
   * `IsPlaying` (1 byte): `bool`.
   * `Beats` (8 bytes): micro-beats (`int64`).
   * `Timestamp` (8 bytes): microseconds (`int64`).
   * Total Size: 17 bytes.

3. **SessionMembership** (Key: `'sess'`)
   * `SessionId` (8 bytes): The Node ID of the session's founding peer.
   * Total Size: 8 bytes.

4. **HostTime** (Key: `'__ht'`)
   * `Time` (8 bytes): microseconds (`int64`).

5. **GHostTime** (Key: `'__gt'`)
   * `Time` (8 bytes): microseconds (`int64`).

6. **PrevGHostTime** (Key: `'_pgt'`)
   * `Time` (8 bytes): microseconds (`int64`).

*Note: Discovery messages may also contain IPv4/IPv6 endpoint advertisement entries (`mep4`, `mep6`, `aep4`, `aep6`) which tell peers where to send unicast measurement pings.*

## 4. Discovery State Machine

* **NodeState**: A peer tracks its `NodeId`, `SessionId`, `Timeline`, and `StartStopState`.
* **Broadcast**: A peer broadcasts an `Alive` message periodically to the multicast group. 
  * The nominal interval is `TTL * 1000 / TTL_Ratio`. Link uses `TTL = 5` and `TTL_Ratio = 20`, yielding a 250ms interval.
  * A hard lower bound of 50ms is enforced between broadcasts to prevent flooding.
* **Response**: When a peer receives an `Alive` message from an unknown peer, or a known peer with a new state, it responds by sending a `Response` message via unicast back to the sender's endpoint to ensure symmetric discovery.
* **ByeBye**: When shutting down gracefully, a peer broadcasts a `ByeBye` message (TTL = 0).
* **Peer Timeout**: A peer is considered dead if no `Alive` or `Response` message is received from it within `TTL` seconds (plus a 1-second padding interval).

## 5. Measurement and Clock Sync

Peers exchange pings and pongs to measure the clock offset between their local "host time" and a peer's "ghost time". The `GhostXForm` is a linear transform with `slope = 1.0` and `intercept` representing the offset.

### The Ping-Pong Process
1. **Peer A** sends a `Ping` containing its current time: `Payload(HostTime=A_t1)`.
2. **Peer B** receives the Ping at `B_t1`. It replies with a `Pong` containing:
   * `SessionMembership`
   * `GHostTime` = `B_t1` (in B's timeline).
   * **Appends** the entire payload of the received Ping (so the Pong contains A's `HostTime`).
3. **Peer A** receives the `Pong` at `A_t2`. It immediately sends a follow-up `Ping` containing:
   * `HostTime` = `A_t2`.
   * `PrevGHostTime` = `B_t1` (extracted from the Pong).
4. **Peer B** receives this Ping at `B_t2` and sends a `Pong` containing:
   * `SessionMembership`
   * `GHostTime` = `B_t2`.
   * **Appends** the Ping payload (`HostTime`=`A_t2`, `PrevGHostTime`=`B_t1`).
5. **Peer A** receives this second Pong at `A_t3`. It calculates offset data points:
   * `offset1 = B_t2 - (A_t3 + A_t2) / 2.0`
   * `offset2 = (B_t2 + B_t1) / 2.0 - A_t2`
   * These points are collected.

Link collects up to 100 data points. It takes the **median** of the collected offsets to establish the `intercept` for the `GhostXForm`. 

## 6. Session Management

* **SessionId Generation**: When starting solo, a peer's `SessionId` is its own `NodeId`.
* **Adopting Sessions**: When a peer discovers another peer, it compares `SessionId`s. To resolve conflicts, the peer with the lexicographically larger `SessionId` wins, and the other peer adopts it.
* **Timeline Updates**: If a peer adopts a session timeline, it adjusts its local timeline so that its `timeOrigin` matches beat 0 of the session timeline (using the `GhostXForm` to translate time), preserving its continuous local beat magnitude.
* **StartStopState**: This state persists locally and is broadcast. A newer start/stop request (based on the `timestamp` field) wins and propagates.

## 7. Timeline Math and Phase Alignment

A `Timeline` establishes a bijection between host time and beats. Tempo is limited to the range `[20.0, 999.0]` BPM.

### Core Formulas

**Beat at Time:**
```
beats = beatOrigin + (time - timeOrigin) / tempo_micros_per_beat
```

**Time at Beat:**
```
time = timeOrigin + (beats - beatOrigin) * tempo_micros_per_beat
```

**Phase at Time:**
Given a `quantum` (e.g., 4.0 beats per bar):
```
phase = (beats + quantumBins * quantum) % quantum
```
Where `quantumBins` is computed (in integer arithmetic) as `(abs(beats) + quantum) / quantum` to ensure the operand is positive before the modulo. This acts like `fmod(fmod(beats, quantum) + quantum, quantum)`.

**Request Beat at Time (Quantized Launch):**
If a peer is solo, `requestBeatAtTime(beat, time, quantum)` forces `beatAtTime(time) == beat`. 
If in a session, it waits for the next time the session phase matches the requested event's phase. It finds the next time greater than `time` where the phase matches, and schedules the change there.

## 8. Client API Semantics

* **Capture / Commit**: The API uses a non-blocking lock-free state copy for the audio thread. `captureAppSessionState` returns a thread-local copy of the session state. Modifications (like `requestBeatAtTime`) modify this copy, which is then committed back (`commitAppSessionState`).
* **Start/Stop State**: Changing `isPlaying` records the time of the request in `timestamp`. Peers adopt the state with the highest timestamp.
