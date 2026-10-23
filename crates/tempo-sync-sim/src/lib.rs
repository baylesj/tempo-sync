// SPDX-License-Identifier: Apache-2.0

use std::collections::{HashMap, VecDeque};
use tempo_sync_core::{Engine, HostTime, InterfaceId, NodeId, Output, SocketAddrPlain};

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
                    if self.partition
                        && ((msg.from.0[0] < 5 && target_id.0[0] >= 5)
                            || (msg.from.0[0] >= 5 && target_id.0[0] < 5))
                    {
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
                        let from_addr = SocketAddrPlain {
                            ip: [msg.from.0[0], 0, 0, 0],
                            port: 20808,
                        };
                        let out = target_engine.handle_packet(
                            InterfaceId(1),
                            from_addr,
                            &msg.bytes,
                            HostTime(self.time),
                        );
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
        sim.add_node(
            NodeId([1, 0, 0, 0, 0, 0, 0, 0]),
            SocketAddrPlain {
                ip: [1, 0, 0, 0],
                port: 20808,
            },
        );
        sim.add_node(
            NodeId([2, 0, 0, 0, 0, 0, 0, 0]),
            SocketAddrPlain {
                ip: [2, 0, 0, 0],
                port: 20808,
            },
        );
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
        sim.add_node(
            NodeId([1, 0, 0, 0, 0, 0, 0, 0]),
            SocketAddrPlain {
                ip: [1, 0, 0, 0],
                port: 20808,
            },
        );
        sim.add_node(
            NodeId([2, 0, 0, 0, 0, 0, 0, 0]),
            SocketAddrPlain {
                ip: [2, 0, 0, 0],
                port: 20808,
            },
        );
        sim.add_node(
            NodeId([3, 0, 0, 0, 0, 0, 0, 0]),
            SocketAddrPlain {
                ip: [3, 0, 0, 0],
                port: 20808,
            },
        );

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

    #[test]
    fn test_tie_break_adoption() {
        // Test ordering where ID1 < ID2
        let mut sim = Simulator::new();
        sim.latency = 1_000;

        let id1 = NodeId([1, 0, 0, 0, 0, 0, 0, 0]);
        let id2 = NodeId([2, 0, 0, 0, 0, 0, 0, 0]);

        // Add ID1 but start it with 90
        let mut engine1 = Engine::new(id1);
        engine1.state.set_tempo(90.0, HostTime(0));
        let out1 = engine1.add_interface(
            InterfaceId(1),
            SocketAddrPlain {
                ip: [1, 0, 0, 0],
                port: 20808,
            },
        );
        sim.handle_outputs(id1, out1);
        let out1b = engine1.set_enabled(true, HostTime(0));
        sim.handle_outputs(id1, out1b);
        sim.engines.insert(id1, engine1);

        // Add ID2 but start it with 128
        let mut engine2 = Engine::new(id2);
        engine2.state.set_tempo(128.0, HostTime(0));
        let out2 = engine2.add_interface(
            InterfaceId(1),
            SocketAddrPlain {
                ip: [2, 0, 0, 0],
                port: 20808,
            },
        );
        sim.handle_outputs(id2, out2);
        let out2b = engine2.set_enabled(true, HostTime(0));
        sim.handle_outputs(id2, out2b);
        sim.engines.insert(id2, engine2);

        sim.step(2_000_000); // 2 sec

        let t1 = sim
            .engines
            .get(&id1)
            .unwrap()
            .state
            .timeline
            .tempo
            .to_tempo()
            .0;
        let t2 = sim
            .engines
            .get(&id2)
            .unwrap()
            .state
            .timeline
            .tempo
            .to_tempo()
            .0;
        assert!((t1 - 90.0).abs() < 0.1, "t1: {}", t1);
        assert!((t2 - 90.0).abs() < 0.1, "t2: {}", t2);

        // Test ordering where ID1 > ID2
        let mut sim2 = Simulator::new();
        sim2.latency = 1_000;

        let id3 = NodeId([3, 0, 0, 0, 0, 0, 0, 0]);
        let id4 = NodeId([2, 0, 0, 0, 0, 0, 0, 0]); // smaller!

        let mut engine3 = Engine::new(id3);
        engine3.state.set_tempo(90.0, HostTime(0));
        let out3 = engine3.add_interface(
            InterfaceId(1),
            SocketAddrPlain {
                ip: [3, 0, 0, 0],
                port: 20808,
            },
        );
        sim2.handle_outputs(id3, out3);
        let out3b = engine3.set_enabled(true, HostTime(0));
        sim2.handle_outputs(id3, out3b);
        sim2.engines.insert(id3, engine3);

        let mut engine4 = Engine::new(id4);
        engine4.state.set_tempo(128.0, HostTime(0));
        let out4 = engine4.add_interface(
            InterfaceId(1),
            SocketAddrPlain {
                ip: [2, 0, 0, 0],
                port: 20808,
            },
        );
        sim2.handle_outputs(id4, out4);
        let out4b = engine4.set_enabled(true, HostTime(0));
        sim2.handle_outputs(id4, out4b);
        sim2.engines.insert(id4, engine4);

        sim2.step(2_000_000);

        let t3 = sim2
            .engines
            .get(&id3)
            .unwrap()
            .state
            .timeline
            .tempo
            .to_tempo()
            .0;
        let t4 = sim2
            .engines
            .get(&id4)
            .unwrap()
            .state
            .timeline
            .tempo
            .to_tempo()
            .0;
        assert!((t3 - 128.0).abs() < 0.1, "t3: {}", t3);
        assert!((t4 - 128.0).abs() < 0.1, "t4: {}", t4);
    }
}
