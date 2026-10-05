"""What Sqooky's Blur Disabler (pak97_dir.vpk) changes, checked against the vanilla stylesheet.

Prints, for each of the two compiled stylesheets in the pak, the DATA prefix facts (the
`prefix == source_crc ^ crc32(text)` rule, with source_crc found in RED2) and a
declaration-level diff of the `base/` copy against GameTracking-Deadlock's decompiled
`citadel_base_styles.css` at a pinned commit (fetched, ~100 KB).

usage: python3 blur_base_diff.py [pak97_dir.vpk] [gametracking-sha]
"""
import difflib, os, re, struct, sys, urllib.request, zlib

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from vpkr import read, data

HERE = os.path.dirname(os.path.abspath(__file__))
PAK = sys.argv[1] if len(sys.argv) > 1 else os.path.join(
    HERE, '../../configs/OptimizationLock/Various Addons Relating to Performance/Blur Disabler/pak97_dir.vpk')
SHA = sys.argv[2] if len(sys.argv) > 2 else '58b3529cc17088df1eed5c39c6eeac72d7133b16'
GT = f'https://raw.githubusercontent.com/SteamTracking/GameTracking-Deadlock/{SHA}/game/citadel/pak01_dir/panorama/styles/citadel_base_styles.css'
STUB = 'panorama/styles/citadel_base_styles.vcss_c'
BASE = 'panorama/styles/base/citadel_base_styles.vcss_c'


def blocks(b):
    fs, hv, tv, bo, bc = struct.unpack_from('<IHHII', b, 0)
    out = {}
    for i in range(bc):
        p = 8 + bo + i * 12
        off, sz = struct.unpack_from('<II', b, p + 4)
        out[b[p:p + 4].decode()] = b[p + 4 + off:p + 4 + off + sz]
    return tv, out


def split_data(dat, tv):
    nimg = struct.unpack_from('<H', dat, 4)[0]
    p = 6
    for _ in range(nimg):
        p = dat.index(b'\0', p) + 1 + 4 + (4 if tv >= 3 else 0)
    return struct.unpack_from('<I', dat, 0)[0], nimg, dat[4:p], dat[p:]


def tokens(text):
    text = re.sub(r'/\*.*?\*/', '', text, flags=re.S)
    return [t.strip() for t in re.split(r'([{};])', text) if t.strip()]


d, ver, hdr, ts, pos, entries = read(PAK)
files = {e['path']: data(d, hdr, ts, e, PAK) for e in entries}
texts = {}
for path, b in files.items():
    tv, bl = blocks(b)
    prefix, nimg, table, text = split_data(bl['DATA'], tv)
    source = prefix ^ zlib.crc32(text)
    texts[path] = text
    print(f'{path}: {len(b)} B, version {tv}, blocks {[(k, len(v)) for k, v in bl.items()]}, images {nimg}, text {len(text)} B')
    print(f'  prefix {prefix:08x}  crc32(text) {zlib.crc32(text):08x}  crc32(table+text) {zlib.crc32(table + text):08x}'
          f'  prefix^crc32(text) {source:08x}  in RED2: {struct.pack("<I", source) in bl["RED2"]}')
    if len(text) < 300:
        print('  text:', text.decode())

vanilla = urllib.request.urlopen(GT).read().decode('utf-8')
a, b = tokens(vanilla), tokens(texts[BASE].decode('utf-8'))
ops = [op for op in difflib.SequenceMatcher(a=a, b=b, autojunk=False).get_opcodes() if op[0] != 'equal']
print(f'\nbase/ vs GameTracking @{SHA[:10]}: {len(a)} vs {len(b)} tokens, {len(ops)} differing runs')
for tag, i1, i2, j1, j2 in ops[:30]:
    print(' ', tag, 'vanilla:', a[i1:i2][:3], '| pak97 base/:', b[j1:j2][:3])
for name in ('ingameHudBlur', 'menuBlur'):
    print(f'  vanilla @define {name}:', [t for t in a if t.startswith(f'@define {name}:')])
