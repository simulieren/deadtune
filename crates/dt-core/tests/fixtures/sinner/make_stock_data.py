"""Regenerates stock_model_data.kv3: the upstream Sinner's Sacrifice model DATA block put back
the way the stock game file most likely stores it (LZ4, frame size 16384, like every other
KV3 block in that model), with the LOD fields restored to what the model's RED2 records:
m_refLODGroupMasks [1, 2, 4] and m_lodGroupSwitchDistances [0, 12, 25].

Usage: python3 make_stock_data.py   (needs `pip install lz4`; run from this directory)
"""
import struct
import sys

import lz4.block

sys.path.insert(0, '../../../../../research/hud/scripts')
import vpkr  # noqa: E402

PAK = '../../../../../research/configs/OptimizationLock/Various Addons Relating to Performance/Sinner Light Fix Mod/pak26_dir.vpk'
MODEL = 'models/props_gameplay/sinners_sacrifice_vault/sinners_sacrifice.vmdl_c'

d, ver, hdr, ts, pos, out = vpkr.read(PAK)
model = vpkr.data(d, hdr, ts, next(e for e in out if e['path'] == MODEL))
toff, count = struct.unpack_from('<II', model, 8)
for i in range(count):
    e = 8 + toff + 12 * i
    if model[e:e + 4] == b'DATA':
        off, size = struct.unpack_from('<II', model, e + 4)
        data = model[e + 4 + off:e + 4 + off + size]

HEADER = 120
u1, u2 = struct.unpack_from('<i', data, 72)[0], struct.unpack_from('<i', data, 80)[0]
buf1 = data[HEADER:HEADER + u1]
buf2 = bytearray(data[HEADER + u1:HEADER + u1 + u2])
# Offsets found by walking the KV3 (see native_sinner.rs); asserted so a different input fails loudly.
assert struct.unpack_from('<III', buf2, 152) == (7, 0, 0)
assert struct.unpack_from('<dd', buf2, 2040) == (1e6, 1e6)
struct.pack_into('<III', buf2, 152, 1, 2, 4)
struct.pack_into('<dd', buf2, 2040, 12.0, 25.0)

c1 = lz4.block.compress(bytes(buf1), store_size=False, mode='high_compression', compression=9)
c2 = lz4.block.compress(bytes(buf2), store_size=False, mode='high_compression', compression=9)
head = bytearray(data[:HEADER])
struct.pack_into('<I', head, 20, 1)
struct.pack_into('<HH', head, 24, 0, 16384)
struct.pack_into('<i', head, 52, len(c1) + len(c2))
struct.pack_into('<i', head, 76, len(c1))
struct.pack_into('<i', head, 84, len(c2))
with open('stock_model_data.kv3', 'wb') as f:
    f.write(bytes(head) + c1 + c2)
print('wrote', HEADER + len(c1) + len(c2), 'bytes')
