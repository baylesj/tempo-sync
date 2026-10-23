import re
with open("crates/tempo-sync-std/src/lib.rs", "r") as f:
    data = f.read()

data = data.replace('let mut sockets = HashMap::new();', 'let mut sockets: HashMap<InterfaceId, UdpSocket> = HashMap::new();')
data = data.replace('let mut multicast_sockets = HashMap::new();', 'let mut multicast_sockets: HashMap<InterfaceId, UdpSocket> = HashMap::new();')
data = data.replace('while let Ok((len, from)) = sock.recv_from(&mut buf)', 'while let Ok((len, from)) = sock.recv_from(&mut buf)')

# The error: `sock.recv_from` type cannot be inferred? 
# from is SocketAddr but wait, we need to bind explicitly? No, `recv_from` infers SocketAddr. 
# "cannot infer type" for from might be because of `sock.recv_from(&mut buf)`. The compiler knows sock is `UdpSocket`. But wait, `recv_from` returns `io::Result<(usize, SocketAddr)>`. Why can't it infer?
# Oh, maybe I need `sock: &UdpSocket` type annotation in the loop.
data = data.replace('for (&id, sock) in &sockets {', 'for (&id, sock) in sockets.iter() {')
data = data.replace('for (&id, sock) in &multicast_sockets {', 'for (&id, sock) in multicast_sockets.iter() {')

with open("crates/tempo-sync-std/src/lib.rs", "w") as f:
    f.write(data)
