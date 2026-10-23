// SPDX-License-Identifier: Apache-2.0

use std::collections::HashMap;
use crate::time::HostTime;
use crate::timeline::{SessionState, Timeline, StartStopState};
use crate::wire::{Message, DiscoveryMsg, MeasurementMsg, NodeId, serialize_discovery_alive, serialize_discovery_byebye, serialize_measurement_ping, serialize_measurement_pong};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct InterfaceId(pub u32);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]

#[derive(Default)]
pub struct SocketAddrPlain {
    pub ip: [u8; 4],
    pub port: u16,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum IoErrorKind {
    JoinMulticastFailed,
    OpenUnicastFailed,
    SendFailed,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Output {
    Send {
        iface_id: InterfaceId,
        to: SocketAddrPlain,
        bytes: std::vec::Vec<u8>,
    },
    JoinMulticast {
        iface_id: InterfaceId,
        group: [u8; 4],
        port: u16,
    },
    LeaveMulticast,
    OpenUnicast {
        iface_id: InterfaceId,
    },
    SetTimer {
        at: HostTime,
    },
    StateChanged {
        state: SessionState,
    },
    NumPeersChanged(u32),
}

pub struct PeerState {
    pub session_id: Option<NodeId>,
    pub timeline_ghost: Option<Timeline>,
    pub start_stop_ghost: Option<StartStopState>,
    pub last_seen: HostTime,
    pub addr: SocketAddrPlain,
    
    pub offsets: Vec<i64>, // Up to 100 samples
    pub next_ping_time: HostTime,
    pub ping_timeouts: u32,
    pub measuring: bool,
}

impl PeerState {
    pub fn new(addr: SocketAddrPlain, now: HostTime) -> Self {
        Self {
            session_id: None,
            timeline_ghost: None,
            start_stop_ghost: None,
            last_seen: now,
            addr,
            offsets: Vec::new(),
            next_ping_time: now,
            ping_timeouts: 0,
            measuring: true,
        }
    }
    
    pub fn median_offset(&self) -> i64 {
        if self.offsets.is_empty() { return 0; }
        let mut sorted = self.offsets.clone();
        sorted.sort_unstable();
        sorted[sorted.len() / 2]
    }
}

pub struct Engine {
    pub node_id: NodeId,
    pub enabled: bool,
    pub session_id: NodeId,
    pub session_offset: i64,
    pub state: SessionState, // In HostTime
    
    pub interfaces: HashMap<InterfaceId, SocketAddrPlain>,
    pub peers: HashMap<NodeId, PeerState>,
    pub dump_packets: bool,
    pub num_session_peers: u32,
    
    pub next_broadcast: HostTime,
}

impl Engine {
    pub fn new(node_id: NodeId) -> Self {
        Engine {
            node_id,
            enabled: false,
            session_id: node_id,
            session_offset: 0,
            state: SessionState::default(),
            interfaces: HashMap::new(),
            peers: HashMap::new(),
            num_session_peers: 0,
            next_broadcast: HostTime(0),
            dump_packets: false,
        }
    }

    fn produce_outputs(&mut self, now: HostTime, outputs: &mut Vec<Output>) {
        if !self.enabled { return; }
        
        let mut next_timer = HostTime(i64::MAX);

        // Discovery broadcast
        if now.0 >= self.next_broadcast.0 {
            // Serialize in Ghost Time
            let mut ghost_tl = self.state.timeline.clone();
            ghost_tl.time_origin = HostTime(ghost_tl.time_origin.0.wrapping_add(self.session_offset));
            let mut ghost_ss = self.state.start_stop.clone();
            ghost_ss.time = HostTime(ghost_ss.time.0.wrapping_add(self.session_offset));

            for (&iface_id, mep4) in &self.interfaces {
                let msg = serialize_discovery_alive(&self.node_id, &self.session_id, &ghost_tl, &ghost_ss, Some(*mep4), false);
                if self.dump_packets { eprintln!("SEND MULTICAST ALIVE: {:x?}", msg); }
                outputs.push(Output::Send {
                    iface_id,
                    to: SocketAddrPlain { ip: [224, 76, 78, 75], port: 20808 },
                    bytes: msg,
                });
            }
            // "A hard floor of 50ms is enforced... nominal interval is 250ms"
            self.next_broadcast = HostTime(now.0 + 250_000); 
        }
        if self.next_broadcast.0 < next_timer.0 {
            next_timer = self.next_broadcast;
        }

        // Pings
        for (_peer_id, peer) in self.peers.iter_mut() {
            if peer.measuring && now.0 >= peer.next_ping_time.0 {
                if peer.ping_timeouts >= 5 {
                    peer.measuring = false;
                    continue;
                }
                peer.ping_timeouts += 1;
                
                let ping = serialize_measurement_ping(now, None);
                if self.dump_packets { eprintln!("SEND PING (INIT): {:x?}", ping); }
                for &iface_id in self.interfaces.keys() {
                    outputs.push(Output::Send {
                        iface_id,
                        to: peer.addr,
                        bytes: ping.clone(),
                    });
                }
                peer.next_ping_time = HostTime(now.0 + 50_000); // 50ms timeout
            }
            if peer.measuring && peer.next_ping_time.0 < next_timer.0 {
                next_timer = peer.next_ping_time;
            }
        }
        
        // Peer timeouts (6s)
        let timeout = HostTime(now.0 - 6_000_000);
        let old_len = self.peers.len();
        self.peers.retain(|_, p| p.last_seen.0 >= timeout.0);
        if self.peers.len() != old_len {
            self.resolve_session(now, outputs);
        }
        
        // Wakeup for next timeout
        let mut min_last_seen = HostTime(i64::MAX);
        for peer in self.peers.values() {
            if peer.last_seen.0 < min_last_seen.0 {
                min_last_seen = peer.last_seen;
            }
        }
        if min_last_seen.0 != i64::MAX {
            let next_timeout = HostTime(min_last_seen.0 + 6_000_000);
            if next_timeout.0 < next_timer.0 {
                next_timer = next_timeout;
            }
        }

        if next_timer.0 != i64::MAX {
            outputs.push(Output::SetTimer { at: next_timer });
        }
        
        let mut session_peers = 0;
        for peer in self.peers.values() {
            if peer.session_id == Some(self.session_id) {
                session_peers += 1;
            }
        }
        if session_peers != self.num_session_peers {
            self.num_session_peers = session_peers;
            outputs.push(Output::NumPeersChanged(session_peers));
        }
    }

    pub fn handle_packet(&mut self, iface_id: InterfaceId, from: SocketAddrPlain, bytes: &[u8], now: HostTime) -> Vec<Output> {
        let mut outputs = Vec::new();
        if !self.enabled { return outputs; }
        if self.dump_packets { eprintln!("RECV MSG: {:x?}", bytes); }
        if let Some(msg) = crate::wire::parse_message(bytes) {
            match msg {
                Message::Discovery(DiscoveryMsg::Alive { node_id, timeline, start_stop, session_id, mep4 }) => {
                    if node_id == self.node_id { return outputs; } // Ignore self
                    
                    let is_new = !self.peers.contains_key(&node_id);
                    let mut needs_resolution = false;

                    let peer = self.peers.entry(node_id).or_insert_with(|| PeerState::new(from, now));
                    peer.last_seen = now;
                    peer.addr = mep4.unwrap_or(from);

                    let session_changed = peer.session_id != session_id;
                    if session_changed {
                        peer.offsets.clear();
                        peer.measuring = true;
                        peer.ping_timeouts = 0;
                        peer.next_ping_time = now;
                    }

                    if peer.timeline_ghost != timeline || peer.start_stop_ghost != start_stop || session_changed {
                        peer.timeline_ghost = timeline;
                        peer.start_stop_ghost = start_stop;
                        peer.session_id = session_id;
                        needs_resolution = !peer.offsets.is_empty();
                        
                        // Send Response
                        let mut ghost_tl = self.state.timeline.clone();
                        ghost_tl.time_origin = HostTime(ghost_tl.time_origin.0.wrapping_add(self.session_offset));
                        let mut ghost_ss = self.state.start_stop.clone();
                        ghost_ss.time = HostTime(ghost_ss.time.0.wrapping_add(self.session_offset));
                        let mep4 = self.interfaces.get(&iface_id).copied();
                        let resp = serialize_discovery_alive(&self.node_id, &self.session_id, &ghost_tl, &ghost_ss, mep4, true);
                        if self.dump_packets { eprintln!("SEND RESPONSE ALIVE: {:x?}", resp); }
                        outputs.push(Output::Send {
                            iface_id,
                            to: peer.addr,
                            bytes: resp,
                        });
                    }
                    
                    if is_new {
                    }
                    if needs_resolution {
                        self.resolve_session(now, &mut outputs);
                    }
                }
                Message::Discovery(DiscoveryMsg::ByeBye { node_id }) => {
                    if self.peers.remove(&node_id).is_some() {
                        self.resolve_session(now, &mut outputs);
                    }
                }
                Message::Measurement(MeasurementMsg::Ping { host_time: _, prev_ghost_time: _ }) => {
                    // Pong: `__gt` is our GhostTime
                    let our_ghost_time = HostTime(now.0.wrapping_add(self.session_offset));
                    let pong = serialize_measurement_pong(&self.session_id, our_ghost_time, &bytes[9..]);
                    if self.dump_packets { eprintln!("SEND PONG: {:x?}", pong); }
                    outputs.push(Output::Send {
                        iface_id,
                        to: from,
                        bytes: pong,
                    });
                }
                Message::Measurement(MeasurementMsg::Pong { session_id, ghost_time: b_t2, ping_host_time: a_t2, ping_prev_ghost_time: prev_ghost_time }) => {
                    {
                        let a_t3 = now;
                        let offset = if let Some(b_t1) = prev_ghost_time {
                            let off1 = b_t2.0.wrapping_sub((a_t3.0.wrapping_add(a_t2.0)) / 2);
                            let off2 = (b_t2.0.wrapping_add(b_t1.0)) / 2 - a_t2.0;
                            (off1 + off2) / 2
                        } else {
                            b_t2.0.wrapping_sub((a_t3.0.wrapping_add(a_t2.0)) / 2)
                        };
                        
                        let mut found_node_id = None;
                        for (id, peer) in &self.peers {
                            if peer.addr == from { found_node_id = Some(*id); break; }
                        }
                        
                        if let Some(id) = found_node_id {
                            let peer = self.peers.get_mut(&id).unwrap();
                            if let Some(pong_sess) = session_id {
                                if peer.session_id != Some(pong_sess) {
                                    peer.session_id = Some(pong_sess);
                                    peer.offsets.clear();
                                }
                            }
                            if peer.measuring {
                                peer.offsets.push(offset);
                                peer.ping_timeouts = 0;
                                if peer.offsets.len() >= 20 {
                                    peer.measuring = false;
                                } else {
                                    // Send immediate follow-up
                                    let ping = serialize_measurement_ping(now, Some(b_t2));
                                    if self.dump_packets { eprintln!("SEND PING: {:x?}", ping); }
                                    outputs.push(Output::Send {
                                        iface_id,
                                        to: peer.addr,
                                        bytes: ping,
                                    });
                                    peer.next_ping_time = HostTime(now.0 + 50_000);
                                }
                                if peer.offsets.len() == 1 || !peer.measuring {
                                    self.resolve_session(now, &mut outputs);
                                }
                            }
                        }
                    }
                }
            }
        }
        
        self.produce_outputs(now, &mut outputs);
        outputs
    }

    pub fn handle_timer(&mut self, now: HostTime) -> Vec<Output> {
        let mut outputs = Vec::new();
        self.produce_outputs(now, &mut outputs);
        outputs
    }

    pub fn commit_app_state(&mut self, mut state: SessionState, now: HostTime) -> Vec<Output> {
        let mut outputs = Vec::new();
        if self.state != state {
            if state.timeline != self.state.timeline {
                let mut max_bo = self.state.timeline.beat_origin.0;
                for peer in self.peers.values() {
                    if peer.session_id == Some(self.session_id) {
                        if let Some(ref tl) = peer.timeline_ghost {
                            if tl.beat_origin.0 > max_bo {
                                max_bo = tl.beat_origin.0;
                            }
                        }
                    }
                }
                if state.timeline.beat_origin.0 <= max_bo {
                    let new_bo = max_bo + 1_000_000;
                    let delta_mb = new_bo - state.timeline.beat_origin.0;
                    let delta_us = ((delta_mb as i128 * state.timeline.tempo.0 as i128) / 1_000_000) as i64;
                    state.timeline.beat_origin = crate::time::MicroBeats(new_bo);
                    state.timeline.time_origin = HostTime(state.timeline.time_origin.0.wrapping_add(delta_us));
                }
            }
            if state.start_stop != self.state.start_stop {
                let mut max_ss_ghost = self.state.start_stop.time.0.wrapping_add(self.session_offset);
                for peer in self.peers.values() {
                    if peer.session_id == Some(self.session_id) {
                        if let Some(ref ss) = peer.start_stop_ghost {
                            if ss.time.0 > max_ss_ghost {
                                max_ss_ghost = ss.time.0;
                            }
                        }
                    }
                }
                let committed_ss_ghost = state.start_stop.time.0.wrapping_add(self.session_offset);
                if committed_ss_ghost <= max_ss_ghost {
                    state.start_stop.time = HostTime((max_ss_ghost + 1).wrapping_sub(self.session_offset));
                }
            }
            self.state = state;
            outputs.push(Output::StateChanged { state: self.state.clone() });
            
            if self.enabled {
                self.next_broadcast = now;
                self.produce_outputs(now, &mut outputs);
            }
        }
        outputs
    }

    pub fn set_enabled(&mut self, enabled: bool, now: HostTime) -> Vec<Output> {
        let mut outputs = Vec::new();
        if self.enabled != enabled {
            self.enabled = enabled;
            if enabled {
                if self.peers.is_empty() {
                    self.session_id = self.node_id;
                    self.session_offset = -now.0;
                }
                if self.state.timeline.time_origin.0 == 0 {
                    self.state.timeline.time_origin = now;
                    outputs.push(Output::StateChanged { state: self.state.clone() });
                }
                self.next_broadcast = now;
                self.produce_outputs(now, &mut outputs);
            } else {
                let msg = serialize_discovery_byebye(&self.node_id);
                for &iface_id in self.interfaces.keys() {
                    outputs.push(Output::Send {
                        iface_id,
                        to: SocketAddrPlain { ip: [224, 76, 78, 75], port: 20808 },
                        bytes: msg.clone(),
                    });
                }
                self.peers.clear();
            }
        }
        outputs
    }

    pub fn add_interface(&mut self, iface_id: InterfaceId, local_addr: SocketAddrPlain) -> Vec<Output> {
        let outputs = Vec::new();
        self.interfaces.insert(iface_id, local_addr);
        // Note: join multicast logic handled outside, engine shouldn't emit OpenUnicast if it already receives the port.
        outputs
    }

    pub fn remove_interface(&mut self, iface_id: InterfaceId) -> Vec<Output> {
        self.interfaces.remove(&iface_id);
        Vec::new()
    }
    
    pub fn handle_io_error(&mut self, iface_id: InterfaceId, _error: IoErrorKind, _now: HostTime) -> Vec<Output> {
        self.interfaces.remove(&iface_id);
        Vec::new()
    }

    fn resolve_session(&mut self, now: HostTime, outputs: &mut Vec<Output>) {
        let mut best_session = self.session_id;
        let mut best_offset = self.session_offset;
        let mut our_ghost_time = now.0.wrapping_add(self.session_offset);
        
        let mut changed = false;

        for (_id, peer) in &self.peers {
            if peer.offsets.is_empty() {
                continue;
            }
            if let Some(peer_sess) = peer.session_id {
                let peer_offset = peer.median_offset();
                let peer_ghost_time = now.0.wrapping_add(peer_offset);
                
                if peer_sess == best_session {
                    if best_session != self.node_id && (best_offset - peer_offset).abs() > 500 {
                        best_offset = peer_offset;
                        our_ghost_time = peer_ghost_time;
                    }
                    continue;
                }

                // Compare sessions
                let diff = peer_ghost_time.wrapping_sub(our_ghost_time);
                let older = diff > 500_000;
                let within_eps = diff.abs() <= 500_000;
                
                let win = if older {
                    true
                } else if within_eps && Self::compare_session_ids(&peer_sess, &best_session) == std::cmp::Ordering::Less {
                    true // Lexicographically smaller wins
                } else {
                    false
                };
                
                if win {
                    best_session = peer_sess;
                    best_offset = peer_offset;
                    our_ghost_time = peer_ghost_time;
                }
            }
        }

        let mut changed_session = false;
        if best_session != self.session_id {
            self.session_id = best_session;
            self.session_offset = best_offset;
            changed_session = true;
            changed = true;
        } else if self.session_offset != best_offset {
            self.session_offset = best_offset;
            changed = true;
        }
        
        // Find best timeline in current session
        let mut best_tl = if changed_session {
            None
        } else {
            let mut tl = self.state.timeline.clone();
            tl.time_origin = HostTime(tl.time_origin.0.wrapping_add(self.session_offset));
            Some(tl)
        };
        
        for peer in self.peers.values() {
            if peer.session_id == Some(self.session_id) {
                if let Some(ref tl) = peer.timeline_ghost {
                    match best_tl {
                        Some(ref b_tl) if tl.beat_origin.0 <= b_tl.beat_origin.0 => {}
                        _ => best_tl = Some(tl.clone()),
                    }
                }
            }
        }
        
        if let Some(tl) = best_tl {
            let mut best_tl_host = tl;
            best_tl_host.time_origin = HostTime(best_tl_host.time_origin.0.wrapping_sub(self.session_offset));
            if best_tl_host != self.state.timeline {
                self.state.timeline = best_tl_host;
                changed = true;
            }
        }

        // StartStopState resolution
        let mut best_ss = if changed_session {
            None
        } else {
            let mut ss = self.state.start_stop.clone();
            ss.time = HostTime(ss.time.0.wrapping_add(self.session_offset));
            Some(ss)
        };
        
        for peer in self.peers.values() {
            if peer.session_id == Some(self.session_id) {
                if let Some(ref ss) = peer.start_stop_ghost {
                    match best_ss {
                        Some(ref b_ss) if ss.time.0 <= b_ss.time.0 => {}
                        _ => best_ss = Some(ss.clone()),
                    }
                }
            }
        }
        
        if let Some(ss) = best_ss {
            let mut best_ss_host = ss;
            best_ss_host.time = HostTime(best_ss_host.time.0.wrapping_sub(self.session_offset));
            if best_ss_host != self.state.start_stop {
                self.state.start_stop = best_ss_host;
                changed = true;
            }
        }

        if changed {
            outputs.push(Output::StateChanged { state: self.state.clone() });
            self.next_broadcast = now; // Broadcast ASAP
        }
    }
    
    fn compare_session_ids(a: &NodeId, b: &NodeId) -> std::cmp::Ordering {
        a.0.cmp(&b.0)
    }
}
