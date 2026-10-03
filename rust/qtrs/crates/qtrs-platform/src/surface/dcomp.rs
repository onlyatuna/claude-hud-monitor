#![allow(non_snake_case, dead_code)]
//! DirectComposition Hardware Accelerated Surface for Windows.
//!
//! Provides true differential presentation using DirectComposition and DXGI SwapChain1
//! (`Present1` with `pDirtyRects`), breaking the Win32 `UpdateLayeredWindow` full-surface
//! composition bottleneck on high-resolution screens.

use crate::surface::PlatformSurface;
use qtrs_gui::geometry::Rect;
use qtrs_gui::paint::Pixmap;
use std::ffi::c_void;
use std::ptr;
use windows_sys::core::{GUID, HRESULT};
use windows_sys::Win32::Foundation::{HMODULE, HWND, POINT, RECT};
use windows_sys::Win32::Graphics::Gdi::{
    CreateCompatibleDC, CreateDIBSection, DeleteDC, DeleteObject, SelectObject,
    BITMAPINFO, BITMAPINFOHEADER, BI_RGB, DIB_RGB_COLORS, HBITMAP, HDC, HGDIOBJ,
};
use windows_sys::Win32::System::LibraryLoader::{GetProcAddress, LoadLibraryA};

#[link(name = "kernel32")]
extern "system" {
    fn FreeLibrary(hLibModule: HMODULE) -> i32;
}
// -----------------------------------------------------------------------------
// Constants and GUIDs
// -----------------------------------------------------------------------------

const DXGI_FORMAT_B8G8R8A8_UNORM: u32 = 87;
const DXGI_USAGE_RENDER_TARGET_OUTPUT: u32 = 0x00000020;
const DXGI_SCALING_STRETCH: u32 = 0;
const DXGI_SWAP_EFFECT_FLIP_SEQUENTIAL: u32 = 3;
const DXGI_ALPHA_MODE_PREMULTIPLIED: u32 = 2;
const DXGI_SWAP_CHAIN_FLAG_GDI_COMPATIBLE: u32 = 0x200;

const D3D_DRIVER_TYPE_HARDWARE: u32 = 1;
const D3D11_CREATE_DEVICE_BGRA_SUPPORT: u32 = 0x20;
const D3D11_SDK_VERSION: u32 = 7;

const IID_IUNKNOWN: GUID = GUID {
    data1: 0x00000000,
    data2: 0x0000,
    data3: 0x0000,
    data4: [0xC0, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x46],
};

const IID_IDXGI_FACTORY2: GUID = GUID {
    data1: 0x50c83a1c,
    data2: 0xe072,
    data3: 0x4d48,
    data4: [0x87, 0xb0, 0x36, 0x30, 0xfa, 0x36, 0xa6, 0xd0],
};

const IID_IDXGI_SWAP_CHAIN1: GUID = GUID {
    data1: 0x790a45f8,
    data2: 0x0f42,
    data3: 0x4876,
    data4: [0x98, 0x52, 0x38, 0xb8, 0x2e, 0x7e, 0x1b, 0x6a],
};

const IID_IDXGI_SURFACE1: GUID = GUID {
    data1: 0x4AE63092,
    data2: 0x6327,
    data3: 0x4c1b,
    data4: [0x80, 0xAE, 0xBF, 0xE1, 0xE0, 0x84, 0x10, 0xCD],
};

const IID_IDCOMPOSITION_DEVICE: GUID = GUID {
    data1: 0xC37D25AE,
    data2: 0x2E15,
    data3: 0x4673,
    data4: [0x9F, 0x94, 0xDB, 0x76, 0x08, 0x16, 0x0F, 0xDE],
};

// -----------------------------------------------------------------------------
// DXGI / Direct3D / DirectComposition Structs
// -----------------------------------------------------------------------------

