// SPDX-License-Identifier: Apache-2.0

use std::sync::{Arc, Mutex};
use std::time::{Instant, Duration as StdDuration};
use std::net::{UdpSocket, Ipv4Addr, SocketAddrV4, SocketAddr};
use std::thread;
use std::collections::HashMap;
use socket2::{Socket, Domain, Type, Protocol};
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

    pub fn set_dump_packets(&mut self, dump: bool) { self.engine.dump_packets = dump; }

    pub fn run(mut self) {
        let mut sockets: HashMap<InterfaceId, UdpSocket> = HashMap::new();
        
        let now = self.clock.now();
        let mut next_timer = None;
        let mut outputs = self.engine.set_enabled(true, now);
        
        // 1. Create the shared multicast receive socket using socket2
        let multi_sock = Socket::new(Domain::IPV4, Type::DGRAM, Some(Protocol::UDP)).unwrap();
        multi_sock.set_reuse_address(true).unwrap();
        multi_sock.set_reuse_port(true).unwrap();
        
        let addr: SocketAddr = "0.0.0.0:20808".parse().unwrap();
        multi_sock.bind(&addr.into()).unwrap();
        multi_sock.set_nonblocking(true).unwrap();
        
        let group_ip = Ipv4Addr::new(224, 76, 78, 75);
        for &ip in &self.ifaces {
            let _ = multi_sock.join_multicast_v4(&group_ip, &ip);
        }
        multi_sock.set_multicast_loop_v4(true).unwrap();

        let multi_sock_std: UdpSocket = multi_sock.into();
        
        let mcast_send_sock = UdpSocket::bind("0.0.0.0:0").unwrap();
        let mcast_send_sock2 = socket2::SockRef::from(&mcast_send_sock);
        mcast_send_sock2.set_multicast_loop_v4(true).unwrap();
        mcast_send_sock2.set_reuse_address(true).unwrap();

        for (i, &ip) in self.ifaces.iter().enumerate() {
            let id = InterfaceId(i as u32);
            // 2. Unicast measurement socket bound to an ephemeral port per interface, advertised in mep4.
            if let Ok(sock) = UdpSocket::bind(SocketAddrV4::new(ip, 0)) {
                sock.set_nonblocking(true).unwrap();
                let sock2 = socket2::SockRef::from(&sock);
                sock2.set_multicast_loop_v4(true).unwrap();
                let local_addr = sock.local_addr().unwrap();
                let port = local_addr.port();
                sockets.insert(id, sock);
                
                let mep4 = SocketAddrPlain {
                    ip: ip.octets(),
                    port,
                };
                outputs.extend(self.engine.add_interface(id, mep4));
            }
        }

        loop {
            // Process outputs
            for out in outputs.drain(..) {
                match out {
                    Output::Send { iface_id, to, bytes } => {
                        let to_addr = SocketAddrV4::new(Ipv4Addr::from(to.ip), to.port);
                        if to.ip == [224, 76, 78, 75] {
                            // Send multicast through the specific interface's socket
                            let ip = self.ifaces[iface_id.0 as usize];
                            mcast_send_sock2.set_multicast_if_v4(&ip).unwrap();
                            mcast_send_sock.set_multicast_ttl_v4(5).unwrap();
                            println!("TX to {}: {}", to_addr, bytes.iter().map(|b| format!("{:02X}", b)).collect::<String>());
                            let _ = mcast_send_sock.send_to(&bytes, to_addr);
                        } else {
                            // Unicast
                            if let Some(sock) = sockets.get(&iface_id) {
                                println!("TX to {}: {}", to_addr, bytes.iter().map(|b| format!("{:02X}", b)).collect::<String>()); let _ = sock.send_to(&bytes, to_addr);
                            }
                        }
                    }
                    Output::JoinMulticast { .. } => {}
                    Output::LeaveMulticast => {}
                    Output::OpenUnicast { .. } => {}
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
            
            while let Ok(state) = self.rx.try_recv() {
                outputs.extend(self.engine.commit_app_state(state, now));
            }

            let mut buf = [0u8; 2048];
            for (&id, sock) in sockets.iter() {
                while let Ok((len, from)) = sock.recv_from(&mut buf) {
                    println!("RX from {}: {}", from, buf[..len].iter().map(|b| format!("{:02X}", b)).collect::<String>());
                    if let std::net::SocketAddr::V4(v4) = from {
                        let plain = SocketAddrPlain { ip: v4.ip().octets(), port: v4.port() };
                        outputs.extend(self.engine.handle_packet(id, plain, &buf[..len], now));
                    }
                }
            }
            // Read from shared multicast socket
            // Pass interface ID 0 as a dummy, or attempt to find the right one.
            // Since we bound 0.0.0.0, we just process it.
            while let Ok((len, from)) = multi_sock_std.recv_from(&mut buf) {
                println!("RX from {}: {}", from, buf[..len].iter().map(|b| format!("{:02X}", b)).collect::<String>());
                if let std::net::SocketAddr::V4(v4) = from {
                    let plain = SocketAddrPlain { ip: v4.ip().octets(), port: v4.port() };
                    // Default to InterfaceId(0) for incoming multicasts
                    outputs.extend(self.engine.handle_packet(InterfaceId(0), plain, &buf[..len], now));
                }
            }
            
            if let Some(t) = next_timer {
                if now.0 >= t.0 {
                    outputs.extend(self.engine.handle_timer(now));
                    next_timer = None;
                }
            }
        }
    }
}
