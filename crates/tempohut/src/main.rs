// SPDX-License-Identifier: Apache-2.0

use std::env;
use std::io::{self, BufRead, Read, Write};
use std::net::Ipv4Addr;
use std::sync::mpsc;
use std::thread;
use std::time::Duration;
use tempo_sync_core::{HostTime, NodeId};
use tempo_sync_std::{Clock, Runtime, SessionHandle};

struct RawTerminal {
    orig: Option<libc::termios>,
    alt_screen: bool,
}

impl RawTerminal {
    fn enter() -> Self {
        let mut orig = None;
        unsafe {
            if libc::isatty(libc::STDIN_FILENO) == 1 {
                let mut t: libc::termios = std::mem::zeroed();
                if libc::tcgetattr(libc::STDIN_FILENO, &mut t) == 0 {
                    orig = Some(t);
                    let mut raw = t;
                    raw.c_lflag &= !(libc::ICANON | libc::ECHO | libc::ISIG);
                    raw.c_iflag &= !(libc::IXON | libc::ICRNL);
                    raw.c_cc[libc::VMIN] = 1;
                    raw.c_cc[libc::VTIME] = 0;
                    libc::tcsetattr(libc::STDIN_FILENO, libc::TCSANOW, &raw);
                }
            }
        }
        let mut out = io::stdout();
        let _ = write!(out, "\x1b[?1049h\x1b[?25l\x1b[2J\x1b[H");
        let _ = out.flush();
        Self {
            orig,
            alt_screen: true,
        }
    }
}

impl Drop for RawTerminal {
    fn drop(&mut self) {
        if let Some(ref orig) = self.orig {
            unsafe {
                libc::tcsetattr(libc::STDIN_FILENO, libc::TCSANOW, orig);
            }
        }
        if self.alt_screen {
            let mut out = io::stdout();
            let _ = write!(out, "\x1b[0m\x1b[?25h\x1b[?1049l");
            let _ = out.flush();
        }
    }
}

enum KeyCmd {
    TogglePlay,
    TempoDelta(f64),
    SetQuantum(f64),
    ResetBeat,
    Quit,
}

fn spawn_key_reader(tx: mpsc::Sender<KeyCmd>) {
    thread::spawn(move || {
        let stdin = io::stdin();
        let mut handle = stdin.lock();
        let mut buf = [0u8; 8];
        while let Ok(n) = handle.read(&mut buf) {
            if n == 0 {
                break;
            }
            let cmd = match &buf[..n] {
                [b'q'] | [b'Q'] | [3] | [4] | [27] => Some(KeyCmd::Quit),
                [b' '] => Some(KeyCmd::TogglePlay),
                [b'+'] | [b'='] | [b'w'] | [b'W'] | [b'k'] | [27, b'[', b'A'] => {
                    Some(KeyCmd::TempoDelta(1.0))
                }
                [b'-'] | [b'_'] | [b's'] | [b'S'] | [b'j'] | [27, b'[', b'B'] => {
                    Some(KeyCmd::TempoDelta(-1.0))
                }
                [b']'] | [b'd'] | [b'D'] | [b'l'] | [27, b'[', b'C'] => {
                    Some(KeyCmd::TempoDelta(0.1))
                }
                [b'['] | [b'a'] | [b'A'] | [b'h'] | [27, b'[', b'D'] => {
                    Some(KeyCmd::TempoDelta(-0.1))
                }
                [b'r'] | [b'R'] => Some(KeyCmd::ResetBeat),
                [c @ b'1'..=b'8'] => Some(KeyCmd::SetQuantum((c - b'0') as f64)),
                _ => None,
            };
            if let Some(c) = cmd {
                let is_quit = matches!(c, KeyCmd::Quit);
                let _ = tx.send(c);
                if is_quit {
                    break;
                }
            }
        }
    });
}

fn render_smooth_bar(ratio: f64, width: usize) -> String {
    const BLOCKS: [char; 8] = [' ', '▏', '▎', '▍', '▌', '▋', '▊', '▉'];
    let clamped = ratio.clamp(0.0, 1.0);
    let total_eighths = (clamped * (width as f64) * 8.0).round() as usize;
    let full = total_eighths / 8;
    let rem = total_eighths % 8;
    let mut s = String::with_capacity(width * 4);
    for i in 0..width {
        if i < full {
            s.push('█');
        } else if i == full && rem > 0 {
            s.push(BLOCKS[rem]);
        } else {
            s.push('░');
        }
    }
    s
}

