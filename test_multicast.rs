use std::net::{UdpSocket, SocketAddrV4, Ipv4Addr};
fn main() {
    let sock = UdpSocket::bind(SocketAddrV4::new(Ipv4Addr::new(0,0,0,0), 20808));
    println!("Bind: {:?}", sock.is_ok());
}
