#!/usr/bin/env python3
"""pkt2ai - giai ma file Cisco Packet Tracer (.pkt/.pka) sang XML, va trich
xuat JSON gon de dua cho AI doc.

Khong can thu vien ngoai (chi dung stdlib). Twofish/EAX duoc tu cai dat.

Pipeline (theo cac cong cu da biet, PT 7.x tro len):
  .pkt --[dao byte + XOR]--> Twofish-EAX (key 0x89*16, nonce 0x10*16, tag 16B o cuoi)
       --[XOR]--> [4 byte kich thuoc goc, big-endian] + zlib --> XML

Chi dung cho file cua chinh ban hoac file ban duoc phep su dung.
"""
import argparse
import struct
import sys
import zlib

KEY = bytes([137]) * 16
NONCE = bytes([16]) * 16
BLOCK = 16
M32 = 0xFFFFFFFF


# --------------------------------------------------------------------------
# Twofish (chi ho tro khoa 128-bit, chi chieu ma hoa - du cho EAX/CTR/CMAC)
# --------------------------------------------------------------------------
_QT = (
    ((0x8, 0x1, 0x7, 0xD, 0x6, 0xF, 0x3, 0x2, 0x0, 0xB, 0x5, 0x9, 0xE, 0xC, 0xA, 0x4),
     (0xE, 0xC, 0xB, 0x8, 0x1, 0x2, 0x3, 0x5, 0xF, 0x4, 0xA, 0x6, 0x7, 0x0, 0x9, 0xD),
     (0xB, 0xA, 0x5, 0xE, 0x6, 0xD, 0x9, 0x0, 0xC, 0x8, 0xF, 0x3, 0x2, 0x4, 0x7, 0x1),
     (0xD, 0x7, 0xF, 0x4, 0x1, 0x2, 0x6, 0xE, 0x9, 0xB, 0x3, 0x0, 0x8, 0x5, 0xC, 0xA)),
    ((0x2, 0x8, 0xB, 0xD, 0xF, 0x7, 0x6, 0xE, 0x3, 0x1, 0x9, 0x4, 0x0, 0xA, 0xC, 0x5),
     (0x1, 0xE, 0x2, 0xB, 0x4, 0xC, 0x3, 0x7, 0x6, 0xD, 0xA, 0x5, 0xF, 0x9, 0x0, 0x8),
     (0x4, 0xC, 0x7, 0x5, 0x1, 0x6, 0x9, 0xA, 0x0, 0xE, 0xD, 0x8, 0x2, 0xB, 0x3, 0xF),
     (0xB, 0x9, 0x5, 0x1, 0xC, 0x3, 0xD, 0xE, 0x6, 0x4, 0x7, 0xF, 0x2, 0x0, 0x8, 0xA)),
)
_MDS = ((0x01, 0xEF, 0x5B, 0x5B),
        (0x5B, 0xEF, 0xEF, 0x01),
        (0xEF, 0x5B, 0x01, 0xEF),
        (0xEF, 0x01, 0xEF, 0x5B))
_RS = ((0x01, 0xA4, 0x55, 0x87, 0x5A, 0x58, 0xDB, 0x9E),
       (0xA4, 0x56, 0x82, 0xF3, 0x1E, 0xC6, 0x68, 0xE5),
       (0x02, 0xA1, 0xFC, 0xC1, 0x47, 0xAE, 0x3D, 0x19),
       (0xA4, 0x55, 0x87, 0x5A, 0x58, 0xDB, 0x9E, 0x03))


def _ror4(x, n):
    return ((x >> n) | (x << (4 - n))) & 15


def _make_q(t):
    t0, t1, t2, t3 = t
    out = []
    for x in range(256):
        a0, b0 = x >> 4, x & 15
        a1 = a0 ^ b0
        b1 = (a0 ^ _ror4(b0, 1) ^ ((a0 << 3) & 15)) & 15
        a2, b2 = t0[a1], t1[b1]
        a3 = a2 ^ b2
        b3 = (a2 ^ _ror4(b2, 1) ^ ((a2 << 3) & 15)) & 15
        a4, b4 = t2[a3], t3[b3]
        out.append((b4 << 4) | a4)
    return out


_Q0 = _make_q(_QT[0])
_Q1 = _make_q(_QT[1])


