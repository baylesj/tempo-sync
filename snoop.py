import socket
import struct
import time

MCAST_GRP = '224.0.0.251'
MCAST_PORT = 20808

sock = socket.socket(socket.AF_INET, socket.SOCK_DGRAM, socket.IPPROTO_UDP)
sock.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
if hasattr(socket, 'SO_REUSEPORT'):
    sock.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEPORT, 1)

sock.bind(('224.0.0.251', MCAST_PORT))
mreq = struct.pack("4s4s", socket.inet_aton(MCAST_GRP), socket.inet_aton("127.0.0.1"))
sock.setsockopt(socket.IPPROTO_IP, socket.IP_ADD_MEMBERSHIP, mreq)

print("Listening...")
with open("snoop.log", "w") as f:
    while True:
        data, addr = sock.recvfrom(1024)
        f.write(f"From {addr}: {data.hex()}\n")
        f.flush()
