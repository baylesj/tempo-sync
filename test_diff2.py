import socket, struct, time, select, subprocess

def run_test(payload_hex):
    s = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
    s.bind(("127.0.0.1", 20809))
    s.setsockopt(socket.IPPROTO_IP, socket.IP_MULTICAST_IF, socket.inet_aton("127.0.0.1"))
    s.setsockopt(socket.IPPROTO_IP, socket.IP_MULTICAST_LOOP, 1)

    p = subprocess.Popen(["strace", "-f", "-e", "trace=sendto", "tools/linkpeer/linkpeer"], stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
    payload = bytes.fromhex(payload_hex.replace(" ", ""))
    t0 = time.time()
    while time.time() - t0 < 3:
        s.sendto(payload, ("224.76.78.75", 20808))
        r, _, _ = select.select([s], [], [], 0.5)
        if r:
            s.recvfrom(1024)

    p.terminate()
    out, _ = p.communicate()
    has_ping = False
    for line in out.decode().splitlines():
        if "sendto(" in line and "_link_v" in line:
            has_ping = True
    return has_ping

# tempohut's exact packet after adopting linkpeer's session (with port changed to 20809)
adopted = "5F617364705F76010105000054656D706F487574746D6C6E00000018000000000007A12000000000000011A0FFFFFFFFFA24CE4073657373000000085772323A626D7A4E737473740000001100000000000000000000000000000EFDF06D657034000000067F0000015149"
print("Adopted:", run_test(adopted))