fn run_tui(
    session: SessionHandle,
    clock: Clock,
    mut quantum: f64,
    interval_ms: u64,
    ifaces: &[Ipv4Addr],
    node_id: NodeId,
) {
    let _term = RawTerminal::enter();
    let (tx, rx) = mpsc::channel();
    spawn_key_reader(tx);

    let iface_str = ifaces
        .iter()
        .map(|ip| ip.to_string())
        .collect::<Vec<_>>()
        .join(", ");
    let node_str = String::from_utf8_lossy(&node_id.0);
    let mut last_action = String::from("Ready — listening on 224.76.78.75:20808");

    loop {
        while let Ok(cmd) = rx.try_recv() {
            let now = clock.now();
            match cmd {
                KeyCmd::Quit => {
                    session.shutdown();
                    thread::sleep(Duration::from_millis(20));
                    return;
                }
                KeyCmd::TogglePlay => {
                    if let Some(mut state) = session.capture_state() {
                        let next = !state.start_stop.is_playing;
                        state.set_is_playing(next, now);
                        session.commit_state(state);
                        last_action = format!(
                            "Transport -> {}",
                            if next { "PLAYING" } else { "STOPPED" }
                        );
                    }
                }
                KeyCmd::TempoDelta(delta) => {
                    if let Some(mut state) = session.capture_state() {
                        let cur = state.timeline.tempo.to_tempo().0;
                        let next = (cur + delta).clamp(20.0, 999.0);
                        state.set_tempo(next, now);
                        session.commit_state(state);
                        last_action = format!("Tempo -> {:.2} BPM", next);
                    }
                }
                KeyCmd::SetQuantum(q) => {
                    quantum = q;
                    last_action = format!("Quantum -> {:.0} beats", quantum);
                }
                KeyCmd::ResetBeat => {
                    if let Some(mut state) = session.capture_state() {
                        state.request_beat_at_time(now, 0.0, quantum);
                        session.commit_state(state);
                        last_action = String::from("Requested beat 0.0 at current host time");
                    }
                }
            }
        }

        if let Some(state) = session.capture_state() {
            let now = clock.now();
            let beat = state.beat_at_time(now, quantum);
            let phase = state.phase_at_time(now, quantum);
            let num_peers = session.num_peers();
            let bpm = state.timeline.tempo.to_tempo().0;
            let playing = state.start_stop.is_playing;
            let bar = (beat / quantum).floor() as i64 + 1;
            let q_int = (quantum.round() as usize).clamp(1, 16);
            let cur_beat_idx = (phase.floor() as usize) % q_int;
            let sub_beat = phase.fract();

            let peer_badge = if num_peers > 0 {
                format!(
                    "\x1b[1;32m● {} PEER{} SYNCED\x1b[0m",
                    num_peers,
                    if num_peers == 1 { "" } else { "S" }
                )
            } else {
                String::from("\x1b[1;33m○ 0 PEERS (WAITING FOR LINK / CHROMIUM)\x1b[0m")
            };

            let play_badge = if playing {
                "\x1b[1;42;30m ▶ PLAYING \x1b[0m"
            } else {
                "\x1b[1;41;37m ■ STOPPED \x1b[0m"
            };

            let mut pads = String::new();
            for b in 0..q_int {
                if b == cur_beat_idx {
                    if b == 0 {
                        // Downbeat (Beat 1): bright cyan/white highlight
                        pads.push_str(&format!("\x1b[1;46;30m  BEAT {}  \x1b[0m ", b + 1));
                    } else {
                        // Other active beats: bright green highlight
                        pads.push_str(&format!("\x1b[1;42;30m  BEAT {}  \x1b[0m ", b + 1));
                    }
                } else {
                    pads.push_str(&format!("\x1b[2;37m [Beat {}] \x1b[0m ", b + 1));
                }
            }

            let bar_meter = render_smooth_bar(phase / quantum.max(1.0), 40);
            let beat_meter = render_smooth_bar(sub_beat, 40);

            let mut out = io::stdout();
            let _ = write!(
                out,
                concat!(
                    "\x1b[H",
                    "\x1b[1;36m┌──────────────────────────────────────────────────────────────────────┐\x1b[0m\x1b[K\r\n",
                    "\x1b[1;36m│\x1b[0m  \x1b[1;37mTempoHut\x1b[0m — Ableton Link / Web Tempo Sync Live Monitor               \x1b[1;36m│\x1b[0m\x1b[K\r\n",
                    "\x1b[1;36m└──────────────────────────────────────────────────────────────────────┘\x1b[0m\x1b[K\r\n",
                    "  \x1b[2mNode:\x1b[0m {:<10}  \x1b[2mInterface:\x1b[0m {:<15}  \x1b[2mMulticast:\x1b[0m 224.76.78.75:20808\x1b[K\r\n",
                    "\x1b[K\r\n",
                    "  Status:    {}    {}\x1b[K\r\n",
                    "  Tempo:     \x1b[1;33m{:>7.2} BPM\x1b[0m      Quantum: \x1b[1;37m{:.0} beats/bar\x1b[0m\x1b[K\r\n",
                    "  Timeline:  Bar \x1b[1;37m{:>4}\x1b[0m   Beat \x1b[1;37m{:>9.2}\x1b[0m   Phase \x1b[1;36m{:>5.2}\x1b[0m / {:.2}\x1b[K\r\n",
                    "\x1b[K\r\n",
                    "  {}\x1b[K\r\n",
                    "\x1b[K\r\n",
                    "  Bar Phase  [\x1b[1;36m{}\x1b[0m] {:>5.1}%\x1b[K\r\n",
                    "  Beat Pulse [\x1b[1;32m{}\x1b[0m] {:>5.1}%\x1b[K\r\n",
                    "\x1b[K\r\n",
                    "  \x1b[2m────────────────────────────────────────────────────────────────────\x1b[0m\x1b[K\r\n",
                    "  \x1b[1mControls:\x1b[0m  \x1b[36m[Space]\x1b[0m Play/Stop   \x1b[36m[↑/↓ or +/-]\x1b[0m ±1 BPM   \x1b[36m[←/→ or [/]]\x1b[0m ±0.1 BPM\x1b[K\r\n",
                    "             \x1b[36m[1-8]\x1b[0m   Quantum     \x1b[36m[r]\x1b[0m Reset Beat 0  \x1b[36m[q / Esc]\x1b[0m    Quit\x1b[K\r\n",
                    "  \x1b[2mLast Event:\x1b[0m {}\x1b[K\r\n\x1b[J"
                ),
                node_str,
                iface_str,
                play_badge,
                peer_badge,
                bpm,
                quantum,
                bar,
                beat,
                phase,
                quantum,
                pads,
                bar_meter,
                (phase / quantum.max(1.0)) * 100.0,
                beat_meter,
                sub_beat * 100.0,
                last_action
            );
            let _ = out.flush();
        }

        thread::sleep(Duration::from_millis(interval_ms));
    }
}

