use std::net::{UdpSocket, SocketAddrV4, Ipv4Addr};
fn main() {
    match UdpSocket::bind(SocketAddrV4::new(Ipv4Addr::new(0,0,0,0), 20808)) {
        Ok(_) => println!("Bound successfully"),
        Err(e) => println!("Bind failed: {}", e),
    }
}
