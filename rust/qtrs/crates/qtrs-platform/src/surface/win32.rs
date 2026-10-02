use crate::surface::PlatformSurface;
use qtrs_gui::geometry::Rect;
use qtrs_gui::paint::Pixmap;
use std::ptr;
use windows_sys::Win32::Foundation::{HWND, POINT, RECT, SIZE};
use windows_sys::Win32::Graphics::Gdi::{
    CreateCompatibleDC, CreateDIBSection, DeleteDC, DeleteObject, GetDC, ReleaseDC, SelectObject,
    AC_SRC_ALPHA, AC_SRC_OVER, BITMAPINFO, BITMAPINFOHEADER, BI_RGB, BLENDFUNCTION, DIB_RGB_COLORS,
    HBITMAP, HDC, HGDIOBJ,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    GetWindowLongPtrW, GetWindowRect, SetWindowLongPtrW, UpdateLayeredWindow,
    UpdateLayeredWindowIndirect, GWL_EXSTYLE, ULW_ALPHA, UPDATELAYEREDWINDOWINFO, WS_EX_LAYERED,
};

pub struct Win32LayeredSurface {
    hwnd: HWND,
    width: u32,
    height: u32,
    screen_dc: HDC,
    mem_dc: HDC,
    hbitmap: HBITMAP,
    old_hbitmap: HGDIOBJ,
    bits: *mut u8,
}

unsafe impl Send for Win32LayeredSurface {}
unsafe impl Sync for Win32LayeredSurface {}

impl Win32LayeredSurface {
    /// Creates layered surface for given window and dimensions.
    pub fn new(hwnd: HWND, width: u32, height: u32) -> Result<Self, &'static str> {
        if width == 0 || height == 0 {
            return Err("Width and height must be greater than 0");
        }

