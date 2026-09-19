// src/bin/inspect_hwnd.rs
// cargo run --bin inspect_hwnd
// 印出所有屬於 claude-hud-monitor 視窗的 HWND 與 style bits

fn main() {
    #[cfg(target_os = "windows")]
    unsafe {
        #[link(name = "user32")]
        extern "system" {
            fn EnumWindows(
                lpEnumFunc: unsafe extern "system" fn(isize, isize) -> i32,
                lParam: isize,
            ) -> i32;
            fn GetWindowLongW(hWnd: isize, nIndex: i32) -> i32;
            fn GetWindowTextW(hWnd: isize, lpString: *mut u16, nMaxCount: i32) -> i32;
            fn GetWindowThreadProcessId(hWnd: isize, lpdwProcessId: *mut u32) -> u32;
            fn IsWindowVisible(hWnd: isize) -> i32;
        }
        #[link(name = "kernel32")]
        extern "system" {
            fn GetCurrentProcessId() -> u32;
        }

        static MY_PID: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
        MY_PID.store(GetCurrentProcessId(), std::sync::atomic::Ordering::SeqCst);

        // Find claude-hud-monitor.exe PID by name via toolhelp snapshot
        #[link(name = "kernel32")]
        extern "system" {
            fn CreateToolhelp32Snapshot(dwFlags: u32, th32ProcessID: u32) -> isize;
            fn Process32FirstW(hSnapshot: isize, lppe: *mut PROCESSENTRY32W) -> i32;
            fn Process32NextW(hSnapshot: isize, lppe: *mut PROCESSENTRY32W) -> i32;
            fn CloseHandle(hObject: isize) -> i32;
        }

        #[repr(C)]
        struct PROCESSENTRY32W {
            dw_size: u32,
            cnt_usage: u32,
            th32_process_id: u32,
            th32_default_heap_id: usize,
            th32_module_id: u32,
            cnt_threads: u32,
            th32_parent_process_id: u32,
            pc_pri_class_base: i32,
            dw_flags: u32,
            sz_exe_file: [u16; 260],
        }

        const TH32CS_SNAPPROCESS: u32 = 0x00000002;
        let snap = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0);
        let mut target_pids: Vec<u32> = Vec::new();
        if snap != -1isize {
            let mut pe = PROCESSENTRY32W {
                dw_size: std::mem::size_of::<PROCESSENTRY32W>() as u32,
                cnt_usage: 0,
                th32_process_id: 0,
                th32_default_heap_id: 0,
                th32_module_id: 0,
                cnt_threads: 0,
                th32_parent_process_id: 0,
                pc_pri_class_base: 0,
                dw_flags: 0,
                sz_exe_file: [0u16; 260],
            };
            if Process32FirstW(snap, &mut pe) != 0 {
                loop {
                    let exe = String::from_utf16_lossy(
                        &pe.sz_exe_file[..pe.sz_exe_file.iter().position(|&c| c == 0).unwrap_or(260)]
                    ).to_lowercase();
                    if exe.contains("claude-hud") {
                        println!("Found process: {} (PID={})", exe, pe.th32_process_id);
                        target_pids.push(pe.th32_process_id);
                    }
                    if Process32NextW(snap, &mut pe) == 0 { break; }
                }
            }
            CloseHandle(snap);
        }

        if target_pids.is_empty() {
            println!("No claude-hud-monitor process found. Make sure it is running.");
            return;
        }

        // Now enumerate all top-level windows
        static TARGET_PIDS: std::sync::OnceLock<Vec<u32>> = std::sync::OnceLock::new();
        TARGET_PIDS.set(target_pids).ok();

        unsafe extern "system" fn enum_cb(hwnd: isize, _: isize) -> i32 {
            let mut pid = 0u32;
            GetWindowThreadProcessId(hwnd, &mut pid);
            let pids = TARGET_PIDS.get().unwrap();
            if pids.contains(&pid) {
                let mut buf = [0u16; 256];
                let len = GetWindowTextW(hwnd, buf.as_mut_ptr(), 256);
                let title = String::from_utf16_lossy(&buf[..len as usize]);
                let style = GetWindowLongW(hwnd, -16);   // GWL_STYLE
                let exstyle = GetWindowLongW(hwnd, -20); // GWL_EXSTYLE
                let visible = IsWindowVisible(hwnd) != 0;
                println!(
                    "HWND=0x{:08X}  pid={}  vis={}  style=0x{:08X}  exstyle=0x{:08X}  title='{}'",
                    hwnd as u64, pid, visible, style as u32, exstyle as u32, title
                );
                // Decode key style bits
                let has_caption   = (style & 0x00C00000u32 as i32) != 0;
                let has_sysmenu   = (style & 0x00080000) != 0;
                let has_popup     = (style & 0x80000000u32 as i32) != 0;
                let has_minbox    = (style & 0x00020000) != 0;
                let has_maxbox    = (style & 0x00010000) != 0;
                let has_toolwin   = (exstyle & 0x00000080) != 0;
                println!(
                    "  WS_POPUP={} WS_CAPTION={} WS_SYSMENU={} WS_MINIMIZEBOX={} WS_MAXIMIZEBOX={} WS_EX_TOOLWINDOW={}",
                    has_popup, has_caption, has_sysmenu, has_minbox, has_maxbox, has_toolwin
                );
            }
            1
        }
        EnumWindows(enum_cb, 0);
    }
    #[cfg(not(target_os = "windows"))]
    println!("Windows only");
}
