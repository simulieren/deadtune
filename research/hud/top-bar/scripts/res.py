"""Dump a mod VPK: entries, compiled-resource blocks, DATA text of scripts and styles,
LaCo header of layouts. usage: res.py <pak_dir.vpk> <out_dir>"""
import os
import struct
import sys


def read_vpk(path):
    b = open(path, "rb").read()
    sig, ver, tree_size = struct.unpack_from("<IIi", b, 0)
    assert sig == 0x55AA1234 and ver in (1, 2), (sig, ver)
    pos = 28 if ver == 2 else 12
    data_start = pos + tree_size
    entries = {}

    def cstr():
        nonlocal pos
        end = b.index(b"\0", pos)
        s = b[pos:end].decode("utf-8", "replace")
        pos = end + 1
        return s

    while True:
        ext = cstr()
        if not ext:
            break
        while True:
            d = cstr()
            if not d:
                break
            while True:
                name = cstr()
                if not name:
                    break
                crc, preload, arch, off, length, term = struct.unpack_from("<IHHIIH", b, pos)
                pos += 18
                pre = b[pos:pos + preload]
                pos += preload
                full = f"{d}/{name}.{ext}" if d != " " else f"{name}.{ext}"
                assert arch == 0x7FFF, (full, arch)
                entries[full] = pre + b[data_start + off:data_start + off + length]
    return entries


def blocks(data):
    size, hver, tver, toff, count = struct.unpack_from("<IHHII", data, 0)
    table = 8 + toff
    out = []
    for i in range(count):
        e = table + i * 12
        name = data[e:e + 4].decode()
        rel, blen = struct.unpack_from("<II", data, e + 4)
        start = e + 4 + rel
        out.append((name, data[start:start + blen]))
    return hver, tver, out


def main():
    vpk, out_dir = sys.argv[1], sys.argv[2]
    os.makedirs(out_dir, exist_ok=True)
    for path, data in sorted(read_vpk(vpk).items()):
        print(f"== {path}: {len(data)} bytes")
        if not path.endswith("_c"):
            continue
        hver, tver, bl = blocks(data)
        print(f"   header v{hver} type v{tver}: " + " ".join(f"{n}:{len(d)}" for n, d in bl))
        d = dict(bl)
        if "DATA" in d:
            dat = d["DATA"]
            if path.endswith(".vjs_c"):
                text = dat
                print(f"   DATA starts {dat[:24]!r}")
                name = os.path.join(out_dir, os.path.basename(path).replace(".vjs_c", ".js"))
                open(name, "wb").write(text)
                print(f"   wrote {name}")
            elif path.endswith(".vxml_c"):
                print(f"   DATA {dat.hex()} (prefix {struct.unpack_from('<I', dat)[0]:08x})")
        if "LaCo" in d:
            la = d["LaCo"]
            magic = la[:4]
            fmt = la[4:20].hex()
            method, = struct.unpack_from("<I", la, 20)
            print(f"   LaCo {len(la)} bytes magic {magic!r} guid {fmt} compression {method} head {la[20:48].hex()}")
            name = os.path.join(out_dir, os.path.basename(path).replace(".vxml_c", ".laco"))
            open(name, "wb").write(la)


if __name__ == "__main__":
    main()
