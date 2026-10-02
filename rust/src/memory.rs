//! src/memory.rs — Process memory trimming utility
//! Mirrors Python `system/memory.py`.

use log::debug;

/// Run memory trimming: trims cold working set pages without affecting responsiveness.
pub fn trim_memory() {
    #[cfg(target_os = "windows")]
    {
        #[link(name = "kernel32")]
        extern "system" {
            fn GetCurrentProcess() -> isize;
        }
        #[link(name = "psapi")]
        extern "system" {
            fn EmptyWorkingSet(hProcess: isize) -> i32;
        }

        unsafe {
            let handle = GetCurrentProcess();
            if EmptyWorkingSet(handle) != 0 {
                debug!("[Memory] Working set trimmed successfully");
            } else {
                debug!("[Memory] EmptyWorkingSet returned 0");
            }
        }
    }
    #[cfg(target_os = "macos")]
    {
        extern "C" {
            fn malloc_zone_pressure_relief(zone: *mut std::ffi::c_void, goal: usize) -> usize;
        }
        unsafe {
            let released = malloc_zone_pressure_relief(std::ptr::null_mut(), 0);
            debug!("[Memory] malloc_zone_pressure_relief returned: {released}");
        }
    }
    #[cfg(target_os = "linux")]
    {
        extern "C" {
            fn malloc_trim(pad: usize) -> i32;
        }
        unsafe {
            let res = malloc_trim(0);
            debug!("[Memory] malloc_trim returned: {res}");
        }
    }
}
