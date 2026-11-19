"""Tiny AbletonOSC client (Live listens on 11000, replies to 11001). CLI: python3 tools/live-osc.py /live/test"""
import socket, struct, sys, time, json
def pad(b): return b + b"\0" * (4 - len(b) % 4)
def enc(addr, args):
    tags, data = ",", b""
    for a in args:
        if isinstance(a, bool): tags += "T" if a else "F"
        elif isinstance(a, int): tags += "i"; data += struct.pack(">i", a)
        elif isinstance(a, float): tags += "f"; data += struct.pack(">f", a)
        else: tags += "s"; data += pad(str(a).encode())
    return pad(addr.encode()) + pad(tags.encode()) + data
def rs(d, i):
    e = d.index(b"\0", i); return d[i:e].decode(), (e + 4) & ~3
def dec(d):
    a, i = rs(d, 0); t, i = rs(d, i); out = []
    for c in t[1:]:
        if c == "i": out.append(struct.unpack(">i", d[i:i+4])[0]); i += 4
        elif c == "f": out.append(round(struct.unpack(">f", d[i:i+4])[0], 4)); i += 4
        elif c == "s": s, i = rs(d, i); out.append(s)
        elif c in "TF": out.append(c == "T")
        elif c == "N": out.append(None)
    return a, out
class Live:
    def __init__(s):
        s.sock = socket.socket(socket.AF_INET, socket.SOCK_DGRAM); s.sock.bind(("127.0.0.1", 11001)); s.sock.settimeout(0.5)
    def send(s, addr, *args): s.sock.sendto(enc(addr, list(args)), ("127.0.0.1", 11000))
    def ask(s, addr, *args, wait=1.0):
        s.send(addr, *args); end = time.time() + wait
        while time.time() < end:
            try: a, v = dec(s.sock.recvfrom(65536)[0])
            except socket.timeout: continue
            if a == addr or a.startswith("/live/error"): return a, v
        return None
    def drain(s, secs):
        out, end = [], time.time() + secs
        while time.time() < end:
            try: out.append((round(time.time(), 3), *dec(s.sock.recvfrom(65536)[0])))
            except socket.timeout: pass
        return out
if __name__ == "__main__":
    L = Live(); args = sys.argv[1:]
    def conv(x):
        for f in (int, float):
            try: return f(x)
            except ValueError: pass
        return x
    print(L.ask(args[0], *map(conv, args[1:])))