def _gf(a, b, poly):
    r = 0
    while b:
        if b & 1:
            r ^= a
        a <<= 1
        if a & 0x100:
            a ^= poly
        b >>= 1
    return r


def _rol(x, n):
    return ((x << n) | (x >> (32 - n))) & M32


def _ror(x, n):
    return ((x >> n) | (x << (32 - n))) & M32


def _y_bytes(x, l0, l1):
    """4 byte sau lop q-hoan-vi cua h (k=2); x la 4 byte dau vao."""
    return (
        _Q1[_Q0[_Q0[x[0]] ^ l1[0]] ^ l0[0]],
        _Q0[_Q0[_Q1[x[1]] ^ l1[1]] ^ l0[1]],
        _Q1[_Q1[_Q0[x[2]] ^ l1[2]] ^ l0[2]],
        _Q0[_Q1[_Q1[x[3]] ^ l1[3]] ^ l0[3]],
    )


def _mds_word(y):
    w = 0
    for i in range(4):
        v = 0
        for j in range(4):
            v ^= _gf(_MDS[i][j], y[j], 0x169)
        w |= v << (8 * i)
    return w


def _h(x_word, L):
    xb = x_word.to_bytes(4, "little")
    y = _y_bytes(xb, L[0].to_bytes(4, "little"), L[1].to_bytes(4, "little"))
    return _mds_word(y)


class Twofish:
    """Twofish khoa 128-bit. Chi co encrypt() mot khoi 16 byte."""

    def __init__(self, key):
        if len(key) != 16:
            raise ValueError("chi ho tro khoa 16 byte")
        w = struct.unpack("<4I", key)
        me, mo = (w[0], w[2]), (w[1], w[3])
        s = []
        for i in range(2):
            m = key[8 * i:8 * i + 8]
            sb = bytes(
                _xor_all(_gf(_RS[r][c], m[c], 0x14D) for c in range(8))
                for r in range(4)
            )
            s.append(struct.unpack("<I", sb)[0])
        self._S = (s[1], s[0])  # L0 = S_{k-1}

        rho = 0x01010101
        K = []
        for i in range(20):
            a = _h((2 * i * rho) & M32, me)
            b = _rol(_h(((2 * i + 1) * rho) & M32, mo), 8)
            K.append((a + b) & M32)
            K.append(_rol((a + 2 * b) & M32, 9))
        self._K = K

        # bang S-box phu thuoc khoa: sb[j][x] = dong gop cua byte j vao g()
        l0 = self._S[0].to_bytes(4, "little")
        l1 = self._S[1].to_bytes(4, "little")
        sbox = [[0] * 256 for _ in range(4)]
        for j in range(4):
            for x in range(256):
                xb = [0, 0, 0, 0]
                xb[j] = x
                y = _y_bytes(xb, l0, l1)
                # chi lay cot j cua MDS nhan voi y[j]
                col = [0, 0, 0, 0]
                col[j] = y[j]
                sbox[j][x] = _mds_word(col)
        self._sb = sbox

    def encrypt(self, block):
        K = self._K
        sb0, sb1, sb2, sb3 = self._sb
        p = struct.unpack("<4I", block)
        r0, r1, r2, r3 = p[0] ^ K[0], p[1] ^ K[1], p[2] ^ K[2], p[3] ^ K[3]
        for r in range(16):
            t0 = (sb0[r0 & 255] ^ sb1[(r0 >> 8) & 255]
                  ^ sb2[(r0 >> 16) & 255] ^ sb3[r0 >> 24])
            v = _rol(r1, 8)
            t1 = (sb0[v & 255] ^ sb1[(v >> 8) & 255]
                  ^ sb2[(v >> 16) & 255] ^ sb3[v >> 24])
            f0 = (t0 + t1 + K[2 * r + 8]) & M32
            f1 = (t0 + 2 * t1 + K[2 * r + 9]) & M32
            r2 = _ror(r2 ^ f0, 1)
            r3 = _rol(r3, 1) ^ f1
            r0, r1, r2, r3 = r2, r3, r0, r1
        return struct.pack("<4I", r2 ^ K[4], r3 ^ K[5], r0 ^ K[6], r1 ^ K[7])


def _xor_all(it):
    v = 0
    for x in it:
        v ^= x
    return v


# --------------------------------------------------------------------------
# CMAC (OMAC1), CTR, EAX
# --------------------------------------------------------------------------
def _xor(a, b):
    return bytes(x ^ y for x, y in zip(a, b))


