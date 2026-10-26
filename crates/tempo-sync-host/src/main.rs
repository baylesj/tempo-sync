// SPDX-License-Identifier: Apache-2.0

//! Native messaging host for the Web Tempo Sync extension.
//!
//! Chrome starts this process when the extension's service worker calls `connectNative`, and
//! closes stdin when it disconnects. One process is one Link peer, shared by every tab, which
//! is the same shared endpoint the spec gives the browser.
//!
//! Messages are Chrome's native messaging framing: a native-endian u32 length, then UTF-8 JSON.
//! Session times cross the boundary as integers in the runtime's own units (microseconds,
//! micro-beats, microseconds per beat) so a capture and commit round-trip loses nothing.
//!
//! In:  `{"type":"ping","id":…}`, `{"type":"commit","state":{…}}`
//! Out: `{"type":"pong","id":…,"hostTime":µs}`, `{"type":"state","state":{…},"numPeers":n}`

use serde_json::{json, Value};
use std::io::{self, Read, Write};
use std::net::Ipv4Addr;
use std::sync::mpsc::{self, RecvTimeoutError};
use std::thread;
use std::time::Duration;
use tempo_sync_core::time::{MicroBeats, TempoWire};
use tempo_sync_core::{HostTime, NodeId, SessionState};
use tempo_sync_std::Runtime;

/// How often to look for state or peer changes when no message arrives. Changes from the
/// network reach the page at most this late.
const POLL: Duration = Duration::from_millis(5);

/// Chrome caps messages to the host at 4 GB but messages from it at 1 MB; ours are tiny, so
/// anything large is a framing error.
const MAX_MESSAGE: usize = 1 << 20;

fn read_message(r: &mut impl Read) -> io::Result<Option<Value>> {
    let mut len = [0u8; 4];
    match r.read_exact(&mut len) {
        Ok(()) => {}
        Err(e) if e.kind() == io::ErrorKind::UnexpectedEof => return Ok(None),
        Err(e) => return Err(e),
    }
    let len = u32::from_ne_bytes(len) as usize;
    if len > MAX_MESSAGE {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "message too long",
        ));
    }
    let mut buf = vec![0u8; len];
    r.read_exact(&mut buf)?;
    serde_json::from_slice(&buf)
        .map(Some)
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
}

fn write_message(w: &mut impl Write, msg: &Value) -> io::Result<()> {
    let bytes = serde_json::to_vec(msg)?;
    w.write_all(&(bytes.len() as u32).to_ne_bytes())?;
    w.write_all(&bytes)?;
    w.flush()
}

fn state_to_json(s: &SessionState) -> Value {
    json!({
        "tempo": s.timeline.tempo.0,
        "beatOrigin": s.timeline.beat_origin.0,
        "timeOrigin": s.timeline.time_origin.0,
        "isPlaying": s.start_stop.is_playing,
        "startStopBeat": s.start_stop.beat.0,
        "startStopTime": s.start_stop.time.0,
    })
}

fn state_from_json(v: &Value) -> Option<SessionState> {
    let int = |k: &str| v.get(k)?.as_i64();
    let tempo = int("tempo")?;
    if tempo <= 0 {
        return None;
    }
    let mut s = SessionState::default();
    s.timeline.tempo = TempoWire(tempo);
    s.timeline.beat_origin = MicroBeats(int("beatOrigin")?);
    s.timeline.time_origin = HostTime(int("timeOrigin")?);
    s.start_stop.is_playing = v.get("isPlaying")?.as_bool()?;
    s.start_stop.beat = MicroBeats(int("startStopBeat")?);
    s.start_stop.time = HostTime(int("startStopTime")?);
    Some(s)
}

/// The interface the OS would route the Link multicast group through, as tempohut picks it.
fn default_iface() -> Ipv4Addr {
    std::net::UdpSocket::bind("0.0.0.0:0")
        .and_then(|probe| {
            probe.connect("224.76.78.75:20808")?;
            probe.local_addr()
        })
        .ok()
        .and_then(|addr| match addr {
            std::net::SocketAddr::V4(v4) if !v4.ip().is_unspecified() => Some(*v4.ip()),
            _ => None,
        })
        .unwrap_or(Ipv4Addr::LOCALHOST)
}

