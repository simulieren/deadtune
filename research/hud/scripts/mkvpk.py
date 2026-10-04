"""Write single-file VPK v2 (all data embedded, archive index 0x7fff). usage: mkvpk.py out_dir.vpk vpkpath=localfile ..."""
import struct,sys,zlib,hashlib,collections
out=sys.argv[1]; files=[a.split('=',1) for a in sys.argv[2:]]
tree=collections.OrderedDict(); data=bytearray(); 
for vp,lf in sorted(files):
    b=open(lf,'rb').read()
    path,_,fname=vp.rpartition('/'); name,_,ext=fname.rpartition('.')
    tree.setdefault(ext,collections.OrderedDict()).setdefault(path or ' ',[]).append((name,zlib.crc32(b),len(data),len(b)))
    data+=b
t=bytearray()
for ext,dirs in sorted(tree.items()):
    t+=ext.encode()+b'\0'
    for d,fl in sorted(dirs.items()):
        t+=d.encode()+b'\0'
        for name,crc,off,ln in sorted(fl):
            t+=name.encode()+b'\0'+struct.pack('<IHHIIH',crc,0,0x7fff,off,ln,0xffff)
        t+=b'\0'
    t+=b'\0'
t+=b'\0'
omd=hashlib.md5(bytes(t)).digest()
hdr=struct.pack('<7I',0x55AA1234,2,len(t),len(data),0,48,0)
body=hdr+bytes(t)+bytes(data)
# other-md5 section: tree md5, archive-md5-section md5 (empty), whole-file md5 (hdr..omd tree-hash)
sec1=hashlib.md5(bytes(t)).digest(); sec2=hashlib.md5(b'').digest()
sec=sec1+sec2
whole=hashlib.md5(body+sec).digest()
open(out,'wb').write(body+sec+whole); print(out,len(body)+48)