def _dbl(b):
    n = int.from_bytes(b, "big") << 1
    if n >> 128:
        n = (n & ((1 << 128) - 1)) ^ 0x87
    return n.to_bytes(16, "big")


class _Cmac:
    def __init__(self, enc):
        self.enc = enc
        l = enc(bytes(16))
        self.k1 = _dbl(l)
        self.k2 = _dbl(self.k1)

    def digest(self, data):
        n = len(data)
        if n and n % 16 == 0:
            head, last = data[:-16], _xor(data[-16:], self.k1)
        else:
            tail = data[n - (n % 16):] if n else b""
            pad = tail + b"\x80" + bytes(15 - len(tail))
            head, last = data[:n - len(tail)], _xor(pad, self.k2)
        x = bytes(16)
        for i in range(0, len(head), 16):
            x = self.enc(_xor(x, head[i:i + 16]))
        return self.enc(_xor(x, last))


def _ctr(enc, counter, data):
    ctr = int.from_bytes(counter, "big")
    out = bytearray(len(data))
    for off in range(0, len(data), 16):
        ks = enc(ctr.to_bytes(16, "big"))
        chunk = data[off:off + 16]
        out[off:off + len(chunk)] = _xor(chunk, ks)
        ctr = (ctr + 1) & ((1 << 128) - 1)
    return bytes(out)


def _omac(cmac, t, data):
    return cmac.digest(bytes(15) + bytes([t]) + data)


def eax_encrypt(enc, nonce, plaintext, header=b""):
    cm = _Cmac(enc)
    n = _omac(cm, 0, nonce)
    h = _omac(cm, 1, header)
    ct = _ctr(enc, n, plaintext)
    c = _omac(cm, 2, ct)
    return ct, _xor(_xor(n, h), c)


def eax_decrypt(enc, nonce, ciphertext, tag, header=b"", verify=True):
    cm = _Cmac(enc)
    n = _omac(cm, 0, nonce)
    h = _omac(cm, 1, header)
    c = _omac(cm, 2, ciphertext)
    if verify and _xor(_xor(n, h), c) != tag:
        raise ValueError("EAX tag khong khop (sai key/IV hoac dinh dang file khac)")
    return _ctr(enc, n, ciphertext)


# --------------------------------------------------------------------------
# Lop obfuscation + nen cua Packet Tracer
# --------------------------------------------------------------------------
def _obf1_decode(d):
    n = len(d)
    return bytes(d[n - 1 - i] ^ ((n - i * n) & 0xFF) for i in range(n))


def _obf1_encode(d):
    n = len(d)
    out = bytearray(n)
    for i in range(n):
        out[n - 1 - i] = d[i] ^ ((n - i * n) & 0xFF)
    return bytes(out)


def _obf2(d):
    n = len(d)
    return bytes(b ^ ((n - i) & 0xFF) for i, b in enumerate(d))


def pkt_to_xml(data, verify=True):
    """bytes cua .pkt -> bytes XML."""
    if len(data) < 32:
        raise ValueError("file qua ngan de la .pkt")
    s1 = _obf1_decode(data)
    enc = Twofish(KEY).encrypt
    plain = eax_decrypt(enc, NONCE, s1[:-16], s1[-16:], verify=verify)
    s2 = _obf2(plain)
    size = struct.unpack(">I", s2[:4])[0]
    try:
        xml = zlib.decompress(s2[4:])
    except zlib.error as e:
        raise ValueError(f"giai nen zlib that bai: {e}") from e
    if len(xml) != size:
        print(f"[!] canh bao: kich thuoc khai bao {size} != thuc te {len(xml)}",
              file=sys.stderr)
    return xml


def xml_to_pkt(xml):
    """bytes XML -> bytes .pkt."""
    blob = struct.pack(">I", len(xml)) + zlib.compress(xml)
    plain = _obf2(blob)
    ct, tag = eax_encrypt(Twofish(KEY).encrypt, NONCE, plain)
    return _obf1_encode(ct + tag)


# --------------------------------------------------------------------------
# Trich xuat topology -> JSON / Markdown gon cho AI
# --------------------------------------------------------------------------
import collections
import json
import re
import xml.etree.ElementTree as ET

