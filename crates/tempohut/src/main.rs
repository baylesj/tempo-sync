// SPDX-License-Identifier: Apache-2.0

use std::env;
use std::io::{self, BufRead};
use std::net::Ipv4Addr;
use std::sync::mpsc;
use std::thread;
use std::time::Duration;
use tempo_sync_std::{Runtime, Clock};
use tempo_sync_core::{NodeId, HostTime};

fn main() {
    let args: Vec<String> = env::args().collect();
    let mut is_json = false;
    let mut tempo = 120.0;
    let mut quantum = 4.0;
    let mut interval_ms = 100;
    let mut ifaces = Vec::new();

    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--json" => is_json = true,
            "--tempo" => {
                i += 1;
                if i < args.len() { tempo = args[i].parse().unwrap_or(tempo); }
            },
            "--quantum" => {
                i += 1;
                if i < args.len() { quantum = args[i].parse().unwrap_or(quantum); }
            },
            "--interval-ms" => {
                i += 1;
                if i < args.len() { interval_ms = args[i].parse().unwrap_or(interval_ms); }
            },
            "--iface" => {
                i += 1;
                if i < args.len() { 
                    if let Ok(ip) = args[i].parse::<Ipv4Addr>() {
                        ifaces.push(ip);
                    }
                }
            },
            _ => {}
        }
        i += 1;
    }

    if ifaces.is_empty() {
        ifaces.push(Ipv4Addr::new(127, 0, 0, 1));
    }

    let node_id = NodeId([0, 1, 2, 3, 4, 5, 6, 7]);
    let runtime = Runtime::new(node_id, ifaces);
    let session = runtime.handle();
    let clock = Clock::new();

    thread::spawn(move || {
        runtime.run();
    });

    let (tx, rx) = mpsc::channel();
    
    thread::spawn(move || {
        let stdin = io::stdin();
        for line in stdin.lock().lines() {
            if let Ok(l) = line {
                let _ = tx.send(l);
            }
        }
    });

    loop {
        match rx.recv_timeout(Duration::from_millis(interval_ms)) {
            Ok(line) => {
                let parts: Vec<&str> = line.split_whitespace().collect();
                if parts.is_empty() { continue; }
                
                let mut state = session.capture_state().unwrap();
                let now = clock.now();
                let mut ack = "";

                match parts[0] {
                    "quit" => {
                        if is_json { println!("{{\"ack\": \"quit\"}}"); }
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
                        if let (Ok(beat), Ok(time)) = (parts[1].parse::<f64>(), parts[3].parse::<i64>()) {
                            state.request_beat_at_time(HostTime(time), beat, quantum);
                            session.commit_state(state);
                            ack = "beat";
                        }
                    }
                    "force" if parts.len() > 3 && parts[2] == "at" => {
                        if let (Ok(beat), Ok(time)) = (parts[1].parse::<f64>(), parts[3].parse::<i64>()) {
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