#[repr(C)]
#[derive(Clone, Copy)]
pub struct DXGI_SWAP_CHAIN_DESC1 {
    pub Width: u32,
    pub Height: u32,
    pub Format: u32,
    pub Stereo: i32,
    pub SampleDesc_Count: u32,
    pub SampleDesc_Quality: u32,
    pub BufferUsage: u32,
    pub BufferCount: u32,
    pub Scaling: u32,
    pub SwapEffect: u32,
    pub AlphaMode: u32,
    pub Flags: u32,
}

#[repr(C)]
pub struct DXGI_PRESENT_PARAMETERS {
    pub DirtyRectsCount: u32,
    pub pDirtyRects: *const RECT,
    pub pScrollRect: *const RECT,
    pub pScrollOffset: *const POINT,
}

// -----------------------------------------------------------------------------
// Function Pointer Types
// -----------------------------------------------------------------------------

type PfnD3D11CreateDevice = unsafe extern "system" fn(
    pAdapter: *mut c_void,
    DriverType: u32,
    Software: *mut c_void,
    Flags: u32,
    pFeatureLevels: *const u32,
    FeatureLevels: u32,
    SDKVersion: u32,
    ppDevice: *mut *mut c_void,
    pFeatureLevel: *mut u32,
    ppImmediateContext: *mut *mut c_void,
) -> HRESULT;

type PfnCreateDXGIFactory1 = unsafe extern "system" fn(
    riid: *const GUID,
    ppFactory: *mut *mut c_void,
) -> HRESULT;

type PfnDCompositionCreateDevice = unsafe extern "system" fn(
    dxgiDevice: *mut c_void,
    riid: *const GUID,
    pDCompositionDevice: *mut *mut c_void,
) -> HRESULT;

// -----------------------------------------------------------------------------
// COM Helper Utilities
// -----------------------------------------------------------------------------

unsafe fn com_release(ptr: *mut c_void) {
    if !ptr.is_null() {
        let vtbl = *(ptr as *mut *mut usize);
        let release_fn: unsafe extern "system" fn(*mut c_void) -> u32 =
            std::mem::transmute(*vtbl.add(2));
        release_fn(ptr);
    }
}

// -----------------------------------------------------------------------------
// DirectComposition Surface Implementation
// -----------------------------------------------------------------------------

pub struct DCompSurface {
    hwnd: HWND,
    width: u32,
    height: u32,
    d3d11_mod: HMODULE,
    dxgi_mod: HMODULE,
    dcomp_mod: HMODULE,
    d3d_device: *mut c_void,
    d3d_context: *mut c_void,
    dcomp_device: *mut c_void,
    dcomp_target: *mut c_void,
    dcomp_visual: *mut c_void,
    swap_chain: *mut c_void,
    // Staging CPU GDI DIB for format blit
    staging_dc: HDC,
    staging_bitmap: HBITMAP,
    staging_old_bitmap: HGDIOBJ,
    staging_bits: *mut u8,
    staging_w: u32,
    staging_h: u32,
}

unsafe impl Send for DCompSurface {}
unsafe impl Sync for DCompSurface {}

impl DCompSurface {
    /// Creates a hardware accelerated DirectComposition surface for `hwnd`.
    pub fn new(hwnd: HWND, width: u32, height: u32) -> Result<Self, &'static str> {
        if width == 0 || height == 0 {
            return Err("Width and height must be greater than 0");
        }

