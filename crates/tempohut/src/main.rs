// SPDX-License-Identifier: Apache-2.0

use std::env;
use std::io::{self, BufRead};
#[allow(unused_imports)]
use tempo_sync_std::{Runtime, Clock};

fn main() {
    let args: Vec<String> = env::args().collect();
    let mut _is_json = false;
    let mut tempo = 120.0;
    let mut quantum = 4.0;
    let mut interval_ms = 100;
    let mut ifaces = Vec::new();

    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--json" => _is_json = true,
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
                if i < args.len() { ifaces.push(args[i].clone()); }
            },
            _ => {}
        }
        i += 1;
    }

    // Silence unused variables for now
    let _ = tempo;
    let _ = quantum;
    let _ = interval_ms;
    let _ = ifaces;

    // CLI loop placeholder
    // Reads stdin commands: "tempo <bpm>", "play", "stop", "beat <beat> at <host_time_us>", "force <beat> at <host_time_us>", "quit"
    let stdin = io::stdin();
    for line in stdin.lock().lines() {
        let line = line.unwrap();
        if line == "quit" {
            println!("{{\"ack\": \"quit\"}}");
            break;
        }
    }
}
