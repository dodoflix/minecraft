#!/usr/bin/env python3
"""Prints Data.spawn of a level.dat (gzip-compressed NBT), for scripts/world-spawn-check.sh.

Usage: scripts/level-dat-spawn.py PATH_TO_level.dat
"""
import gzip,struct,sys

if sys.argv[1:2] in (["-h"], ["--help"]):
    print(__doc__)
    sys.exit(0)
def rd(f,t):
    if t==1: return struct.unpack('>b',f.read(1))[0]
    if t==2: return struct.unpack('>h',f.read(2))[0]
    if t==3: return struct.unpack('>i',f.read(4))[0]
    if t==4: return struct.unpack('>q',f.read(8))[0]
    if t==5: return struct.unpack('>f',f.read(4))[0]
    if t==6: return struct.unpack('>d',f.read(8))[0]
    if t==7: n=struct.unpack('>i',f.read(4))[0]; return list(f.read(n))
    if t==8: n=struct.unpack('>H',f.read(2))[0]; return f.read(n).decode('utf8','replace')
    if t==9:
        et=f.read(1)[0]; n=struct.unpack('>i',f.read(4))[0]; return [rd(f,et) for _ in range(n)]
    if t==10:
        d={}
        while True:
            tt=f.read(1)[0]
            if tt==0: return d
            n=struct.unpack('>H',f.read(2))[0]; k=f.read(n).decode(); d[k]=rd(f,tt)
    if t==11: n=struct.unpack('>i',f.read(4))[0]; return [struct.unpack('>i',f.read(4))[0] for _ in range(n)]
    if t==12: n=struct.unpack('>i',f.read(4))[0]; return [struct.unpack('>q',f.read(8))[0] for _ in range(n)]
def load(p):
    f=gzip.open(p); t=f.read(1)[0]; n=struct.unpack('>H',f.read(2))[0]; f.read(n); return rd(f,t)
if __name__=='__main__':
    d=load(sys.argv[1])['Data']
    print(d.get('spawn'))
