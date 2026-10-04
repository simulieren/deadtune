import struct,sys,zlib
def parse(b):
    fs,hv,ver,bo,bc=struct.unpack_from('<IHHII',b,0)
    blocks=[]
    for i in range(bc):
        p=8+bo+i*12
        name=b[p:p+4].decode(); off,sz=struct.unpack_from('<II',b,p+4)
        blocks.append((name,p+4+off,sz))
    return fs,hv,ver,bo,bc,blocks
if __name__=='__main__':
    for f in sys.argv[1:]:
        b=open(f,'rb').read()
        fs,hv,ver,bo,bc,bl=parse(b)
        print(f,len(b),'fs',fs,'hv',hv,'ver',ver,'bo',bo,'bc',bc,bl)
        for n,o,s in bl:
            if n=='DATA':
                d=b[o:o+s]
                h=struct.unpack_from('<I',d,0)[0]
                print(' DATA head',d[:8].hex(),'crc(text)',hex(zlib.crc32(d[6:])),'crc(d[4:])',hex(zlib.crc32(d[4:])),'hdr',hex(h), 'len',len(d))
