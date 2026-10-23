import socket, time, select, subprocess
s1 = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
s1.bind(("127.0.0.1", 30000)) # mcast sender

s2 = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
s2.bind(("127.0.0.1", 40000)) # mep4 listener

s1.setsockopt(socket.IPPROTO_IP, socket.IP_MULTICAST_IF, socket.inet_aton("127.0.0.1"))
s1.setsockopt(socket.IPPROTO_IP, socket.IP_MULTICAST_LOOP, 1)

p = subprocess.Popen(["strace", "-f", "-e", "trace=sendto", "tools/linkpeer/linkpeer"], stdout=subprocess.PIPE, stderr=subprocess.STDOUT)

# payload with mep4=40000 (0x9C40)
adopted = "5F617364705F76010105000054656D706F487574746D6C6E00000018000000000007A12000000000000011A0FFFFFFFFFA24CE4073657373000000085772323A626D7A4E737473740000001100000000000000000000000000000EFDF06D657034000000067F0000019C40"
payload = bytes.fromhex(adopted.replace(" ", ""))

t0 = time.time()
ping_port = None
while time.time() - t0 < 3:
    s1.sendto(payload, ("224.76.78.75", 20808))
    r, _, _ = select.select([s1, s2], [], [], 0.5)
    for sock in r:
        data, addr = sock.recvfrom(1024)
        if data.startswith(b"_link_v"):
            ping_port = sock.getsockname()[1]

p.terminate()
out, _ = p.communicate()
print("Ping received on port:", ping_port)
