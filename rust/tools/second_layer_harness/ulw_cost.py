"""ULW cost with/without ULW_EX_NORESIZE, same window size, full vs small dirty rect."""
import ctypes, time, statistics as st, sys
from ctypes import wintypes as W
exec(open('w32_layered.py').read().split("st = {")[0].replace("VARIANT = sys.argv[1].upper() if len(sys.argv) > 1 else \"A\"","VARIANT='A'"))
screen_dc = u32.GetDC(None)
def mk(w,h):
    dc=g32.CreateCompatibleDC(screen_dc); bmi=BMI(ctypes.sizeof(BMI),w,-h,1,32,0,0,0,0,0,0); p=ctypes.c_void_p()
    bm=g32.CreateDIBSection(screen_dc,ctypes.byref(bmi),0,ctypes.byref(p),None,0); g32.SelectObject(dc,bm)
    ctypes.memset(p.value,0x80,w*h*4); return dc
class WC(ctypes.Structure):
    _fields_=[("style",W.UINT),("proc",WNDPROC),("cb",ctypes.c_int),("cw",ctypes.c_int),("inst",W.HINSTANCE),("icon",W.HICON),("cur",W.HANDLE),("bg",W.HBRUSH),("menu",W.LPCWSTR),("name",W.LPCWSTR)]
@WNDPROC
def wp(h,m,w,l): return u32.DefWindowProcW(h,m,w,l)
wc=WC(0,wp,0,0,None,None,None,None,None,"ULWC"); u32.RegisterClassW(ctypes.byref(wc))
def run(w,h,flags,dirty,n=300):
    hwnd=u32.CreateWindowExW(0x80000|8,"ULWC","x",0x80000000,100,100,w,h,None,None,None,None); u32.ShowWindow(hwnd,4)
    dc=mk(w,h); pt=W.POINT(100,100); sz=W.SIZE(w,h); src=W.POINT(0,0); bl=BLEND(0,0,255,1); d=W.RECT(*dirty)
    info=ULWI(ctypes.sizeof(ULWI),screen_dc,ctypes.pointer(pt),ctypes.pointer(sz),dc,ctypes.pointer(src),0,ctypes.pointer(bl),flags,ctypes.pointer(d))
    ts=[]; fail=0
    for _ in range(n+20):
        t=time.perf_counter(); ok=u32.UpdateLayeredWindowIndirect(hwnd,ctypes.byref(info)); ts.append((time.perf_counter()-t)*1e3); fail+= (not ok)
    u32.DestroyWindow(hwnd); ts=ts[20:]
    return st.median(ts), sorted(ts)[int(len(ts)*.9)], fail
for (w,h) in [(600,500),(1200,800),(1900,1000)]:
    for dn,dr in [("full",(0,0,w,h)),("small 100x40",(0,0,100,40))]:
        row=[]
        for fn,fl in [("ALPHA",2),("ALPHA|NORESIZE",2|8)]:
            m,p,f=run(w,h,fl,dr); row.append(f"{fn}: p50={m:.3f} p90={p:.3f} ms fail={f}")
        print(f"{w}x{h} dirty={dn:13}", " | ".join(row))
