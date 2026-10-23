// SPDX-License-Identifier: Apache-2.0

use std::collections::{HashMap, VecDeque};
use tempo_sync_core::{Engine, Output, InterfaceId, SocketAddrPlain, HostTime, NodeId};

pub struct Simulator {
    pub engines: HashMap<NodeId, Engine>,
    pub time: i64,
    pub in_flight: VecDeque<InFlightMessage>,
    pub drop_rate: f64,
    pub latency: i64,
    pub partition: bool,
}

#[derive(Clone)]
pub struct InFlightMessage {
    pub delivery_time: i64,
    pub from: NodeId,
    pub to_addr: SocketAddrPlain,
    pub bytes: std::vec::Vec<u8>,
}

impl Default for Simulator {
    fn default() -> Self {
        Self::new()
    }
}

impl Simulator {
    pub fn new() -> Self {
        Simulator {
            engines: HashMap::new(),
            time: 0,
            in_flight: VecDeque::new(),
            drop_rate: 0.0,
            latency: 10_000,
            partition: false,
        }
    }

    pub fn add_node(&mut self, node_id: NodeId, addr: SocketAddrPlain) {
        let mut engine = Engine::new(node_id);
        let iface = InterfaceId(1);
        let outputs = engine.add_interface(iface, addr);
        self.handle_outputs(node_id, outputs);
        let outputs = engine.set_enabled(true, HostTime(self.time));
        self.handle_outputs(node_id, outputs);
        self.engines.insert(node_id, engine);
    }

    pub fn step(&mut self, dt: i64) {
        let target_time = self.time + dt;
        
        while self.time < target_time {
            let mut next_event = target_time;
            for msg in &self.in_flight {
                if msg.delivery_time < next_event {
                    next_event = msg.delivery_time;
                }
            }
            // Trigger timers (check min timer across engines)
            // In a real simulator, we'd query next_timer. Here we just step every 1ms.
            if self.time + 1000 < next_event {
                next_event = self.time + 1000;
            }
            
            self.time = next_event;

            let mut to_deliver = Vec::new();
            self.in_flight.retain(|msg| {
                if msg.delivery_time <= self.time {
                    to_deliver.push(msg.clone());
                    false
                } else {
                    true
                }
            });

            for msg in to_deliver {
                let mut all_outputs = Vec::new();
                for (target_id, target_engine) in self.engines.iter_mut() {
                    if self.partition && ((msg.from.0[0] < 5 && target_id.0[0] >= 5) || (msg.from.0[0] >= 5 && target_id.0[0] < 5)) {
                        continue;
                    }
                    
                    let mut receive = false;
                    if msg.to_addr.ip == [224, 76, 78, 75] {
                        receive = true;
                    } else if let Some(ip) = target_engine.interfaces.get(&InterfaceId(1)) {
                        if ip.ip == msg.to_addr.ip {
                            receive = true;
                        }
                    }

                    if receive {
                        let from_addr = SocketAddrPlain { ip: [msg.from.0[0], 0, 0, 0], port: 20808 };
                        let out = target_engine.handle_packet(InterfaceId(1), from_addr, &msg.bytes, HostTime(self.time));
                        all_outputs.push((*target_id, out));
                    }
                }
                for (id, out) in all_outputs {
                    self.handle_outputs(id, out);
                }
            }
            
            let mut all_timer_outputs = Vec::new();
            for (node_id, engine) in self.engines.iter_mut() {
                let out = engine.handle_timer(HostTime(self.time));
                all_timer_outputs.push((*node_id, out));
            }
            for (id, out) in all_timer_outputs {
                self.handle_outputs(id, out);
            }
        }
    }

    fn handle_outputs(&mut self, from: NodeId, outputs: Vec<Output>) {
        for out in outputs {
            if let Output::Send { to, bytes, .. } = out {
                self.in_flight.push_back(InFlightMessage {
                    delivery_time: self.time + self.latency,
                    from,
                    to_addr: to,
                    bytes,
                });
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_simulation_runs() {
        let mut sim = Simulator::new();
        sim.add_node(NodeId([1,0,0,0,0,0,0,0]), SocketAddrPlain { ip: [1,0,0,0], port: 20808 });
        sim.add_node(NodeId([2,0,0,0,0,0,0,0]), SocketAddrPlain { ip: [2,0,0,0], port: 20808 });
        sim.step(1_000_000);
        assert_eq!(sim.time, 1_000_000);
    }
}

#[cfg(test)]
mod sim_tests {
    use super::*;

    #[test]
    fn test_convergence() {
        let mut sim = Simulator::new();
        sim.latency = 10_000;
        sim.add_node(NodeId([1,0,0,0,0,0,0,0]), SocketAddrPlain { ip: [1,0,0,0], port: 20808 });
        sim.add_node(NodeId([2,0,0,0,0,0,0,0]), SocketAddrPlain { ip: [2,0,0,0], port: 20808 });
        sim.add_node(NodeId([3,0,0,0,0,0,0,0]), SocketAddrPlain { ip: [3,0,0,0], port: 20808 });
        
        sim.step(10_000_000); // 10 seconds
        
        // Assert convergence
        let mut state_iter = sim.engines.values();
        let first = state_iter.next().unwrap();
        let target_beat = first.state.beat_at_time(HostTime(sim.time), 4.0);
        let target_tempo = first.state.timeline.tempo.to_tempo().0;

        for engine in state_iter {
            let beat = engine.state.beat_at_time(HostTime(sim.time), 4.0);
            let tempo = engine.state.timeline.tempo.to_tempo().0;
            assert!((tempo - target_tempo).abs() < 0.1);
            // In a real network with latency, the beats should be very close after GhostXForm
            assert!((beat - target_beat).abs() < 0.1);
        }
    }
}
