import socket
import select
import sys

s = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
s.bind(("127.0.0.1", 20809))
s.setsockopt(socket.IPPROTO_IP, socket.IP_MULTICAST_IF, socket.inet_aton("127.0.0.1"))
s.setsockopt(socket.IPPROTO_IP, socket.IP_MULTICAST_LOOP, 1)

tempo = "5F 61 73 64 70 5F 76 01 01 05 00 00 54 65 6D 70 6F 48 75 74 74 6D 6C 6E 00 00 00 18 00 00 00 00 00 07 A1 20 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 73 65 73 73 00 00 00 08 54 65 6D 70 6F 48 75 74 73 74 73 74 00 00 00 11 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 6D 65 70 34 00 00 00 06 7F 00 00 01 51 49"
payload = bytes.fromhex(tempo.replace(" ", ""))

while True:
    s.sendto(payload, ("224.76.78.75", 20808))
    r, _, _ = select.select([s], [], [], 0.5)
    if r:
        data, addr = s.recvfrom(1024)
        print("RX:", data.hex())
        sys.stdout.flush()