        unsafe {
            // Ensure window has WS_EX_LAYERED extended style for UpdateLayeredWindow
            let ex_style = GetWindowLongPtrW(hwnd, GWL_EXSTYLE);
            if (ex_style as u32 & WS_EX_LAYERED) == 0 {
                SetWindowLongPtrW(hwnd, GWL_EXSTYLE, ex_style | WS_EX_LAYERED as isize);
            }
            let screen_dc = GetDC(ptr::null_mut());
            if screen_dc.is_null() {
                return Err("Failed to get screen DC (GetDC)");
            }

            let mem_dc = CreateCompatibleDC(screen_dc);
            if mem_dc.is_null() {
                ReleaseDC(ptr::null_mut(), screen_dc);
                return Err("Failed to create compatible DC (CreateCompatibleDC)");
            }

            let mut bmi: BITMAPINFO = std::mem::zeroed();
            bmi.bmiHeader.biSize = std::mem::size_of::<BITMAPINFOHEADER>() as u32;
            bmi.bmiHeader.biWidth = width.max(1) as i32;
            bmi.bmiHeader.biHeight = -(height.max(1) as i32); // Top-down
            bmi.bmiHeader.biPlanes = 1;
            bmi.bmiHeader.biBitCount = 32;
            bmi.bmiHeader.biCompression = BI_RGB;

            let mut bits: *mut core::ffi::c_void = ptr::null_mut();
            let hbitmap =
                CreateDIBSection(mem_dc, &bmi, DIB_RGB_COLORS, &mut bits, ptr::null_mut(), 0);

            if hbitmap.is_null() || bits.is_null() {
                ReleaseDC(ptr::null_mut(), screen_dc);
                DeleteDC(mem_dc);
                return Err("CreateDIBSection failed to create layered bitmap");
            }

            let old_hbitmap = SelectObject(mem_dc, hbitmap);

            Ok(Self {
                hwnd,
                width,
                height,
                screen_dc,
                mem_dc,
                hbitmap,
                old_hbitmap,
                bits: bits as *mut u8,
            })
        }
    }

    #[inline]
    pub fn width(&self) -> u32 {
        self.width
    }

    #[inline]
    pub fn height(&self) -> u32 {
        self.height
    }

    #[inline]
    pub fn hdc(&self) -> HDC {
        self.mem_dc
    }

    #[inline]
    pub fn bits(&self) -> *mut u8 {
        self.bits
    }

    #[inline]
    pub fn buffer(&self) -> &[u8] {
        let len = (self.width * self.height * 4) as usize;
        unsafe { std::slice::from_raw_parts(self.bits, len) }
    }

    #[inline]
    pub fn buffer_mut(&mut self) -> &mut [u8] {
        let len = (self.width * self.height * 4) as usize;
        unsafe { std::slice::from_raw_parts_mut(self.bits, len) }
    }

    pub fn resize(&mut self, width: u32, height: u32) -> Result<(), &'static str> {
        if width == 0 || height == 0 {
            return Err("Width and height must be greater than 0");
        }
        if self.width == width && self.height == height {
            return Ok(());
        }

        unsafe {
            let mut bmi: BITMAPINFO = std::mem::zeroed();
            bmi.bmiHeader.biSize = std::mem::size_of::<BITMAPINFOHEADER>() as u32;
            bmi.bmiHeader.biWidth = width.max(1) as i32;
            bmi.bmiHeader.biHeight = -(height.max(1) as i32); // Top-down
            bmi.bmiHeader.biPlanes = 1;
            bmi.bmiHeader.biBitCount = 32;
            bmi.bmiHeader.biCompression = BI_RGB;

            let mut new_bits: *mut core::ffi::c_void = ptr::null_mut();
            let new_hbitmap = CreateDIBSection(
                self.mem_dc,
                &bmi,
                DIB_RGB_COLORS,
                &mut new_bits,
                ptr::null_mut(),
                0,
            );

            if new_hbitmap.is_null() || new_bits.is_null() {
                return Err("CreateDIBSection failed during resize");
            }

            SelectObject(self.mem_dc, new_hbitmap);
            DeleteObject(self.hbitmap);

            self.width = width;
            self.height = height;
            self.hbitmap = new_hbitmap;
            self.bits = new_bits as *mut u8;
        }

        Ok(())
    }

    pub fn present(&mut self, pixmap: &mut Pixmap, opacity: f32) -> Result<(), &'static str> {
        let p_width = pixmap.physical_width();
        let p_height = pixmap.physical_height();

        if p_width != self.width || p_height != self.height {
            self.resize(p_width, p_height)?;
        }

        pixmap.convert_to_bgra_in_place();

        unsafe {
            let src_data = pixmap.data();
            std::ptr::copy_nonoverlapping(
                src_data.as_ptr(),
                self.bits,
                (p_width * p_height * 4) as usize,
            );
        }

        unsafe {
            let blend = BLENDFUNCTION {
                BlendOp: AC_SRC_OVER as u8,
                BlendFlags: 0,
                SourceConstantAlpha: (opacity.clamp(0.0, 1.0) * 255.0).round() as u8,
                AlphaFormat: AC_SRC_ALPHA as u8,
            };

            let mut pt_dst = POINT { x: 0, y: 0 };
            let mut win_rect: RECT = std::mem::zeroed();
            GetWindowRect(self.hwnd, &mut win_rect);
            pt_dst.x = win_rect.left;
            pt_dst.y = win_rect.top;

            let size = SIZE {
                cx: self.width as i32,
                cy: self.height as i32,
            };
            let pt_src = POINT { x: 0, y: 0 };

            let res = UpdateLayeredWindow(
                self.hwnd,
                ptr::null_mut(),
                &pt_dst,
                &size,
                self.mem_dc,
                &pt_src,
                0,
                &blend,
                ULW_ALPHA,
            );
            if res == 0 {
                let err = windows_sys::Win32::Foundation::GetLastError();
                eprintln!("[Win32Surface] UpdateLayeredWindow failed, GetLastError = {}", err);
                return Err("UpdateLayeredWindow call failed");
            }

            pixmap.convert_to_bgra_in_place();

            if res == 0 {
                let err = windows_sys::Win32::Foundation::GetLastError();
                eprintln!("[Win32Surface] UpdateLayeredWindow failed, GetLastError = {}", err);
                return Err("UpdateLayeredWindow call failed");
            }
        }

        Ok(())
    }

    pub fn present_dirty(
        &mut self,
        pixmap: &mut Pixmap,
        opacity: f32,
        dirty: Rect,
    ) -> Result<(), &'static str> {
        let p_width = pixmap.physical_width();
        let p_height = pixmap.physical_height();

        if p_width != self.width || p_height != self.height {
            self.resize(p_width, p_height)?;
        }

        let full_window_rect = Rect::new(0, 0, self.width as i32, self.height as i32);
        let clipped_dirty = full_window_rect.intersected(&dirty);

        if clipped_dirty.is_empty() {
            return Ok(());
        }

        if clipped_dirty == full_window_rect {
            return self.present(pixmap, opacity);
        }
        let stride = (self.width * 4) as usize;
        let dirty_x = clipped_dirty.x as usize;
        let dirty_w = clipped_dirty.width as usize;

        unsafe {
            let src_data = pixmap.data();
            for y in clipped_dirty.y..(clipped_dirty.y + clipped_dirty.height) {
                let y = y as usize;
                let row_offset = y * stride + dirty_x * 4;
                let copy_bytes = dirty_w * 4;
                let src_row = std::slice::from_raw_parts(src_data.as_ptr().add(row_offset), copy_bytes);
                let dst_row = std::slice::from_raw_parts_mut(self.bits.add(row_offset), copy_bytes);

                for (src_chunk, dst_chunk) in src_row.chunks_exact(4).zip(dst_row.chunks_exact_mut(4)) {
                    dst_chunk[0] = src_chunk[2]; // B
                    dst_chunk[1] = src_chunk[1]; // G
                    dst_chunk[2] = src_chunk[0]; // R
                    dst_chunk[3] = src_chunk[3]; // A
                }
            }
        }
        unsafe {
            let blend = BLENDFUNCTION {
                BlendOp: AC_SRC_OVER as u8,
                BlendFlags: 0,
                SourceConstantAlpha: (opacity.clamp(0.0, 1.0) * 255.0).round() as u8,
                AlphaFormat: AC_SRC_ALPHA as u8,
            };

            let mut pt_dst = POINT { x: 0, y: 0 };
            let mut win_rect: RECT = std::mem::zeroed();
            GetWindowRect(self.hwnd, &mut win_rect);
            pt_dst.x = win_rect.left;
            pt_dst.y = win_rect.top;

            let size = SIZE {
                cx: self.width as i32,
                cy: self.height as i32,
            };
            let pt_src = POINT { x: 0, y: 0 };

            let mut dirty_win_rect = RECT {
                left: clipped_dirty.x,
                top: clipped_dirty.y,
                right: clipped_dirty.x + clipped_dirty.width,
                bottom: clipped_dirty.y + clipped_dirty.height,
            };

            let mut info: UPDATELAYEREDWINDOWINFO = std::mem::zeroed();
            info.cbSize = std::mem::size_of::<UPDATELAYEREDWINDOWINFO>() as u32;
            info.hdcDst = self.screen_dc;
            info.pptDst = &pt_dst;
            info.psize = &size;
            info.hdcSrc = self.mem_dc;
            info.pptSrc = &pt_src;
            info.crKey = 0;
            info.pblend = &blend;
            info.dwFlags = ULW_ALPHA;
            info.prcDirty = &mut dirty_win_rect;

            let res = UpdateLayeredWindowIndirect(self.hwnd, &info);
            if res == 0 {
                let err = windows_sys::Win32::Foundation::GetLastError();
                eprintln!("[Win32Surface] UpdateLayeredWindowIndirect failed (GetLastError = {}), falling back to full present", err);
                return self.present(pixmap, opacity);
            }
        }

        Ok(())
    }
}

impl PlatformSurface for Win32LayeredSurface {
    fn width(&self) -> u32 {
        self.width
    }

    fn height(&self) -> u32 {
        self.height
    }

    fn resize(&mut self, width: u32, height: u32) -> Result<(), &'static str> {
        self.resize(width, height)
    }

    fn present(&mut self, pixmap: &mut Pixmap, opacity: f32) -> Result<(), &'static str> {
        self.present(pixmap, opacity)
    }

    fn present_dirty(
        &mut self,
        pixmap: &mut Pixmap,
        opacity: f32,
        dirty: Rect,
    ) -> Result<(), &'static str> {
        self.present_dirty(pixmap, opacity, dirty)
    }
}

impl Drop for Win32LayeredSurface {
    fn drop(&mut self) {
        unsafe {
            if !self.mem_dc.is_null() && !self.old_hbitmap.is_null() {
                SelectObject(self.mem_dc, self.old_hbitmap);
            }
            if !self.hbitmap.is_null() {
                DeleteObject(self.hbitmap);
            }
            if !self.mem_dc.is_null() {
                DeleteDC(self.mem_dc);
            }
            if !self.screen_dc.is_null() {
                ReleaseDC(ptr::null_mut(), self.screen_dc);
            }
        }
    }
}