        unsafe {
            // 1. Dynamically load runtime DLLs
            let d3d11_mod = LoadLibraryA(c"d3d11.dll".as_ptr() as *const u8);
            if d3d11_mod.is_null() {
                return Err("Failed to load d3d11.dll");
            }

            let dxgi_mod = LoadLibraryA(c"dxgi.dll".as_ptr() as *const u8);
            if dxgi_mod.is_null() {
                FreeLibrary(d3d11_mod);
                return Err("Failed to load dxgi.dll");
            }

            let dcomp_mod = LoadLibraryA(c"dcomp.dll".as_ptr() as *const u8);
            if dcomp_mod.is_null() {
                FreeLibrary(dxgi_mod);
                FreeLibrary(d3d11_mod);
                return Err("Failed to load dcomp.dll (DirectComposition unsupported)");
            }

            // 2. Resolve entry points
            let d3d11_create_device_ptr = GetProcAddress(d3d11_mod, c"D3D11CreateDevice".as_ptr() as *const u8);
            let create_dxgi_factory_ptr = GetProcAddress(dxgi_mod, c"CreateDXGIFactory1".as_ptr() as *const u8);
            let dcomp_create_device_ptr = GetProcAddress(dcomp_mod, c"DCompositionCreateDevice".as_ptr() as *const u8);

            if d3d11_create_device_ptr.is_none()
                || create_dxgi_factory_ptr.is_none()
                || dcomp_create_device_ptr.is_none()
            {
                FreeLibrary(dcomp_mod);
                FreeLibrary(dxgi_mod);
                FreeLibrary(d3d11_mod);
                return Err("Failed to resolve DirectComposition entry points");
            }

            let d3d11_create_device: PfnD3D11CreateDevice = std::mem::transmute(d3d11_create_device_ptr);
            let create_dxgi_factory: PfnCreateDXGIFactory1 = std::mem::transmute(create_dxgi_factory_ptr);
            let dcomp_create_device: PfnDCompositionCreateDevice = std::mem::transmute(dcomp_create_device_ptr);

            // 3. Create Direct3D11 Device
            let mut d3d_device: *mut c_void = ptr::null_mut();
            let mut d3d_context: *mut c_void = ptr::null_mut();
            let hr = d3d11_create_device(
                ptr::null_mut(),
                D3D_DRIVER_TYPE_HARDWARE,
                ptr::null_mut(),
                D3D11_CREATE_DEVICE_BGRA_SUPPORT,
                ptr::null(),
                0,
                D3D11_SDK_VERSION,
                &mut d3d_device,
                ptr::null_mut(),
                &mut d3d_context,
            );
            if hr < 0 || d3d_device.is_null() {
                FreeLibrary(dcomp_mod);
                FreeLibrary(dxgi_mod);
                FreeLibrary(d3d11_mod);
                return Err("D3D11CreateDevice failed");
            }

            // 4. Create IDXGIFactory2
            let mut dxgi_factory2: *mut c_void = ptr::null_mut();
            let hr = create_dxgi_factory(&IID_IDXGI_FACTORY2, &mut dxgi_factory2);
            if hr < 0 || dxgi_factory2.is_null() {
                com_release(d3d_context);
                com_release(d3d_device);
                FreeLibrary(dcomp_mod);
                FreeLibrary(dxgi_mod);
                FreeLibrary(d3d11_mod);
                return Err("CreateDXGIFactory1 failed for IDXGIFactory2");
            }

            // 5. Create SwapChain for DirectComposition
            let desc = DXGI_SWAP_CHAIN_DESC1 {
                Width: width,
                Height: height,
                Format: DXGI_FORMAT_B8G8R8A8_UNORM,
                Stereo: 0,
                SampleDesc_Count: 1,
                SampleDesc_Quality: 0,
                BufferUsage: DXGI_USAGE_RENDER_TARGET_OUTPUT,
                BufferCount: 2,
                Scaling: DXGI_SCALING_STRETCH,
                SwapEffect: DXGI_SWAP_EFFECT_FLIP_SEQUENTIAL,
                AlphaMode: DXGI_ALPHA_MODE_PREMULTIPLIED,
                Flags: DXGI_SWAP_CHAIN_FLAG_GDI_COMPATIBLE,
            };

            let fac_vtbl = *(dxgi_factory2 as *mut *mut usize);
            // CreateSwapChainForComposition is slot 24 in IDXGIFactory2
            type PfnCreateSwapChainForComposition = unsafe extern "system" fn(
                this: *mut c_void,
                pDevice: *mut c_void,
                pDesc: *const DXGI_SWAP_CHAIN_DESC1,
                pRestrictToOutput: *mut c_void,
                ppSwapChain: *mut *mut c_void,
            ) -> HRESULT;
            let create_swap_chain_fn: PfnCreateSwapChainForComposition =
                std::mem::transmute(*fac_vtbl.add(24));

            let mut swap_chain: *mut c_void = ptr::null_mut();
            let hr = create_swap_chain_fn(
                dxgi_factory2,
                d3d_device,
                &desc,
                ptr::null_mut(),
                &mut swap_chain,
            );
            com_release(dxgi_factory2);

            if hr < 0 || swap_chain.is_null() {
                com_release(d3d_context);
                com_release(d3d_device);
                FreeLibrary(dcomp_mod);
                FreeLibrary(dxgi_mod);
                FreeLibrary(d3d11_mod);
                return Err("CreateSwapChainForComposition failed");
            }

            // 6. Create DirectComposition Device
            let mut dcomp_device: *mut c_void = ptr::null_mut();
            let hr = dcomp_create_device(d3d_device, &IID_IDCOMPOSITION_DEVICE, &mut dcomp_device);
            if hr < 0 || dcomp_device.is_null() {
                com_release(swap_chain);
                com_release(d3d_context);
                com_release(d3d_device);
                FreeLibrary(dcomp_mod);
                FreeLibrary(dxgi_mod);
                FreeLibrary(d3d11_mod);
                return Err("DCompositionCreateDevice failed");
            }

            // 7. Create Target for HWND
            let dev_vtbl = *(dcomp_device as *mut *mut usize);
            // CreateTargetForHwnd is slot 6 on IDCompositionDevice
            type PfnCreateTargetForHwnd = unsafe extern "system" fn(
                this: *mut c_void,
                hwnd: HWND,
                topmost: i32,
                target: *mut *mut c_void,
            ) -> HRESULT;
            let create_target_fn: PfnCreateTargetForHwnd = std::mem::transmute(*dev_vtbl.add(6));

            let mut dcomp_target: *mut c_void = ptr::null_mut();
            let hr = create_target_fn(dcomp_device, hwnd, 0, &mut dcomp_target);
            if hr < 0 || dcomp_target.is_null() {
                com_release(dcomp_device);
                com_release(swap_chain);
                com_release(d3d_context);
                com_release(d3d_device);
                FreeLibrary(dcomp_mod);
                FreeLibrary(dxgi_mod);
                FreeLibrary(d3d11_mod);
                return Err("CreateTargetForHwnd failed");
            }

            // 8. Create Visual
            // CreateVisual is slot 7 on IDCompositionDevice
            type PfnCreateVisual = unsafe extern "system" fn(
                this: *mut c_void,
                visual: *mut *mut c_void,
            ) -> HRESULT;
            let create_visual_fn: PfnCreateVisual = std::mem::transmute(*dev_vtbl.add(7));

            let mut dcomp_visual: *mut c_void = ptr::null_mut();
            let hr = create_visual_fn(dcomp_device, &mut dcomp_visual);
            if hr < 0 || dcomp_visual.is_null() {
                com_release(dcomp_target);
                com_release(dcomp_device);
                com_release(swap_chain);
                com_release(d3d_context);
                com_release(d3d_device);
                FreeLibrary(dcomp_mod);
                FreeLibrary(dxgi_mod);
                FreeLibrary(d3d11_mod);
                return Err("CreateVisual failed");
            }

            // 9. Bind visual content to swap chain: SetContent is slot 15 on IDCompositionVisual
            let vis_vtbl = *(dcomp_visual as *mut *mut usize);
            type PfnSetContent = unsafe extern "system" fn(
                this: *mut c_void,
                content: *mut c_void,
            ) -> HRESULT;
            let set_content_fn: PfnSetContent = std::mem::transmute(*vis_vtbl.add(15));
            let hr = set_content_fn(dcomp_visual, swap_chain);
            if hr < 0 {
                com_release(dcomp_visual);
                com_release(dcomp_target);
                com_release(dcomp_device);
                com_release(swap_chain);
                com_release(d3d_context);
                com_release(d3d_device);
                FreeLibrary(dcomp_mod);
                FreeLibrary(dxgi_mod);
                FreeLibrary(d3d11_mod);
                return Err("SetContent failed on visual");
            }

            // 10. Set root visual: SetRoot is slot 3 on IDCompositionTarget
            let target_vtbl = *(dcomp_target as *mut *mut usize);
            type PfnSetRoot = unsafe extern "system" fn(
                this: *mut c_void,
                visual: *mut c_void,
            ) -> HRESULT;
            let set_root_fn: PfnSetRoot = std::mem::transmute(*target_vtbl.add(3));
            let hr = set_root_fn(dcomp_target, dcomp_visual);
            if hr < 0 {
                com_release(dcomp_visual);
                com_release(dcomp_target);
                com_release(dcomp_device);
                com_release(swap_chain);
                com_release(d3d_context);
                com_release(d3d_device);
                FreeLibrary(dcomp_mod);
                FreeLibrary(dxgi_mod);
                FreeLibrary(d3d11_mod);
                return Err("SetRoot failed on target");
            }

            // 11. Commit device: Commit is slot 3 on IDCompositionDevice
            type PfnCommit = unsafe extern "system" fn(this: *mut c_void) -> HRESULT;
            let commit_fn: PfnCommit = std::mem::transmute(*dev_vtbl.add(3));
            let _ = commit_fn(dcomp_device);

            // 12. Allocate staging GDI DIB for software pixel translation
            let staging_dc = CreateCompatibleDC(ptr::null_mut());
            let mut bmi: BITMAPINFO = std::mem::zeroed();
            bmi.bmiHeader.biSize = std::mem::size_of::<BITMAPINFOHEADER>() as u32;
            bmi.bmiHeader.biWidth = width.max(1) as i32;
            bmi.bmiHeader.biHeight = -(height.max(1) as i32); // Top-down
            bmi.bmiHeader.biPlanes = 1;
            bmi.bmiHeader.biBitCount = 32;
            bmi.bmiHeader.biCompression = BI_RGB;

            let mut staging_bits: *mut c_void = ptr::null_mut();
            let staging_bitmap = CreateDIBSection(
                staging_dc,
                &bmi,
                DIB_RGB_COLORS,
                &mut staging_bits,
                ptr::null_mut(),
                0,
            );
            let staging_old_bitmap = SelectObject(staging_dc, staging_bitmap);

            Ok(Self {
                hwnd,
                width,
                height,
                d3d11_mod,
                dxgi_mod,
                dcomp_mod,
                d3d_device,
                d3d_context,
                dcomp_device,
                dcomp_target,
                dcomp_visual,
                swap_chain,
                staging_dc,
                staging_bitmap,
                staging_old_bitmap,
                staging_bits: staging_bits as *mut u8,
                staging_w: width,
                staging_h: height,
            })
        }
    }

    /// Resize DirectComposition swap chain buffers.
    pub fn resize(&mut self, width: u32, height: u32) -> Result<(), &'static str> {
        if width == 0 || height == 0 {
            return Err("Width and height must be greater than 0");
        }
        if self.width == width && self.height == height {
            return Ok(());
        }

        unsafe {
            // Resize swap chain buffers: ResizeBuffers is slot 13 on IDXGISwapChain
            let sc_vtbl = *(self.swap_chain as *mut *mut usize);
            type PfnResizeBuffers = unsafe extern "system" fn(
                this: *mut c_void,
                bufferCount: u32,
                width: u32,
                height: u32,
                newFormat: u32,
                swapChainFlags: u32,
            ) -> HRESULT;
            let resize_fn: PfnResizeBuffers = std::mem::transmute(*sc_vtbl.add(13));
            let hr = resize_fn(
                self.swap_chain,
                2,
                width,
                height,
                DXGI_FORMAT_B8G8R8A8_UNORM,
                DXGI_SWAP_CHAIN_FLAG_GDI_COMPATIBLE,
            );
            if hr < 0 {
                let u_hr = hr as u32;
                if u_hr == 0x887A0005 || u_hr == 0x887A0007 {
                    return Err("DXGI_ERROR_DEVICE_LOST");
                }
                return Err("ResizeBuffers failed on swap chain");
            }

            // Resize staging DIB
            SelectObject(self.staging_dc, self.staging_old_bitmap);
            DeleteObject(self.staging_bitmap);

            let mut bmi: BITMAPINFO = std::mem::zeroed();
            bmi.bmiHeader.biSize = std::mem::size_of::<BITMAPINFOHEADER>() as u32;
            bmi.bmiHeader.biWidth = width.max(1) as i32;
            bmi.bmiHeader.biHeight = -(height.max(1) as i32);
            bmi.bmiHeader.biPlanes = 1;
            bmi.bmiHeader.biBitCount = 32;
            bmi.bmiHeader.biCompression = BI_RGB;

            let mut bits: *mut c_void = ptr::null_mut();
            let new_bmp = CreateDIBSection(
                self.staging_dc,
                &bmi,
                DIB_RGB_COLORS,
                &mut bits,
                ptr::null_mut(),
                0,
            );
            self.staging_old_bitmap = SelectObject(self.staging_dc, new_bmp);
            self.staging_bitmap = new_bmp;
            self.staging_bits = bits as *mut u8;
            self.staging_w = width;
            self.staging_h = height;
            self.width = width;
            self.height = height;
        }

        Ok(())
    }

    pub fn present_dirty_ref(
        &mut self,
        pixmap: &Pixmap,
        _opacity: f32,
        dirty: Rect,
    ) -> Result<(), &'static str> {
        let p_width = pixmap.physical_width();
        let p_height = pixmap.physical_height();

        if p_width != self.width || p_height != self.height {
            self.resize(p_width, p_height)?;
        }

        let full_window_rect = Rect::new(0, 0, self.width as i32, self.height as i32);
        let clipped = full_window_rect.intersected(&dirty);
        if clipped.is_empty() {
            return Ok(());
        }

        unsafe {
            // 1. Copy dirty rect pixels from Pixmap (RGBA) into staging DIB (BGRA)
            let stride = (self.width * 4) as usize;
            let dirty_x = clipped.x as usize;
            let dirty_w = clipped.width as usize;
            let src_data = pixmap.data();

            for y in clipped.y..(clipped.y + clipped.height) {
                let y = y as usize;
                let row_offset = y * stride + dirty_x * 4;
                let copy_bytes = dirty_w * 4;
                let src_row = std::slice::from_raw_parts(src_data.as_ptr().add(row_offset), copy_bytes);
                let dst_row = std::slice::from_raw_parts_mut(self.staging_bits.add(row_offset), copy_bytes);

                let (src_chunks, _) = src_row.as_chunks::<4>();
                let (dst_chunks, _) = dst_row.as_chunks_mut::<4>();
                for (src_chunk, dst_chunk) in src_chunks.iter().zip(dst_chunks.iter_mut()) {
                    dst_chunk[0] = src_chunk[2]; // B
                    dst_chunk[1] = src_chunk[1]; // G
                    dst_chunk[2] = src_chunk[0]; // R
                    dst_chunk[3] = src_chunk[3]; // A
                }
            }

            // 2. Query back buffer IDXGISurface1: GetBuffer is slot 9 on IDXGISwapChain
            let sc_vtbl = *(self.swap_chain as *mut *mut usize);
            type PfnGetBuffer = unsafe extern "system" fn(
                this: *mut c_void,
                buffer: u32,
                riid: *const windows_sys::core::GUID,
                pp_surface: *mut *mut c_void,
            ) -> HRESULT;
            let get_buffer_fn: PfnGetBuffer = std::mem::transmute(*sc_vtbl.add(9));
            let iid_idxgi_surface1 = windows_sys::core::GUID {
                data1: 0x4AE63092,
                data2: 0x6327,
                data3: 0x4827,
                data4: [0x88, 0x2F, 0x20, 0x00, 0x57, 0xA3, 0xD0, 0x33],
            };
            let mut dxgi_surface1: *mut c_void = ptr::null_mut();
            let hr = get_buffer_fn(self.swap_chain, 0, &iid_idxgi_surface1, &mut dxgi_surface1);
            if hr < 0 || dxgi_surface1.is_null() {
                return Err("IDXGISwapChain::GetBuffer(0, IDXGISurface1) failed");
            }

            // 3. Acquire HDC on DXGI Surface: GetDC is slot 11 on IDXGISurface1
            let surf_vtbl = *(dxgi_surface1 as *mut *mut usize);
            type PfnGetDC = unsafe extern "system" fn(
                this: *mut c_void,
                discard: i32,
                phdc: *mut HDC,
            ) -> HRESULT;
            let get_dc_fn: PfnGetDC = std::mem::transmute(*surf_vtbl.add(11));
            let mut dxgi_dc: HDC = ptr::null_mut();
            let hr_dc = get_dc_fn(dxgi_surface1, 0, &mut dxgi_dc);
            if hr_dc < 0 || dxgi_dc.is_null() {
                let release_fn: unsafe extern "system" fn(*mut c_void) -> u32 =
                    std::mem::transmute(**(dxgi_surface1 as *mut *mut usize).add(2));
                release_fn(dxgi_surface1);
                return Err("IDXGISurface1::GetDC failed");
            }

            // 4. BitBlt differential dirty rect from staging DIB to back buffer
            windows_sys::Win32::Graphics::Gdi::BitBlt(
                dxgi_dc,
                clipped.x,
                clipped.y,
                clipped.width,
                clipped.height,
                self.staging_dc,
                clipped.x,
                clipped.y,
                windows_sys::Win32::Graphics::Gdi::SRCCOPY,
            );
            // 5. Release DC: ReleaseDC is slot 12 on IDXGISurface1
            type PfnReleaseDC = unsafe extern "system" fn(
                this: *mut c_void,
                p_dirty_rect: *const RECT,
            ) -> HRESULT;
            let rel_dc_fn: PfnReleaseDC = std::mem::transmute(*surf_vtbl.add(12));
            let dirty_gdi_rect = RECT {
                left: clipped.x,
                top: clipped.y,
                right: clipped.x + clipped.width,
                bottom: clipped.y + clipped.height,
            };
            rel_dc_fn(dxgi_surface1, &dirty_gdi_rect);

            // Release dxgi_surface1 COM reference
            let release_fn: unsafe extern "system" fn(*mut c_void) -> u32 =
                std::mem::transmute(*surf_vtbl.add(2));
            release_fn(dxgi_surface1);

            // 6. Present1 with pDirtyRects (slot 22 on IDXGISwapChain1)
            type PfnPresent1 = unsafe extern "system" fn(
                this: *mut c_void,
                sync_interval: u32,
                present_flags: u32,
                p_present_parameters: *const DXGI_PRESENT_PARAMETERS,
            ) -> HRESULT;
            let present1_fn: PfnPresent1 = std::mem::transmute(*sc_vtbl.add(22));

            let mut dxgi_dirty_rect = RECT {
                left: clipped.x,
                top: clipped.y,
                right: clipped.x + clipped.width,
                bottom: clipped.y + clipped.height,
            };
            let present_params = DXGI_PRESENT_PARAMETERS {
                DirtyRectsCount: 1,
                pDirtyRects: &mut dxgi_dirty_rect,
                pScrollRect: ptr::null_mut(),
                pScrollOffset: ptr::null_mut(),
            };

            let hr = present1_fn(self.swap_chain, 0, 0, &present_params);
            if hr < 0 {
                return Err("IDXGISwapChain1::Present1 failed");
            }

            // 7. Commit DirectComposition device
            let dev_vtbl = *(self.dcomp_device as *mut *mut usize);
            type PfnCommit = unsafe extern "system" fn(this: *mut c_void) -> HRESULT;
            let commit_fn: PfnCommit = std::mem::transmute(*dev_vtbl.add(3));
            let hr_commit = commit_fn(self.dcomp_device);
            if hr_commit < 0 {
                return Err("IDCompositionDevice::Commit failed");
            }
        }

        Ok(())
    }
}