_SECRET_RE = re.compile(r"(?i)\b(password|secret|community|key-string)(\s+\d+)?\s+\S+")
_BOILERPLATE = {
    "version", "no service timestamps log datetime msec",
    "no service timestamps debug datetime msec", "no service password-encryption",
    "service timestamps log datetime msec", "service timestamps debug datetime msec",
    "service password-encryption", "ip cef", "no ipv6 cef", "end",
    "spanning-tree mode pvst", "spanning-tree extend system-id",
}
_IFACE_NOISE = {"no ip address", "duplex auto", "speed auto"}


def _txt(e, path, default=""):
    v = e.findtext(path)
    return v.strip() if v and v.strip() else default


def _redact(line):
    return _SECRET_RE.sub(lambda m: m.group(1) + " <redacted>", line)


def _parse_config(lines, redact=True):
    """Tach running-config thanh interface / routing / phan con lai."""
    if redact:
        lines = [_redact(l) for l in lines]
    interfaces, routing, other = [], [], []
    cur = None  # (kind, dict)
    for raw in lines:
        if raw is None:
            continue
        line = raw.rstrip()
        s = line.strip()
        if not s or s == "!":
            cur = None
            continue
        indented = line.startswith(" ")
        if not indented:
            cur = None
            if s.startswith("interface "):
                cur = ("if", {"name": s[10:].strip(), "ip": None, "mask": None,
                              "shutdown": False, "other": []})
                interfaces.append(cur[1])
            elif s.startswith(("router ", "ip access-list ", "ip dhcp pool ", "line ",
                               "vlan ", "ipv6 router ", "crypto ")):
                cur = ("blk", {"block": s, "lines": []})
                routing.append(cur[1])
            elif s.lower() in _BOILERPLATE or s.startswith("version "):
                pass
            else:
                other.append(s)
            continue
        if cur is None:
            other.append(s)
        elif cur[0] == "if":
            d = cur[1]
            m = re.match(r"ip address (\S+) (\S+)", s)
            if m:
                d["ip"], d["mask"] = m.group(1), m.group(2)
            elif s == "ip address dhcp":
                d["ip"] = "dhcp"
            elif s == "shutdown":
                d["shutdown"] = True
            elif s not in _IFACE_NOISE:
                d["other"].append(s)
        else:
            cur[1]["lines"].append(s)
    return interfaces, routing, other


def _port_info(p):
    ip = _txt(p, "IP")
    info = {
        "type": _txt(p, "TYPE"),
        "mac": _txt(p, "MACADDRESS"),
        "ip": ip or None,
        "subnet": _txt(p, "SUBNET") or None,
        "gateway": _txt(p, "PORT_GATEWAY") or None,
        "dns": _txt(p, "PORT_DNS") or None,
        "dhcp": _txt(p, "PORT_DHCP_ENABLE") == "true",
    }
    return {k: v for k, v in info.items() if v not in (None, "", False)}


