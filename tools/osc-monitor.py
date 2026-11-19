#!/usr/bin/env python3
"""Print OSC messages arriving on a UDP port (default 9000), with time and gap since the last one.

    python3 tools/osc-monitor.py [port]
"""
import socket, struct, sys, time

def pad(n):
    return (n + 4) & ~3

def read_str(data, i):
    end = data.index(b"\0", i)
    return data[i:end].decode(errors="replace"), pad(end)

def decode(data):
    if data.startswith(b"#bundle"):
        i, out = 16, []
        while i < len(data):
            (size,) = struct.unpack(">i", data[i:i + 4])
            out += decode(data[i + 4:i + 4 + size])
            i += 4 + size
        return out
    address, i = read_str(data, 0)
    tags, i = read_str(data, i)
    args = []
    for t in tags[1:]:
        if t == "i":
            args.append(struct.unpack(">i", data[i:i + 4])[0]); i += 4
        elif t == "f":
            args.append(round(struct.unpack(">f", data[i:i + 4])[0], 3)); i += 4
        elif t == "s":
            s, i = read_str(data, i); args.append(s)
        elif t in "TF":
            args.append(t == "T")
        else:
            args.append(f"<{t}>")
    return [(address, args)]

port = int(sys.argv[1]) if len(sys.argv) > 1 else 9000
sock = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
sock.bind(("127.0.0.1", port))
print(f"listening on 127.0.0.1:{port} (Ctrl-C to quit)")
start = last = None
try:
    while True:
        data, _ = sock.recvfrom(65536)
        now = time.monotonic()
        start = start or now
        gap = "" if last is None else f"+{now - last:5.2f}s"
        last = now
        for address, args in decode(data):
            print(f"{now - start:7.2f}s {gap:>8}  {address} {' '.join(map(str, args))}", flush=True)
except KeyboardInterrupt:
    pass
