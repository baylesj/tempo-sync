// SPDX-License-Identifier: Apache-2.0

use crate::time::{HostTime, MicroBeats, TempoWire};
use crate::timeline::{Timeline, StartStopState};

pub const ASDP_MAGIC: &[u8; 8] = b"_asdp_v\x01";
pub const LINK_MAGIC: &[u8; 8] = b"_link_v\x01";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct NodeId(pub [u8; 8]);

#[derive(Debug, Clone, PartialEq)]
pub enum DiscoveryMsg {
    Alive {
        node_id: NodeId,
        timeline: Option<Timeline>,
        start_stop: Option<StartStopState>,
        session_id: Option<NodeId>,
    },
    ByeBye {
        node_id: NodeId,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub enum MeasurementMsg {
    Ping {
        host_time: HostTime,
        prev_ghost_time: Option<HostTime>,
    },
    Pong {
        session_id: Option<NodeId>,
        ghost_time: HostTime,
        ping_payload: Vec<u8>,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub enum Message {
    Discovery(DiscoveryMsg),
    Measurement(MeasurementMsg),
}

pub fn parse_message(bytes: &[u8]) -> Option<Message> {
    if bytes.len() < 8 { return None; }
    let magic = &bytes[0..8];
    if magic == ASDP_MAGIC {
        if bytes.len() < 20 { return None; }
        let msg_type = bytes[8];
        let mut node_id = [0u8; 8];
        node_id.copy_from_slice(&bytes[12..20]);
        let node_id = NodeId(node_id);

        if msg_type == 3 {
            return Some(Message::Discovery(DiscoveryMsg::ByeBye { node_id }));
        } else if msg_type == 1 {
            let mut timeline = None;
            let mut start_stop = None;
            let mut session_id = None;

            let mut offset = 20;
            while offset + 8 <= bytes.len() {
                let key = &bytes[offset..offset+4];
                let size = u32::from_be_bytes(bytes[offset+4..offset+8].try_into().unwrap()) as usize;
                offset += 8;
                if offset + size > bytes.len() { break; }
                let value = &bytes[offset..offset+size];
                match key {
                    b"tmln" if size == 24 => {
                        let tempo = TempoWire(i64::from_be_bytes(value[0..8].try_into().unwrap()));
                        let beat_origin = MicroBeats(i64::from_be_bytes(value[8..16].try_into().unwrap()));
                        let time_origin = HostTime(i64::from_be_bytes(value[16..24].try_into().unwrap()));
                        timeline = Some(Timeline { tempo, beat_origin, time_origin });
                    }
                    b"stst" if size == 17 => {
                        let is_playing = value[0] != 0;
                        let beat = MicroBeats(i64::from_be_bytes(value[1..9].try_into().unwrap()));
                        let time = HostTime(i64::from_be_bytes(value[9..17].try_into().unwrap()));
                        start_stop = Some(StartStopState { is_playing, beat, time });
                    }
                    b"sess" if size == 8 => {
                        let mut sess = [0u8; 8];
                        sess.copy_from_slice(value);
                        session_id = Some(NodeId(sess));
                    }
                    _ => {}
                }
                offset += size;
            }
            return Some(Message::Discovery(DiscoveryMsg::Alive { node_id, timeline, start_stop, session_id }));
        }
    } else if magic == LINK_MAGIC {
        if bytes.len() < 9 { return None; }
        let msg_type = bytes[8];
        let mut host_time = None;
        let mut prev_ghost_time = None;
        let mut ghost_time = None;
        let mut session_id = None;
        let mut ping_payload = Vec::new();

        let mut offset = 9;
        if msg_type == 1 {
            while offset + 8 <= bytes.len() {
                let key = &bytes[offset..offset+4];
                let size = u32::from_be_bytes(bytes[offset+4..offset+8].try_into().unwrap()) as usize;
                offset += 8;
                if offset + size > bytes.len() { break; }
                let value = &bytes[offset..offset+size];
                match key {
                    b"__ht" if size == 8 => host_time = Some(HostTime(i64::from_be_bytes(value.try_into().unwrap()))),
                    b"_pgt" if size == 8 => prev_ghost_time = Some(HostTime(i64::from_be_bytes(value.try_into().unwrap()))),
                    _ => {}
                }
                offset += size;
            }
            if let Some(ht) = host_time {
                return Some(Message::Measurement(MeasurementMsg::Ping { host_time: ht, prev_ghost_time }));
            }
        } else if msg_type == 2 {
            // Pong may append Ping payload
            // let start_offset = offset;
            while offset + 8 <= bytes.len() {
                let key = &bytes[offset..offset+4];
                let size = u32::from_be_bytes(bytes[offset+4..offset+8].try_into().unwrap()) as usize;
                
                // If we see another _link_v1, that's the appended ping payload! Wait, does the ping payload include the magic header?
                // "Appends the entire payload of the received Ping". The ping payload means from byte 0.
                if key == b"_lin" {
                     ping_payload.extend_from_slice(&bytes[offset..]);
                     break;
                }

                offset += 8;
                if offset + size > bytes.len() { break; }
                let value = &bytes[offset..offset+size];
                match key {
                    b"sess" if size == 8 => {
                        let mut sess = [0u8; 8];
                        sess.copy_from_slice(value);
                        session_id = Some(NodeId(sess));
                    }
                    b"__gt" if size == 8 => ghost_time = Some(HostTime(i64::from_be_bytes(value.try_into().unwrap()))),
                    b"_lin" => {} // Handled above, but just in case
                    _ => {}
                }
                offset += size;
            }
            if let Some(gt) = ghost_time {
                return Some(Message::Measurement(MeasurementMsg::Pong { session_id, ghost_time: gt, ping_payload }));
            }
        }
    }
    None
}

pub fn serialize_discovery_alive(node_id: &NodeId, session_id: &NodeId, timeline: &Timeline, start_stop: &StartStopState) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(ASDP_MAGIC);
    out.push(1); // type Alive
    out.extend_from_slice(&[0, 0, 0]); // padding
    out.extend_from_slice(&node_id.0);

    // Timeline
    out.extend_from_slice(b"tmln");
    out.extend_from_slice(&24u32.to_be_bytes());
    out.extend_from_slice(&timeline.tempo.0.to_be_bytes());
    out.extend_from_slice(&timeline.beat_origin.0.to_be_bytes());
    out.extend_from_slice(&timeline.time_origin.0.to_be_bytes());

    // SessionMembership
    out.extend_from_slice(b"sess");
    out.extend_from_slice(&8u32.to_be_bytes());
    out.extend_from_slice(&session_id.0);

    // StartStopState
    out.extend_from_slice(b"stst");
    out.extend_from_slice(&17u32.to_be_bytes());
    out.push(if start_stop.is_playing { 1 } else { 0 });
    out.extend_from_slice(&start_stop.beat.0.to_be_bytes());
    out.extend_from_slice(&start_stop.time.0.to_be_bytes());

    out
}

pub fn serialize_discovery_byebye(node_id: &NodeId) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(ASDP_MAGIC);
    out.push(3); // type ByeBye
    out.extend_from_slice(&[0, 0, 0]); // padding
    out.extend_from_slice(&node_id.0);
    out
}

pub fn serialize_measurement_ping(host_time: HostTime, prev_ghost_time: Option<HostTime>) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(LINK_MAGIC);
    out.push(1); // type Ping

    out.extend_from_slice(b"__ht");
    out.extend_from_slice(&8u32.to_be_bytes());
    out.extend_from_slice(&host_time.0.to_be_bytes());

    if let Some(pgt) = prev_ghost_time {
        out.extend_from_slice(b"_pgt");
        out.extend_from_slice(&8u32.to_be_bytes());
        out.extend_from_slice(&pgt.0.to_be_bytes());
    }

    out
}

pub fn serialize_measurement_pong(session_id: &NodeId, ghost_time: HostTime, ping_payload: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(LINK_MAGIC);
    out.push(2); // type Pong

    out.extend_from_slice(b"sess");
    out.extend_from_slice(&8u32.to_be_bytes());
    out.extend_from_slice(&session_id.0);

    out.extend_from_slice(b"__gt");
    out.extend_from_slice(&8u32.to_be_bytes());
    out.extend_from_slice(&ghost_time.0.to_be_bytes());

    out.extend_from_slice(ping_payload);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_discovery_alive() {
        let hex = "5f617364705f7601010500000102030405060708746d6c6e00000018000000000007a120000000000000000000000000000000007365737300000008080706050403020173747374000000110100000000000f424000000000001e84806d65703400000006c0a8010551486165703400000006c0a801055149";
        let bytes = (0..hex.len()).step_by(2).map(|i| u8::from_str_radix(&hex[i..i+2], 16).unwrap()).collect::<Vec<u8>>();
        let msg = parse_message(&bytes).unwrap();
        match msg {
            Message::Discovery(DiscoveryMsg::Alive { node_id, timeline, start_stop, session_id }) => {
                assert_eq!(node_id.0, [1, 2, 3, 4, 5, 6, 7, 8]);
                assert_eq!(timeline.as_ref().unwrap().tempo.0, 500000);
                assert_eq!(timeline.as_ref().unwrap().beat_origin.0, 0);
                assert_eq!(timeline.as_ref().unwrap().time_origin.0, 0);
                assert_eq!(session_id.unwrap().0, [8, 7, 6, 5, 4, 3, 2, 1]);
                assert_eq!(start_stop.as_ref().unwrap().is_playing, true);
                assert_eq!(start_stop.as_ref().unwrap().beat.0, 1000000);
                assert_eq!(start_stop.as_ref().unwrap().time.0, 2000000);
            }
            _ => panic!("Wrong message type"),
        }
    }

    #[test]
    fn test_parse_discovery_byebye() {
        let hex = "5f617364705f7601030000000102030405060708";
        let bytes = (0..hex.len()).step_by(2).map(|i| u8::from_str_radix(&hex[i..i+2], 16).unwrap()).collect::<Vec<u8>>();
        let msg = parse_message(&bytes).unwrap();
        match msg {
            Message::Discovery(DiscoveryMsg::ByeBye { node_id }) => {
                assert_eq!(node_id.0, [1, 2, 3, 4, 5, 6, 7, 8]);
            }
            _ => panic!("Wrong message type"),
        }
    }

    #[test]
    fn test_parse_measurement_ping() {
        let hex = "5f6c696e6b5f7601015f5f68740000000800000000000f42405f70677400000008000000000007a120";
        let bytes = (0..hex.len()).step_by(2).map(|i| u8::from_str_radix(&hex[i..i+2], 16).unwrap()).collect::<Vec<u8>>();
        let msg = parse_message(&bytes).unwrap();
        match msg {
            Message::Measurement(MeasurementMsg::Ping { host_time, prev_ghost_time }) => {
                assert_eq!(host_time.0, 1000000);
                assert_eq!(prev_ghost_time.unwrap().0, 500000);
            }
            _ => panic!("Wrong message type"),
        }
    }

    #[test]
    fn test_parse_measurement_pong() {
        let hex = "5f6c696e6b5f760102736573730000000808070605040302015f5f67740000000800000000000f42405f6c696e6b5f760102736573730000000808070605040302015f5f67740000000800000000000f4240";
        let bytes = (0..hex.len()).step_by(2).map(|i| u8::from_str_radix(&hex[i..i+2], 16).unwrap()).collect::<Vec<u8>>();
        let msg = parse_message(&bytes).unwrap();
        match msg {
            Message::Measurement(MeasurementMsg::Pong { session_id, ghost_time, ping_payload }) => {
                assert_eq!(session_id.unwrap().0, [8, 7, 6, 5, 4, 3, 2, 1]);
                assert_eq!(ghost_time.0, 1000000);
                assert!(ping_payload.starts_with(LINK_MAGIC));
            }
            _ => panic!("Wrong message type"),
        }
    }
}