impl PlatformSurface for DCompSurface {
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
        let full_dirty = Rect::new(0, 0, self.width as i32, self.height as i32);
        self.present_dirty(pixmap, opacity, full_dirty)
    }

    fn present_dirty(
        &mut self,
        pixmap: &mut Pixmap,
        opacity: f32,
        dirty: Rect,
    ) -> Result<(), &'static str> {
        self.present_dirty_ref(pixmap, opacity, dirty)
    }
}

impl Drop for DCompSurface {
    fn drop(&mut self) {
        unsafe {
            if !self.staging_dc.is_null() {
                SelectObject(self.staging_dc, self.staging_old_bitmap);
                DeleteObject(self.staging_bitmap);
                DeleteDC(self.staging_dc);
            }
            com_release(self.dcomp_visual);
            com_release(self.dcomp_target);
            com_release(self.dcomp_device);
            com_release(self.swap_chain);
            com_release(self.d3d_context);
            com_release(self.d3d_device);

            if !self.dcomp_mod.is_null() {
                FreeLibrary(self.dcomp_mod);
            }
            if !self.dxgi_mod.is_null() {
                FreeLibrary(self.dxgi_mod);
            }
            if !self.d3d11_mod.is_null() {
                FreeLibrary(self.d3d11_mod);
            }
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::window::{NativeWindow, WindowFlags};
    use qtrs_gui::tiny_skia::Color;

    #[test]
    fn test_dcomp_surface_creation_and_present_dirty() {
        let window = NativeWindow::new(
            "DComp Surface Test",
            Rect::new(10, 10, 100, 80),
            WindowFlags::FRAMELESS,
        )
        .expect("native window create");

        let dcomp_res = DCompSurface::new(window.hwnd(), 100, 80);
        if let Ok(mut surface) = dcomp_res {
            assert_eq!(surface.width(), 100);
            assert_eq!(surface.height(), 80);

            let mut pixmap = Pixmap::new(100, 80).unwrap();
            pixmap.fill(Color::from_rgba8(255, 100, 50, 200));

            // Test dirty present
            let dirty = Rect::new(10, 10, 40, 30);
            let res = surface.present_dirty(&mut pixmap, 1.0, dirty);
            assert!(res.is_ok(), "DComp present_dirty failed: {:?}", res.err());

            // Test resize
            let resize_res = surface.resize(120, 90);
            assert!(resize_res.is_ok(), "DComp resize failed: {:?}", resize_res.err());
            assert_eq!(surface.width(), 120);
            assert_eq!(surface.height(), 90);

            let mut pixmap2 = Pixmap::new(120, 90).unwrap();
            pixmap2.fill(Color::from_rgba8(50, 150, 250, 255));
            let res2 = surface.present_dirty(&mut pixmap2, 1.0, Rect::new(0, 0, 120, 90));
            assert!(res2.is_ok(), "DComp resized present_dirty failed: {:?}", res2.err());
        } else {
            eprintln!("DirectComposition not supported on this device/driver, skipping test");
        }
    }

    #[test]
    fn test_windows_surface_backend_negotiation() {
        use crate::surface::WindowsSurface;

        let window = NativeWindow::new(
            "Negotiation Test",
            Rect::new(20, 20, 80, 60),
            WindowFlags::FRAMELESS | WindowFlags::LAYERED,
        )
        .expect("native window create");

        let mut surface = WindowsSurface::create(window.hwnd(), 80, 60)
            .expect("WindowsSurface::create must succeed via DComp or GDI");

        assert_eq!(surface.width(), 80);
        assert_eq!(surface.height(), 60);

        let mut pixmap = Pixmap::new(80, 60).unwrap();
        pixmap.fill(Color::from_rgba8(30, 60, 90, 220));

        let res = surface.present_dirty(&mut pixmap, 0.9, Rect::new(5, 5, 30, 25));
        assert!(res.is_ok(), "WindowsSurface present_dirty failed: {:?}", res.err());
    }
}
