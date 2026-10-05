"""Inventory of node types, flags, keys and eTypes across LaCo files."""
import collections
import sys

import kv3

for path in sys.argv[1:]:
    k = kv3.KV3(open(path, "rb").read())
    types = collections.Counter()
    flags = collections.Counter()
    keys = collections.Counter()
    etypes = collections.Counter()
    # re-walk the type stream
    t = kv3.Lane(k.types.data)
    while t.remaining():
        b = t.take("<B")
        if b & 0x80:
            flags[t.take("<B")] += 1
        if b & 0x40:
            t.take("<B")
        types[b & 0x3F] += 1

    def walk(v):
        v = kv3.unflag(v)
        if isinstance(v, dict):
            for kk, x in v.items():
                keys[kk] += 1
                if kk == "eType":
                    etypes[x] += 1
                walk(x)
        elif isinstance(v, list):
            for x in v:
                walk(x)

    walk(k.root)
    print(path.split("/")[-1], "v%d" % k.version)
    print("  types", dict(sorted(types.items())))
    print("  flags", dict(flags))
    print("  keys", dict(keys))
    print("  eTypes", dict(etypes))
    print("  first strings", k.strings[:12])
