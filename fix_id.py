with open("crates/tempo-sync-core/src/engine.rs", "r") as f:
    data = f.read()
data = data.replace('for (_id, peer) in &self.peers {', 'for (id, peer) in &self.peers {')
data = data.replace('        for (id, peer) in &self.peers {\n            if let Some(peer_sess) = peer.session_id {', '        for (_id, peer) in &self.peers {\n            if let Some(peer_sess) = peer.session_id {')
with open("crates/tempo-sync-core/src/engine.rs", "w") as f:
    f.write(data)
