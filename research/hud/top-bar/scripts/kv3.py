"""Binary KV3 (v4/v5, LZ4 or uncompressed) reader and Panorama layout printer.
usage: kv3.py <file.laco> [--kv3]   (research only; mirrors VRF BinaryKV3.cs)"""
import struct
import sys


def lz4_block_decode(src, out_len):
    out = bytearray()
    i = 0
    n = len(src)
    while i < n:
        token = src[i]
        i += 1
        lit = token >> 4
        if lit == 15:
            while True:
                b = src[i]
                i += 1
                lit += b
                if b != 255:
                    break
        out += src[i:i + lit]
        i += lit
        if i >= n:
            break
        offset = src[i] | (src[i + 1] << 8)
        i += 2
        mlen = (token & 0xF) + 4
        if (token & 0xF) == 15:
            while True:
                b = src[i]
                i += 1
                mlen += b
                if b != 255:
                    break
        start = len(out) - offset
        for k in range(mlen):
            out.append(out[start + k])
    assert len(out) == out_len, (len(out), out_len)
    return bytes(out)


def align(n, a):
    return (n + a - 1) & ~(a - 1)


class Lane:
    def __init__(self, data):
        self.data = data
        self.pos = 0

    def take(self, fmt):
        v = struct.unpack_from(fmt, self.data, self.pos)[0]
        self.pos += struct.calcsize(fmt)
        return v

    def remaining(self):
        return len(self.data) - self.pos


FLAGS = {1: "resource", 2: "resource_name", 8: "panorama", 16: "soundevent", 32: "subclass"}


