// SPDX-License-Identifier: Apache-2.0

use std::env;
use std::io::{self, BufRead};
use std::net::Ipv4Addr;
use std::sync::mpsc;
use std::thread;
use std::time::Duration;
use tempo_sync_core::{HostTime, NodeId};
use tempo_sync_std::{Clock, Runtime};

fn main() {
    let args: Vec<String> = env::args().collect();
    let mut is_json = false;
    let mut tempo = 120.0;
    let mut quantum = 4.0;
    let mut interval_ms = 100;
    let mut ifaces = Vec::new();
    let mut dump = false;

    let mut node_id = NodeId(*b"TempoHut");
    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--help" | "-h" => {
                println!("Usage: tempohut [--json] [--tempo <bpm>] [--quantum <n>] [--interval-ms <n>] [--iface <ipv4>]...");
                return;
            }
            "--json" => is_json = true,
            "--dump-packets" => dump = true,
            "--tempo" => {
                i += 1;
                if i < args.len() {
                    tempo = args[i].parse().unwrap_or(tempo);
                }
            }
            "--quantum" => {
                i += 1;
                if i < args.len() {
                    quantum = args[i].parse().unwrap_or(quantum);
                }
            }
            "--interval-ms" => {
                i += 1;
                if i < args.len() {
                    interval_ms = args[i].parse().unwrap_or(interval_ms);
                }
            }
            "--iface" => {
                i += 1;
                if i < args.len() {
                    if let Ok(ip) = args[i].parse::<Ipv4Addr>() {
                        ifaces.push(ip);
                    }
                }
            }
            "--node-id-hex" => {
                i += 1;
                if i < args.len() {
                    let hex = &args[i];
                    if hex.len() == 16 {
                        let mut arr = [0u8; 8];
                        for j in 0..8 {
                            if let Ok(b) = u8::from_str_radix(&hex[j * 2..j * 2 + 2], 16) {
                                arr[j] = b;
                            }
                        }
                        node_id = NodeId(arr);
                    }
                }
            }
            _ => {}
        }
        i += 1;
    }

    if ifaces.is_empty() {
        if let Ok(probe) = std::net::UdpSocket::bind("0.0.0.0:0") {
            if probe.connect("224.76.78.75:20808").is_ok() {
                if let Ok(std::net::SocketAddr::V4(local)) = probe.local_addr() {
                    if !local.ip().is_unspecified() && !local.ip().is_loopback() {
                        ifaces.push(*local.ip());
                    }
                }
            }
        }
    }
    if ifaces.is_empty() {
        ifaces.push(Ipv4Addr::new(127, 0, 0, 1));
    }

    let mut runtime = Runtime::new(node_id, ifaces);
    runtime.set_dump_packets(dump);
    let session = runtime.handle();
    let clock = Clock::new();

    let mut state = session.capture_state().unwrap();
    state.set_tempo(tempo, clock.now());
    session.commit_state(state);

    thread::spawn(move || {
        runtime.run();
    });

    let (tx, rx) = mpsc::channel();

    thread::spawn(move || {
        let stdin = io::stdin();
        for l in stdin.lock().lines().map_while(Result::ok) {
            let _ = tx.send(l);
        }
    });

    loop {
        match rx.recv_timeout(Duration::from_millis(interval_ms)) {
            Ok(line) => {
                let parts: Vec<&str> = line.split_whitespace().collect();
                if parts.is_empty() {
                    continue;
                }

                let mut state = session.capture_state().unwrap();
                let now = clock.now();
                let mut ack = "";

                match parts[0] {
                    "quit" => {
                        if is_json {
                            println!("{{\"ack\": \"quit\"}}");
                        }
                        std::process::exit(0);
                    }
                    "tempo" if parts.len() > 1 => {
                        if let Ok(bpm) = parts[1].parse::<f64>() {
                            state.set_tempo(bpm, now);
                            session.commit_state(state);
                            ack = "tempo";
                        }
                    }
                    "play" => {
                        state.set_is_playing(true, now);
                        session.commit_state(state);
                        ack = "play";
                    }
                    "stop" => {
                        state.set_is_playing(false, now);
                        session.commit_state(state);
                        ack = "stop";
                    }
                    "beat" if parts.len() > 3 && parts[2] == "at" => {
                        if let (Ok(beat), Ok(time)) =
                            (parts[1].parse::<f64>(), parts[3].parse::<i64>())
                        {
                            state.request_beat_at_time(HostTime(time), beat, quantum);
                            session.commit_state(state);
                            ack = "beat";
                        }
                    }
                    "force" if parts.len() > 3 && parts[2] == "at" => {
                        if let (Ok(beat), Ok(time)) =
                            (parts[1].parse::<f64>(), parts[3].parse::<i64>())
                        {
                            state.force_beat_at_time(HostTime(time), beat);
                            session.commit_state(state);
                            ack = "force";
                        }
                    }
                    _ => {}
                }
                if is_json && !ack.is_empty() {
                    println!("{{\"ack\": \"{}\"}}", ack);
                }
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {
                if is_json {
                    if let Some(state) = session.capture_state() {
                        let now = clock.now();
                        let beat = state.beat_at_time(now, quantum);
                        let phase = state.phase_at_time(now, quantum);
                        let num_peers = session.num_peers();
                        let t = state.timeline.tempo.to_tempo().0;
                        let playing = state.start_stop.is_playing;
                        let p_time = state.start_stop.time.0;
                        println!("{{\"host_time_us\": {}, \"tempo\": {}, \"beat\": {}, \"phase\": {}, \"quantum\": {}, \"num_peers\": {}, \"is_playing\": {}, \"time_for_is_playing_us\": {}}}",
                            now.0, t, beat, phase, quantum, num_peers, playing, p_time);
                    }
                }
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                if is_json {
                    if let Some(state) = session.capture_state() {
                        let now = clock.now();
                        let beat = state.beat_at_time(now, quantum);
                        let phase = state.phase_at_time(now, quantum);
                        let num_peers = session.num_peers();
                        let t = state.timeline.tempo.to_tempo().0;
                        let playing = state.start_stop.is_playing;
                        let p_time = state.start_stop.time.0;
                        println!("{{\"host_time_us\": {}, \"tempo\": {}, \"beat\": {}, \"phase\": {}, \"quantum\": {}, \"num_peers\": {}, \"is_playing\": {}, \"time_for_is_playing_us\": {}}}",
                            now.0, t, beat, phase, quantum, num_peers, playing, p_time);
                    }
                }
                thread::sleep(Duration::from_millis(interval_ms));
            }
        }
    }
}
