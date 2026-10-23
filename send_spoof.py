import socket
import time

s = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
s.bind(("127.0.0.1", 20809))
s.setsockopt(socket.IPPROTO_IP, socket.IP_MULTICAST_IF, socket.inet_aton("127.0.0.1"))
s.setsockopt(socket.IPPROTO_IP, socket.IP_MULTICAST_LOOP, 1)

# With mep4=127.0.0.1:20809 and aep4=127.0.0.1:20810
payload_hex = "5f 61 73 64 70 5f 76 01 01 05 00 00 01 02 03 04 05 06 07 08 74 6d 6c 6e 00 00 00 18 00 00 00 00 00 07 a1 20 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 73 65 73 73 00 00 00 08 08 07 06 05 04 03 02 01 73 74 73 74 00 00 00 11 01 00 00 00 00 00 0f 42 40 00 00 00 00 00 1e 84 80 6d 65 70 34 00 00 00 06 7f 00 00 01 51 49 61 65 70 34 00 00 00 06 7f 00 00 01 51 4a"
payload = bytes.fromhex(payload_hex.replace(" ", ""))

while True:
    s.sendto(payload, ("224.76.78.75", 20808))
    time.sleep(1)