class KV3:
    def __init__(self, blob):
        self.blob = blob
        self.parse()

    def parse(self):
        b = self.blob
        magic = struct.unpack_from("<I", b, 0)[0]
        self.version = magic & 0xFF
        assert (magic & 0xFFFFFF00) == 0x4B563300, hex(magic)
        v = self.version
        self.guid = b[4:20]
        pos = 20
        (method, dict_id, frame_size, c1, c4, c8, ctypes, cobj, carr, unc_total, comp_total,
         cblocks, blob_bytes) = struct.unpack_from("<IHHiiiiHHiiii", b, pos)
        pos += struct.calcsize("<IHHiiiiHHiiii")
        c2 = sbcs = 0
        if v >= 4:
            c2, sbcs = struct.unpack_from("<ii", b, pos)
            pos += 8
        if v >= 5:
            (u1, cp1, u2, cp2, b2c1, b2c2, b2c4, b2c8, cnodes, b2cobj, b2carr, celem) = struct.unpack_from("<12i", b, pos)
            pos += 48
        else:
            u1, cp1 = unc_total, comp_total
        self.header = dict(version=v, method=method, frame_size=frame_size, c1=c1, c2=c2, c4=c4, c8=c8,
                           ctypes=ctypes, cobj=cobj, carr=carr, unc_total=unc_total, comp_total=comp_total,
                           cblocks=cblocks, blob_bytes=blob_bytes)
        if v >= 5:
            self.header.update(u1=u1, cp1=cp1, u2=u2, cp2=cp2, b2c1=b2c1, b2c2=b2c2, b2c4=b2c4, b2c8=b2c8,
                               cnodes=cnodes, b2cobj=b2cobj, b2carr=b2carr, celem=celem)
        assert cblocks == 0, "binary blobs not handled"

        def read_buf(unc, comp):
            nonlocal pos
            if method == 0:
                data = b[pos:pos + unc]
                pos += unc
            elif method == 1:
                data = lz4_block_decode(b[pos:pos + comp], unc)
                pos += comp
            else:
                raise ValueError("zstd")
            return data

        buf1 = read_buf(u1, cp1)
        off = 0
        lanes1 = {}
        if c1:
            lanes1[1] = Lane(buf1[off:off + c1]); off += c1
        if c2:
            off = align(off, 2); lanes1[2] = Lane(buf1[off:off + c2 * 2]); off += c2 * 2
        if c4:
            off = align(off, 4); lanes1[4] = Lane(buf1[off:off + c4 * 4]); off += c4 * 4
        if c8:
            off = align(off, 8); lanes1[8] = Lane(buf1[off:off + c8 * 8]); off += c8 * 8
        elif v < 5:
            off = align(off, 8)
        nstrings = lanes1[4].take("<i")
        self.strings = []
        if v >= 5:
            sl = lanes1[1]
            for _ in range(nstrings):
                end = sl.data.index(b"\0", sl.pos)
                self.strings.append(sl.data[sl.pos:end].decode("utf-8"))
                sl.pos = end + 1
            assert len(buf1) == off, (len(buf1), off)
            buf2 = read_buf(u2, cp2)
            off = 0
            end = b2cobj * 4
            self.objlens = Lane(buf2[:end]); off = end
            lanes = {}
            if b2c1:
                lanes[1] = Lane(buf2[off:off + b2c1]); off += b2c1
            if b2c2:
                off = align(off, 2); lanes[2] = Lane(buf2[off:off + b2c2 * 2]); off += b2c2 * 2
            if b2c4:
                off = align(off, 4); lanes[4] = Lane(buf2[off:off + b2c4 * 4]); off += b2c4 * 4
            if b2c8:
                off = align(off, 8); lanes[8] = Lane(buf2[off:off + b2c8 * 8]); off += b2c8 * 8
            self.types = Lane(buf2[off:off + ctypes]); off += ctypes
            trailer = struct.unpack_from("<I", buf2, off)[0]
            assert trailer == 0xFFEEDD00, hex(trailer)
            off += 4
            assert len(buf2) == off + sbcs, (len(buf2), off, sbcs)
            self.lanes = lanes
            self.aux = lanes1
        else:
            strings_start = off
            for _ in range(nstrings):
                end = buf1.index(b"\0", off)
                self.strings.append(buf1[off:end].decode("utf-8"))
                off = end + 1
            tlen = ctypes - off + strings_start
            self.types = Lane(buf1[off:off + tlen]); off += tlen
            trailer = struct.unpack_from("<I", buf1, off)[0]
            assert trailer == 0xFFEEDD00, hex(trailer)
            off += 4
            assert len(buf1) == off, (len(buf1), off)
            self.lanes = lanes1
            self.aux = lanes1
            self.objlens = lanes1[4]
        self.lane_pos_at_root = {k: l.pos for k, l in self.lanes.items()}
        t, f = self.read_type()
        self.root = self.read_value(t, f, self.lanes)
        assert self.types.remaining() == 0
        for k, l in self.lanes.items():
            assert l.remaining() == 0, (k, l.remaining())

    def read_type(self):
        t = self.types.take("<B")
        flag = 0
        if t & 0x80:
            flag = self.types.take("<B")
        if t & 0x40:
            self.types.take("<B")
        return t & 0x3F, flag

    def string(self, i):
        return self.strings[i] if 0 <= i < len(self.strings) else ""

    def read_value(self, t, flag, lane):
        L = self.lanes
        if t == 1:
            v = None
        elif t == 13:
            v = True
        elif t == 14:
            v = False
        elif t == 15:
            v = 0
        elif t == 16:
            v = 1
        elif t == 17:
            v = 0.0
        elif t == 18:
            v = 1.0
        elif t == 2:
            v = lane[1].take("<B") != 0
        elif t == 22:
            v = lane[1].take("<b")
        elif t == 23:
            v = lane[1].take("<B")
        elif t == 20:
            v = lane[2].take("<h")
        elif t == 21:
            v = lane[2].take("<H")
        elif t == 11:
            v = lane[4].take("<i")
        elif t == 12:
            v = lane[4].take("<I")
        elif t == 19:
            v = lane[4].take("<f")
        elif t == 3:
            v = lane[8].take("<q")
        elif t == 4:
            v = lane[8].take("<Q")
        elif t == 5:
            v = lane[8].take("<d")
        elif t == 6:
            v = self.string(L[4].take("<i"))
        elif t == 8:
            n = L[4].take("<i")
            v = []
            for _ in range(n):
                st, sf = self.read_type()
                v.append(self.read_value(st, sf, L))
        elif t in (10, 24, 25):
            n = L[4].take("<i") if t == 10 else L[1].take("<B")
            st, sf = self.read_type()
            elane = self.aux if t == 25 else L
            v = [self.read_value(st, sf, elane) for _ in range(n)]
        elif t == 9:
            n = self.objlens.take("<i") if self.version >= 5 else L[4].take("<i")
            v = {}
            for _ in range(n):
                st, sf = self.read_type()
                name = self.string(L[4].take("<i"))
                v[name] = self.read_value(st, sf, L)
        else:
            raise ValueError(f"type {t}")
        if flag:
            return ("flag", FLAGS.get(flag, flag), v)
        return v