def extract(xml, source="", redact=True, full_config=False):
    root = ET.fromstring(xml)
    warnings = []
    net = root.find("NETWORK")
    dev_nodes = net.find("DEVICES") if net is not None else None
    if dev_nodes is None or len(dev_nodes) == 0:
        warnings.append("Khong tim thay NETWORK/DEVICES - cau truc XML co the da doi. "
                        "Chay `pkt2ai tree file.xml` de xem cac the thuc te.")
        dev_nodes = []

    devices, by_addr, by_idx = [], {}, {}
    for idx, d in enumerate(dev_nodes):
        e = d.find("ENGINE")
        if e is None:
            continue
        t = e.find("TYPE")
        dev = {
            "name": _txt(e, "NAME", f"device{idx}"),
            "type": (t.text or "").strip() if t is not None else "",
            "model": (t.get("model") or "") if t is not None else "",
        }
        x, y = _txt(d, "WORKSPACE/LOGICAL/X"), _txt(d, "WORKSPACE/LOGICAL/Y")
        if x and y:
            dev["xy"] = [float(x), float(y)]
        ports = [p for p in e.iter("PORT")
                 if p.find("TYPE") is not None and "Bluetooth" not in _txt(p, "TYPE")]
        shown = [p for p in ports if _txt(p, "IP") or _txt(p, "PORT_DHCP_ENABLE") == "true"
                 or len(ports) <= 3]
        pinfo = [_port_info(p) for p in shown]
        if pinfo:
            dev["ports"] = pinfo
        gw = _txt(e, "GATEWAY")
        if gw and not any(p.get("gateway") == gw for p in pinfo):
            dev["gateway"] = gw
        seen, vlans = set(), []
        for v in e.iter("VLAN"):
            num, nm = v.get("number") or "", v.get("name")
            if num.isdigit() and nm is not None and int(num) < 1002 \
                    and (num, nm) not in seen:  # bo VLAN he thong 1002-1005 cho gon
                seen.add((num, nm))
                vlans.append({"id": int(num), "name": nm})
        if vlans:
            dev["vlans"] = vlans
        rc = e.find("RUNNINGCONFIG")
        if rc is not None:
            lines = [l.text or "" for l in rc.iter("LINE")]
            ifs, routing, other = _parse_config(lines, redact)
            ifs = [i for i in ifs if i["ip"] or i["other"]]
            if ifs:
                dev["interfaces"] = [{k: v for k, v in i.items() if v not in (None, [], False)}
                                     for i in ifs]
            routing = [b for b in routing if b["lines"]]
            if routing:
                dev["blocks"] = routing
            if other:
                dev["config_other"] = other
            if full_config:
                dev["running_config"] = [(_redact(l) if redact else l) for l in lines]
        devices.append(dev)
        addr = _txt(d, "WORKSPACE/LOGICAL/DEV_ADDR")
        if addr:
            by_addr[addr] = dev["name"]
        by_idx[str(idx)] = dev["name"]

    links = []
    for lk in (net.find("LINKS") if net is not None and net.find("LINKS") is not None else []):
        c = lk.find("CABLE")
        if c is None:
            continue
        ports = [p.text or "" for p in c.findall("PORT")]
        fa, ta = _txt(c, "FROM_DEVICE_MEM_ADDR"), _txt(c, "TO_DEVICE_MEM_ADDR")
        fi, ti = _txt(c, "FROM"), _txt(c, "TO")
        a = by_addr.get(fa) or by_idx.get(fi) or fi
        b = by_addr.get(ta) or by_idx.get(ti) or ti
        links.append({
            "a": a, "a_port": ports[0] if ports else "",
            "b": b, "b_port": ports[1] if len(ports) > 1 else "",
            "cable": _txt(c, "TYPE"), "medium": _txt(lk, "TYPE"),
        })

    names = {d["name"] for d in devices}
    bad = [l for l in links if l["a"] not in names or l["b"] not in names]
    if bad:
        warnings.append(f"{len(bad)} link khong khop duoc ten thiet bi (ten hien thi so/dia chi).")

    return {
        "source": source,
        "pt_version": _txt(root, "VERSION"),
        "counts": {
            "devices": len(devices), "links": len(links),
            "by_type": dict(collections.Counter(d["type"] for d in devices)),
        },
        "devices": devices,
        "links": links,
        **({"warnings": warnings} if warnings else {}),
    }


def to_markdown(info):
    o = [f"# Packet Tracer: {info['source']} (PT {info['pt_version']})",
         f"{info['counts']['devices']} thiet bi, {info['counts']['links']} lien ket. "
         f"Theo loai: " + ", ".join(f"{k} x{v}" for k, v in info["counts"]["by_type"].items()),
         "", "## Thiet bi"]
    for d in info["devices"]:
        o.append(f"- **{d['name']}** ({d['type']}" + (f" {d['model']}" if d["model"] else "") + ")")
        if d.get("gateway"):
            o.append(f"  - gateway: {d['gateway']}")
        for p in d.get("ports", []):
            bits = [f"{k}={v}" for k, v in p.items() if k != "type"]
            if bits:
                o.append(f"  - port {p.get('type', '')}: " + ", ".join(bits))
        if d.get("vlans"):
            o.append("  - VLAN: " + ", ".join(f"{v['id']}({v['name']})" for v in d["vlans"]))
        for i in d.get("interfaces", []):
            s = f"  - interface {i['name']}"
            if i.get("ip"):
                s += f": {i['ip']}" + (f" {i['mask']}" if i.get("mask") else "")
            if i.get("shutdown"):
                s += " [shutdown]"
            if i.get("other"):
                s += " | " + "; ".join(i["other"])
            o.append(s)
        for b in d.get("blocks", []):
            o.append(f"  - {b['block']}: " + "; ".join(b["lines"]) if b["lines"]
                     else f"  - {b['block']}")
        for l in d.get("config_other", []):
            o.append(f"  - cfg: {l}")
    o += ["", "## Lien ket"]
    for l in info["links"]:
        o.append(f"- {l['a']}:{l['a_port']} <-> {l['b']}:{l['b_port']} ({l['cable']})")
    for w in info.get("warnings", []):
        o.append(f"\n[!] {w}")
    return "\n".join(o) + "\n"


