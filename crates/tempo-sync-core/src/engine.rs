// SPDX-License-Identifier: Apache-2.0

use crate::time::HostTime;
use crate::timeline::SessionState;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct InterfaceId(pub u32);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SocketAddrPlain {
    pub ip: [u8; 4],
    pub port: u16,
}

pub enum Output {
    Send {
        iface_id: InterfaceId,
        to: SocketAddrPlain,
        bytes: alloc::vec::Vec<u8>,
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

pub struct Engine {
    // fields pending protocol doc
}

impl Engine {
    pub fn new() -> Self {
        Engine {}
    }

    pub fn handle_packet(&mut self, _iface_id: InterfaceId, _from: SocketAddrPlain, _bytes: &[u8], _now: HostTime) -> alloc::vec::Vec<Output> {
        alloc::vec::Vec::new()
    }

    pub fn handle_timer(&mut self, _now: HostTime) -> alloc::vec::Vec<Output> {
        alloc::vec::Vec::new()
    }

    pub fn commit_app_state(&mut self, _state: SessionState, _now: HostTime) -> alloc::vec::Vec<Output> {
        alloc::vec::Vec::new()
    }

    pub fn set_enabled(&mut self, _enabled: bool, _now: HostTime) -> alloc::vec::Vec<Output> {
        alloc::vec::Vec::new()
    }

    pub fn add_interface(&mut self, _iface_id: InterfaceId, _local_addr: [u8; 4]) -> alloc::vec::Vec<Output> {
        alloc::vec::Vec::new()
    }

    pub fn remove_interface(&mut self, _iface_id: InterfaceId) -> alloc::vec::Vec<Output> {
        alloc::vec::Vec::new()
    }
}