def unflag(v):
    return v[2] if isinstance(v, tuple) else v


def print_layout(root, out):
    ast = root["m_AST"]["m_pRoot"]

    def sub(node):
        if "vecChildren" in node:
            return node["vecChildren"]
        if "child" in node:
            return [node["child"]]
        return []

    def attr_value(val):
        t = val["eType"]
        name = unflag(val.get("name", ""))
        if t == "REFERENCE_COMPILED":
            return "s2r://" + name
        if t == "REFERENCE_PASSTHROUGH":
            return "file://" + name
        return name

    def node(n, depth):
        pad = "  " * depth
        t = n["eType"]
        kids = sub(n)
        attrs = [k for k in kids if k.get("eType") == "PANEL_ATTRIBUTE"]
        children = [k for k in kids if k.get("eType") != "PANEL_ATTRIBUTE"]
        if t == "INCLUDE":
            out.append(f'{pad}<include src="{attr_value(n["child"])}" />')
            return
        if t == "SCRIPT_BODY":
            out.append(f"{pad}<script><![CDATA[{unflag(n.get('name'))}]]></script>")
            return
        tag = {"ROOT": "root", "STYLES": "styles", "SCRIPTS": "scripts", "SNIPPETS": "snippets"}.get(t)
        if t == "PANEL":
            tag = unflag(n["name"])
        if t == "SNIPPET":
            tag = f'snippet name="{unflag(n["name"])}"'
        a = "".join(f' {unflag(x["name"])}="{attr_value(x["child"])}"' for x in attrs)
        if not children:
            out.append(f"{pad}<{tag}{a} />")
            return
        out.append(f"{pad}<{tag}{a}>")
        for c in children:
            node(c, depth + 1)
        out.append(f"{pad}</{tag.split(' ')[0]}>")

    node(ast, 0)


def dump(v, depth=0, out=None):
    pad = "  " * depth
    if isinstance(v, tuple):
        out.append(f"{pad}<{v[1]}>")
        dump(v[2], depth, out)
    elif isinstance(v, dict):
        out.append(pad + "{")
        for k, x in v.items():
            if isinstance(x, (dict, list)) or isinstance(x, tuple):
                out.append(f"{pad}  {k} =")
                dump(x, depth + 2, out)
            else:
                out.append(f"{pad}  {k} = {x!r}")
        out.append(pad + "}")
    elif isinstance(v, list):
        out.append(pad + "[")
        for x in v:
            dump(x, depth + 1, out)
        out.append(pad + "]")
    else:
        out.append(f"{pad}{v!r}")


if __name__ == "__main__":
    kv = KV3(open(sys.argv[1], "rb").read())
    print("# header", kv.header)
    print("# strings", len(kv.strings))
    out = []
    if "--kv3" in sys.argv:
        dump(kv.root, 0, out)
    else:
        print_layout(kv.root, out)
    print("\n".join(out))
