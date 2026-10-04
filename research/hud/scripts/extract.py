import sys,os
sys.path.insert(0,'.')
import vpkr
vp,inner,out=sys.argv[1:4]
d,ver,hdr,ts,pos,ents=vpkr.read(vp)
for e in ents:
    if e['path']==inner:
        b=vpkr.data(d,hdr,ts,e,vp); open(out,'wb').write(b); print(len(b),'bytes; vpk entry offset',e['eo'],'abs file offset',hdr+ts+e['eo'],'crc',hex(e['crc']))
