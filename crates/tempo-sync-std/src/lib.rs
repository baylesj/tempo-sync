// SPDX-License-Identifier: Apache-2.0

use std::sync::{Arc, Mutex};
use std::time::{Instant, Duration as StdDuration};
use std::net::{UdpSocket, Ipv4Addr, SocketAddrV4};
use std::thread;
use std::collections::HashMap;
#[allow(unused_imports)]
use tempo_sync_core::{Engine, Output, SessionState, HostTime, NodeId, InterfaceId, SocketAddrPlain};

pub struct Clock {
    epoch: Instant,
}

impl Default for Clock {
    fn default() -> Self {
        Self::new()
    }
}

impl Clock {
    pub fn new() -> Self {
        Clock { epoch: Instant::now() }
    }

    pub fn now(&self) -> HostTime {
        let elapsed = self.epoch.elapsed().as_micros() as i64;
        HostTime(elapsed)
    }
}

#[derive(Clone)]
pub struct SessionHandle {
    state: Arc<Mutex<Option<SessionState>>>,
    num_peers: Arc<Mutex<u32>>,
    // We could use an mpsc channel to push state to the engine thread
    tx: std::sync::mpsc::Sender<SessionState>,
}

impl SessionHandle {
    pub fn capture_state(&self) -> Option<SessionState> {
        self.state.lock().unwrap().clone()
    }

    pub fn commit_state(&self, state: SessionState) {
        *self.state.lock().unwrap() = Some(state.clone());
        let _ = self.tx.send(state);
    }

    pub fn num_peers(&self) -> u32 {
        *self.num_peers.lock().unwrap()
    }
}

pub struct Runtime {
    engine: Engine,
    clock: Clock,
    state: Arc<Mutex<Option<SessionState>>>,
    num_peers: Arc<Mutex<u32>>,
    rx: std::sync::mpsc::Receiver<SessionState>,
    tx: std::sync::mpsc::Sender<SessionState>,
    ifaces: Vec<Ipv4Addr>,
}

impl Runtime {
    pub fn new(node_id: NodeId, ifaces: Vec<Ipv4Addr>) -> Self {
        let (tx, rx) = std::sync::mpsc::channel();
        let state = Arc::new(Mutex::new(Some(SessionState::default())));
        Runtime {
            engine: Engine::new(node_id),
            clock: Clock::new(),
            state,
            num_peers: Arc::new(Mutex::new(0)),
            rx,
            tx,
            ifaces,
        }
    }

    pub fn handle(&self) -> SessionHandle {
        SessionHandle {
            state: self.state.clone(),
            num_peers: self.num_peers.clone(),
            tx: self.tx.clone(),
        }
    }

    pub fn run(mut self) {
        let mut sockets: HashMap<InterfaceId, UdpSocket> = HashMap::new();
        let mut multicast_sockets: HashMap<InterfaceId, UdpSocket> = HashMap::new();
        
        let now = self.clock.now();
        let mut next_timer = None;
        let mut outputs = self.engine.set_enabled(true, now);
        
        for (i, &ip) in self.ifaces.iter().enumerate() {
            let id = InterfaceId(i as u32);
            outputs.extend(self.engine.add_interface(id, ip.octets()));
        }

        loop {
            // Process outputs
            for out in outputs.drain(..) {
                match out {
                    Output::Send { iface_id, to, bytes } => {
                        let to_addr = SocketAddrV4::new(Ipv4Addr::from(to.ip), to.port);
                        
                        // Wait, multicast or unicast?
                        // If it's 224.x.x.x, we should send it via the unicast socket for that interface which is bound to the interface IP.
                        if let Some(sock) = sockets.get(&iface_id) {
                            let _ = sock.send_to(&bytes, to_addr);
                        }
                    }
                    Output::JoinMulticast { iface_id, group, port } => {
                        let ip = self.ifaces[iface_id.0 as usize];
                        if let Ok(sock) = UdpSocket::bind(SocketAddrV4::new(ip, port)) {
                            let group_ip = Ipv4Addr::from(group);
                            let _ = sock.join_multicast_v4(&group_ip, &ip);
                            sock.set_multicast_loop_v4(true).unwrap();
                            sock.set_nonblocking(true).unwrap();
                            multicast_sockets.insert(iface_id, sock);
                        }
                    }
                    Output::LeaveMulticast => {}
                    Output::OpenUnicast { iface_id } => {
                        let ip = self.ifaces[iface_id.0 as usize];
                        // Let OS assign port
                        if let Ok(sock) = UdpSocket::bind(SocketAddrV4::new(ip, 0)) {
                            sock.set_nonblocking(true).unwrap();
                            sockets.insert(iface_id, sock);
                        }
                    }
                    Output::SetTimer { at } => {
                        next_timer = Some(at);
                    }
                    Output::StateChanged { state } => {
                        *self.state.lock().unwrap() = Some(state);
                    }
                    Output::NumPeersChanged(n) => {
                        *self.num_peers.lock().unwrap() = n;
                    }
                }
            }

            // Sleep until next timer or short interval (10ms)
            let mut sleep_time = StdDuration::from_millis(10);
            let now = self.clock.now();
            if let Some(t) = next_timer {
                let diff = t.0 - now.0;
                if diff > 0 {
                    let diff_ms = (diff / 1000) as u64;
                    if diff_ms < 10 {
                        sleep_time = StdDuration::from_millis(diff_ms);
                    }
                } else {
                    sleep_time = StdDuration::from_millis(0);
                }
            }
            thread::sleep(sleep_time);

            let now = self.clock.now();
            
            // App state changes
            while let Ok(state) = self.rx.try_recv() {
                outputs.extend(self.engine.commit_app_state(state, now));
            }

            // Read sockets
            let mut buf = [0u8; 2048];
            for (&id, sock) in sockets.iter() {
                while let Ok((len, from)) = sock.recv_from(&mut buf) {
                    if let std::net::SocketAddr::V4(v4) = from {
                        let plain = SocketAddrPlain { ip: v4.ip().octets(), port: v4.port() };
                        outputs.extend(self.engine.handle_packet(id, plain, &buf[..len], now));
                    }
                }
            }
            for (&id, sock) in multicast_sockets.iter() {
                while let Ok((len, from)) = sock.recv_from(&mut buf) {
                    if let std::net::SocketAddr::V4(v4) = from {
                        let plain = SocketAddrPlain { ip: v4.ip().octets(), port: v4.port() };
                        outputs.extend(self.engine.handle_packet(id, plain, &buf[..len], now));
                    }
                }
            }
            
            // Timer
            if let Some(t) = next_timer {
                if now.0 >= t.0 {
                    outputs.extend(self.engine.handle_timer(now));
                    next_timer = None;
                }
            }
        }
    }
}
