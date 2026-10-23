// SPDX-License-Identifier: Apache-2.0

use std::sync::{Arc, Mutex};
use std::time::Instant;
#[allow(unused_imports)]
use tempo_sync_core::{Engine, Output, SessionState, HostTime};

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

pub struct SessionHandle {
    state: Arc<Mutex<Option<SessionState>>>,
    num_peers: Arc<Mutex<u32>>,
}

impl SessionHandle {
    pub fn capture_state(&self) -> Option<SessionState> {
        self.state.lock().unwrap().clone()
    }

    pub fn commit_state(&self, state: SessionState) {
        *self.state.lock().unwrap() = Some(state);
    }

    pub fn num_peers(&self) -> u32 {
        *self.num_peers.lock().unwrap()
    }
}

#[allow(dead_code)]
pub struct Runtime {
    engine: Engine,
    clock: Clock,
}

impl Default for Runtime {
    fn default() -> Self {
        Self::new()
    }
}

impl Runtime {
    pub fn new() -> Self {
        Runtime {
            engine: Engine::new(),
            clock: Clock::new(),
        }
    }

    pub fn run(&mut self) {
        // Run loop stub. Needs UdpSockets, etc.
    }
}
