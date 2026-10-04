"""Clone a template .vcss_c, replace the DATA text, fix sizes. usage: mkvcss.py template out.vcss_c css.txt [--drop-srma]"""
import struct,sys
sys.path.insert(0,'..')
from ana import parse
tpl,out,cssf=sys.argv[1:4]; drop='--drop-srma' in sys.argv
b=open(tpl,'rb').read()
fs,hv,ver,bo,bc,bl=parse(b)
blk={n:b[o:o+s] for n,o,s in bl}
red2=blk['RED2']; data=blk['DATA']; srma=blk.get('SrMa')
text=open(cssf,'rb').read()
import zlib
old_hdr=struct.unpack_from('<I',data,0)[0]; old_text=data[6:]
src_crc=old_hdr^zlib.crc32(old_text)   # observed: hdr = m_nFileCRC(source) ^ crc32(text)
newdata=struct.pack('<IH',src_crc^zlib.crc32(text),0)+text         # 4-byte hash (template's, unknown algo) + u16 name count (0) + text
blocks=[('RED2',red2),('DATA',newdata)]+([] if drop or not srma else [('SrMa',srma)])
n=len(blocks); tab=16+n*12
pad=lambda x:(x+15)&~15
pos=pad(tab if tab>64 else 64) if False else 64
# observed layout: RED2 at 64 (header+table = 16+36=52 -> padded to 64); each block 16-aligned
offs=[]; p=pad(tab)
for name,bd in blocks:
    offs.append(p); p=pad(p+len(bd))
last_end=offs[-1]+len(blocks[-1][1])
buf=bytearray(last_end)
struct.pack_into('<IHHII',buf,0,last_end,hv,ver,8,n)
for i,((name,bd),o) in enumerate(zip(blocks,offs)):
    ep=16+i*12
    buf[ep:ep+4]=name.encode(); struct.pack_into('<II',buf,ep+4,o-(ep+4),len(bd))
    buf[o:o+len(bd)]=bd
open(out,'wb').write(buf); print(out,last_end,'bytes',[ (nm,o,len(bd)) for (nm,bd),o in zip(blocks,offs)])
