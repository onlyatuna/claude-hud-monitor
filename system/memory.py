"""Cross-platform memory optimization utilities.

Safely flushes unreferenced Python cyclic garbage and trimmed OS working set
pages without affecting responsiveness or rendering performance.
"""
import gc
import logging
import sys

logger = logging.getLogger("ClaudeHUD")

_kernel32 = None
_psapi = None
_h_process = None

if sys.platform == "win32":
    try:
        import ctypes
        from ctypes import wintypes
        _k32 = ctypes.WinDLL("kernel32", use_last_error=True)
        _ps = ctypes.WinDLL("psapi", use_last_error=True)
        _k32.GetCurrentProcess.restype = wintypes.HANDLE
        _ps.EmptyWorkingSet.argtypes = [wintypes.HANDLE]
        _ps.EmptyWorkingSet.restype = wintypes.BOOL
        _kernel32 = _k32
        _psapi = _ps
        _h_process = _kernel32.GetCurrentProcess()
    except Exception as e:
        logger.debug(f"[Memory] Failed to bind Win32 memory APIs: {e}")


def trim_memory():
    """Run lightweight garbage collection and trim cold OS pages."""
    try:
        gc.collect()
        if sys.platform == "win32" and _psapi and _h_process:
            _psapi.EmptyWorkingSet(_h_process)
    except Exception as e:
        logger.debug(f"[Memory] trim_memory error: {e}")
