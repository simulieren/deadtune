import struct,sys,zlib
def read(p):
    d=open(p,'rb').read()
    sig,ver=struct.unpack_from('<II',d,0)
    assert sig==0x55AA1234
    if ver==2:
        tsize,fsd,abm,omd,sigsz=struct.unpack_from('<5I',d,8); hdr=28
    else:
        tsize=struct.unpack_from('<I',d,8)[0]; hdr=12; fsd=0
    pos=hdr; end=hdr+tsize; out=[]
    def s():
        nonlocal pos
        e=d.index(b'\0',pos); r=d[pos:e].decode(); pos=e+1; return r
    while True:
        ext=s()
        if not ext: break
        while True:
            path=s()
            if not path: break
            while True:
                name=s()
                if not name: break
                crc,pre,ai,eo,el=struct.unpack_from('<IHHII',d,pos); pos+=16
                term=struct.unpack_from('<H',d,pos)[0]; pos+=2
                preb=d[pos:pos+pre]; pos+=pre
                full=(path+'/' if path!=' ' else '')+name+'.'+ext
                out.append(dict(path=full,crc=crc,pre=pre,ai=ai,eo=eo,el=el,prebytes=preb,term=term))
    return d,ver,hdr,tsize,pos,out
def data(d,hdr,tsize,e,pdir=None):
    if e['ai']==0x7fff: return e['prebytes']+d[hdr+tsize+e['eo']:hdr+tsize+e['eo']+e['el']]
    import os,re
    f=re.sub(r'_dir\.vpk$','_%03d.vpk'%e['ai'],pdir)
    with open(f,'rb') as fh:
        fh.seek(e['eo']); return e['prebytes']+fh.read(e['el'])
if __name__=='__main__':
    d,ver,hdr,ts,pos,out=read(sys.argv[1])
    print('#',sys.argv[1],'ver',ver,'treesize',ts,'treeend',pos,'filesize',len(d),'n',len(out),'archives',sorted({e['ai'] for e in out}))
    for e in out: print(e['path'],e['ai'],e['eo'],e['el'],e['pre'],hex(e['crc']))
