use crate::objc_runtime::{Id, ObjcMsg, Sel};
use crate::surface::PlatformSurface;
use qtrs_gui::geometry::Rect;
use qtrs_gui::paint::Pixmap;

#[cfg(target_os = "macos")]
mod core_graphics {
    use std::ffi::c_void;

    pub type CGDataProviderRef = *mut c_void;
    pub type CGImageRef = *mut c_void;
    pub type CGColorSpaceRef = *mut c_void;

    pub const K_CG_IMAGE_ALPHA_PREMULTIPLIED_FIRST: u32 = 4;
    pub const K_CG_BITMAP_BYTE_ORDER_32_HOST: u32 = 8192;
    pub const K_CG_RENDERING_INTENT_DEFAULT: u32 = 0;

    #[link(name = "CoreGraphics", kind = "framework")]
    extern "C" {
        pub fn CGColorSpaceCreateDeviceRGB() -> CGColorSpaceRef;
        pub fn CGColorSpaceRelease(space: CGColorSpaceRef);
        pub fn CGDataProviderCreateWithData(
            info: *mut c_void,
            data: *const u8,
            size: usize,
            releaseData: *const c_void,
        ) -> CGDataProviderRef;
        pub fn CGDataProviderRelease(provider: CGDataProviderRef);
        pub fn CGImageCreate(
            width: usize,
            height: usize,
            bitsPerComponent: usize,
            bitsPerPixel: usize,
            bytesPerRow: usize,
            space: CGColorSpaceRef,
            bitmapInfo: u32,
            provider: CGDataProviderRef,
            decode: *const f64,
            shouldInterpolate: bool,
            intent: u32,
        ) -> CGImageRef;
        pub fn CGImageRelease(image: CGImageRef);
    }
}

pub struct CocoaLayerSurface {
    layer_id: usize,
    width: u32,
    height: u32,
    pixel_buffer: Vec<u8>,
}

unsafe impl Send for CocoaLayerSurface {}
unsafe impl Sync for CocoaLayerSurface {}

impl CocoaLayerSurface {
    pub fn new(layer_id: usize, width: u32, height: u32) -> Result<Self, &'static str> {
        if width == 0 || height == 0 {
            return Err("Width and height must be greater than 0");
        }

        let buffer_size = (width * height * 4) as usize;
        let pixel_buffer = vec![0u8; buffer_size];

        Ok(Self {
            layer_id,
            width,
            height,
            pixel_buffer,
        })
    }

    pub fn layer_id(&self) -> usize {
        self.layer_id
    }

    pub fn pixel_buffer(&self) -> &[u8] {
        &self.pixel_buffer
    }

    pub fn commit_to_layer(&self, opacity: f32) -> Result<(), &'static str> {
        let layer = Id(self.layer_id as *mut std::ffi::c_void);
        if layer.is_nil() {
            return Ok(());
        }

        #[cfg(target_os = "macos")]
        unsafe {
            use core_graphics::*;
            let color_space = CGColorSpaceCreateDeviceRGB();
            let provider = CGDataProviderCreateWithData(
                std::ptr::null_mut(),
                self.pixel_buffer.as_ptr(),
                self.pixel_buffer.len(),
                std::ptr::null(),
            );

            let bitmap_info = K_CG_IMAGE_ALPHA_PREMULTIPLIED_FIRST | K_CG_BITMAP_BYTE_ORDER_32_HOST;
            let cg_image = CGImageCreate(
                self.width as usize,
                self.height as usize,
                8,
                32,
                (self.width * 4) as usize,
                color_space,
                bitmap_info,
                provider,
                std::ptr::null(),
                false,
                K_CG_RENDERING_INTENT_DEFAULT,
            );

            let ca_transaction = crate::objc_runtime::Class::get("CATransaction")
                .unwrap_or(crate::objc_runtime::Class::NIL);
            ObjcMsg::send_class_0(ca_transaction, Sel::register("begin"));
            ObjcMsg::send_bool(
                Id(ca_transaction.0),
                Sel::register("setDisableActions:"),
                true,
            );

            ObjcMsg::send_id(
                layer,
                Sel::register("setContents:"),
                Id(cg_image as *mut std::ffi::c_void),
            );
            ObjcMsg::send_length(layer, Sel::register("setOpacity:"), opacity as f64);
            ObjcMsg::send_class_0(ca_transaction, Sel::register("commit"));

            if !cg_image.is_null() {
                CGImageRelease(cg_image);
            }
            if !provider.is_null() {
                CGDataProviderRelease(provider);
            }
            if !color_space.is_null() {
                CGColorSpaceRelease(color_space);
            }
        }

        #[cfg(not(target_os = "macos"))]
        {
            let _ = opacity;
            ObjcMsg::send_id(
                layer,
                Sel::register("setContents:"),
                Id(self.pixel_buffer.as_ptr() as *mut std::ffi::c_void),
            );
            ObjcMsg::send_length(layer, Sel::register("setOpacity:"), opacity as f64);
        }

        Ok(())
    }
}

impl PlatformSurface for CocoaLayerSurface {
    fn width(&self) -> u32 {
        self.width
    }

    fn height(&self) -> u32 {
        self.height
    }

    fn resize(&mut self, width: u32, height: u32) -> Result<(), &'static str> {
        if width == 0 || height == 0 {
            return Err("Width and height must be greater than 0");
        }
        if self.width == width && self.height == height {
            return Ok(());
        }

        self.width = width;
        self.height = height;
        let new_size = (width * height * 4) as usize;
        self.pixel_buffer.resize(new_size, 0);

        Ok(())
    }

    fn present(&mut self, pixmap: &mut Pixmap, opacity: f32) -> Result<(), &'static str> {
        let p_width = pixmap.physical_width();
        let p_height = pixmap.physical_height();

        if p_width != self.width || p_height != self.height {
            self.resize(p_width, p_height)?;
        }

        let src_data = pixmap.data();
        let target_len = (self.width * self.height * 4) as usize;
        let copy_len = src_data.len().min(target_len);

        self.pixel_buffer[..copy_len].copy_from_slice(&src_data[..copy_len]);

        self.commit_to_layer(opacity)?;
        Ok(())
    }

    fn present_dirty(
        &mut self,
        pixmap: &mut Pixmap,
        opacity: f32,
        dirty: Rect,
    ) -> Result<(), &'static str> {
        let full_window_rect = Rect::new(0, 0, self.width as i32, self.height as i32);
        let clipped = full_window_rect.intersected(&dirty);
        if clipped.is_empty() {
            return Ok(());
        }

        if clipped == full_window_rect {
            return self.present(pixmap, opacity);
        }

        let stride = (self.width * 4) as usize;
        let src_data = pixmap.data();
        let dirty_x = clipped.x as usize;
        let dirty_w = clipped.width as usize;
        let copy_bytes = dirty_w * 4;

        for y in clipped.y..(clipped.y + clipped.height) {
            let y = y as usize;
            let offset = y * stride + dirty_x * 4;
            if offset + copy_bytes <= self.pixel_buffer.len()
                && offset + copy_bytes <= src_data.len()
            {
                self.pixel_buffer[offset..offset + copy_bytes]
                    .copy_from_slice(&src_data[offset..offset + copy_bytes]);
            }
        }

        self.commit_to_layer(opacity)?;
        Ok(())
    }
}