fn main() {
    let label = format!("WT{:06X}", std::process::id() & 0xFF_FFFF);
    let mut node_id = [0u8; 8];
    node_id.copy_from_slice(&label.as_bytes()[..8]);

    let mut runtime = Runtime::new(NodeId(node_id), vec![default_iface()]);
    // Tests set this to get a private session that real Link apps never see. Chrome starts the
    // host with its own environment, so the browser under test passes it on.
    if let Some(port) = std::env::var("TEMPO_SYNC_PORT")
        .ok()
        .and_then(|p| p.parse().ok())
    {
        runtime.set_port(port);
    }
    let clock = runtime.clock();
    let session = runtime.handle();
    thread::spawn(move || runtime.run());

    let (tx, rx) = mpsc::channel();
    thread::spawn(move || {
        let mut stdin = io::stdin().lock();
        loop {
            match read_message(&mut stdin) {
                Ok(Some(msg)) => {
                    if tx.send(msg).is_err() {
                        return;
                    }
                }
                Ok(None) => return,
                Err(e) => {
                    eprintln!("tempo-sync-host: {e}");
                    return;
                }
            }
        }
    });

    let mut stdout = io::stdout().lock();
    let mut last: Option<(SessionState, u32)> = None;
    loop {
        match rx.recv_timeout(POLL) {
            Ok(msg) => match msg.get("type").and_then(Value::as_str) {
                // Answer before anything else: the page times this round trip to find the
                // offset between its clock and ours, so any delay here becomes error there.
                Some("ping") => {
                    let reply = json!({
                        "type": "pong",
                        "id": msg.get("id").cloned().unwrap_or(Value::Null),
                        "hostTime": clock.now().0,
                    });
                    if write_message(&mut stdout, &reply).is_err() {
                        break;
                    }
                }
                Some("commit") => match msg.get("state").and_then(state_from_json) {
                    Some(state) => session.commit_state(state),
                    None => eprintln!("tempo-sync-host: ignoring malformed commit"),
                },
                _ => eprintln!("tempo-sync-host: ignoring unknown message"),
            },
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => break,
        }

        if let Some(state) = session.capture_state() {
            let current = Some((state, session.num_peers()));
            if current != last {
                let (state, peers) = current.as_ref().unwrap();
                let msg =
                    json!({"type": "state", "state": state_to_json(state), "numPeers": peers});
                if write_message(&mut stdout, &msg).is_err() {
                    break;
                }
                last = current;
            }
        }
    }

    // Say goodbye to the other peers so they drop us now rather than at timeout.
    session.shutdown();
    thread::sleep(Duration::from_millis(50));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn framing_round_trips() {
        let msg = json!({"type": "ping", "id": 7});
        let mut buf = Vec::new();
        write_message(&mut buf, &msg).unwrap();
        assert_eq!(&buf[..4], &(buf.len() as u32 - 4).to_ne_bytes());
        let mut r = io::Cursor::new(buf);
        assert_eq!(read_message(&mut r).unwrap(), Some(msg));
        assert_eq!(read_message(&mut r).unwrap(), None);
    }

    #[test]
    fn state_round_trips_losslessly() {
        let mut s = SessionState::default();
        s.timeline.tempo = TempoWire(476_190);
        s.timeline.beat_origin = MicroBeats(-1_234_567);
        s.timeline.time_origin = HostTime(9_876_543_210);
        s.start_stop.is_playing = true;
        s.start_stop.beat = MicroBeats(8_000_000);
        s.start_stop.time = HostTime(9_876_600_000);
        assert_eq!(state_from_json(&state_to_json(&s)), Some(s));
    }

    #[test]
    fn rejects_nonpositive_tempo() {
        let mut v = state_to_json(&SessionState::default());
        v["tempo"] = json!(0);
        assert_eq!(state_from_json(&v), None);
    }
}