fn main() {
    let args: Vec<String> = env::args().collect();
    let mut is_json = false;
    let mut is_tui = false;
    let mut tempo = 120.0;
    let mut quantum = 4.0;
    let mut interval_ms: Option<u64> = None;
    let mut ifaces = Vec::new();
    let mut dump = false;

    let mut node_id = NodeId(*b"TempoHut");
    let mut explicit_node_id = false;
    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--help" | "-h" => {
                println!(
                    "Usage: tempohut [--tui] [--json] [--tempo <bpm>] [--quantum <n>] [--interval-ms <n>] [--iface <ipv4>]..."
                );
                return;
            }
            "--json" => is_json = true,
            "--tui" => is_tui = true,
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
                    if let Ok(ms) = args[i].parse::<u64>() {
                        interval_ms = Some(ms);
                    }
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
                        explicit_node_id = true;
                    }
                }
            }
            _ => {}
        }
        i += 1;
    }

    let stdout_is_tty = unsafe { libc::isatty(libc::STDOUT_FILENO) == 1 };
    let use_tui = !is_json && (is_tui || stdout_is_tty);
    if use_tui && !explicit_node_id {
        let pid = std::process::id();
        let label = format!("TH{:06X}", pid & 0xFF_FFFF);
        let mut arr = [0u8; 8];
        arr.copy_from_slice(&label.as_bytes()[..8]);
        node_id = NodeId(arr);
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

    let mut runtime = Runtime::new(node_id, ifaces.clone());
    runtime.set_dump_packets(dump);
    let session = runtime.handle();
    let clock = Clock::new();

    let mut state = session.capture_state().unwrap();
    state.set_tempo(tempo, clock.now());
    session.commit_state(state);

    thread::spawn(move || {
        runtime.run();
    });

    if use_tui {
        let tui_interval = interval_ms.unwrap_or(33);
        run_tui(session, clock, quantum, tui_interval, &ifaces, node_id);
        return;
    }

    let poll_ms = interval_ms.unwrap_or(100);
    let (tx, rx) = mpsc::channel();

    thread::spawn(move || {
        let stdin = io::stdin();
        for l in stdin.lock().lines().map_while(Result::ok) {
            let _ = tx.send(l);
        }
    });

    loop {
        match rx.recv_timeout(Duration::from_millis(poll_ms)) {
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
                        session.shutdown();
                        thread::sleep(Duration::from_millis(20));
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
                thread::sleep(Duration::from_millis(poll_ms));
            }
        }
    }
}