def tree_outline(xml, depth=4):
    """In cac duong dan the (kem so lan) - de tu doc cau truc khi PT doi dinh dang."""
    root = ET.fromstring(xml)
    counts = collections.OrderedDict()

    def walk(e, path, d):
        if d > depth:
            return
        counts[path] = counts.get(path, 0) + 1
        for c in e:
            walk(c, path + "/" + c.tag, d + 1)

    walk(root, root.tag, 0)
    return "\n".join(f"{n:>6}  {p}" for p, n in counts.items()) + "\n"


# --------------------------------------------------------------------------
# CLI
# --------------------------------------------------------------------------
def _load_xml(path):
    """Nhan .pkt hoac .xml, tra ve bytes XML."""
    data = _read(path)
    head = data[:200].lstrip()
    if head.startswith(b"<?xml") or head.startswith(b"<PACKETTRACER"):
        return data
    return pkt_to_xml(data)


def _read(p):
    with open(p, "rb") as f:
        return f.read()


def _write(p, b):
    with open(p, "wb") as f:
        f.write(b)


def main(argv=None):
    ap = argparse.ArgumentParser(prog="pkt2ai", description=__doc__.split("\n")[0])
    sub = ap.add_subparsers(dest="cmd", required=True)

    d = sub.add_parser("decode", help=".pkt -> .xml")
    d.add_argument("input")
    d.add_argument("-o", "--output")
    d.add_argument("--no-verify", action="store_true",
                   help="bo qua kiem tra tag EAX (de thu file phien ban la)")

    e = sub.add_parser("encode", help=".xml -> .pkt")
    e.add_argument("input")
    e.add_argument("-o", "--output")

    s = sub.add_parser("summary", help=".pkt/.xml -> JSON hoac Markdown gon cho AI")
    s.add_argument("input")
    s.add_argument("-o", "--output", help="mac dinh in ra man hinh")
    s.add_argument("--format", choices=["json", "md"], default="json")
    s.add_argument("--keep-secrets", action="store_true",
                   help="khong che mat khau/secret/community trong config")
    s.add_argument("--full-config", action="store_true",
                   help="kem toan bo running-config nguyen van")

    t = sub.add_parser("tree", help="in cau truc the XML (de debug khi PT doi dinh dang)")
    t.add_argument("input")
    t.add_argument("--depth", type=int, default=4)

    args = ap.parse_args(argv)
    try:
        if args.cmd == "summary":
            info = extract(_load_xml(args.input), source=args.input.replace("\\", "/").split("/")[-1],
                           redact=not args.keep_secrets, full_config=args.full_config)
            text = (json.dumps(info, ensure_ascii=False, indent=1) + "\n"
                    if args.format == "json" else to_markdown(info))
            if args.output:
                _write(args.output, text.encode("utf-8"))
                print(f"[+] {args.input} -> {args.output} ({len(text):,} ky tu)")
            else:
                sys.stdout.write(text)
        elif args.cmd == "tree":
            sys.stdout.write(tree_outline(_load_xml(args.input), args.depth))
        elif args.cmd == "decode":
            out = args.output or (args.input.rsplit(".", 1)[0] + ".xml")
            xml = pkt_to_xml(_read(args.input), verify=not args.no_verify)
            _write(out, xml)
            print(f"[+] {args.input} -> {out} ({len(xml):,} byte)")
        elif args.cmd == "encode":
            out = args.output or (args.input.rsplit(".", 1)[0] + ".pkt")
            _write(out, xml_to_pkt(_read(args.input)))
            print(f"[+] {args.input} -> {out}")
    except (ValueError, OSError, ET.ParseError) as ex:
        print(f"[-] Loi: {ex}", file=sys.stderr)
        if "EAX" in str(ex) or "zlib" in str(ex):
            print("    Goi y: neu file tu Packet Tracer 9.x, co the key/dinh dang da doi.\n"
                  "    Thu them --no-verify de bo qua kiem tra tag, va gui lai ket qua "
                  "(so byte dau file, phien ban PT) de chinh tool.", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
