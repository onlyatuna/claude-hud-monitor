//! Tests DirectComposition device loss resilience, GDI fallback, and WindowSystem self-healing.
#![cfg(windows)]

use qtrs_gui::geometry::primitives::Rect;
use qtrs_gui::paint::Pixmap;
use qtrs_gui::tiny_skia::Color;
use qtrs_platform::surface::dcomp::DCompSurface;
use qtrs_platform::surface::win32::Win32LayeredSurface;
use qtrs_platform::surface::PlatformSurface;
use qtrs_platform::window::{NativeWindow, WindowFlags};
use qtrs_platform::PlatformWindow;

#[test]
fn test_dcomp_surface_to_gdi_layered_transition_parity() {
    let window = NativeWindow::new(
        "DComp Transition Test",
        Rect::new(50, 50, 120, 80),
        WindowFlags::FRAMELESS | WindowFlags::LAYERED,
    )
    .expect("native window create");

    let hwnd = window.hwnd();

    // 1. First establish DComp surface if hardware/platform supports it
    if let Ok(mut dcomp) = DCompSurface::new(hwnd, 120, 80) {
        let mut pixmap = Pixmap::new(120, 80).expect("pixmap allocate");
        pixmap.fill(Color::from_rgba8(20, 40, 80, 220));
        let res = dcomp.present_dirty(&mut pixmap, 1.0, Rect::new(0, 0, 120, 80));
        assert!(res.is_ok(), "DComp initial present failed: {:?}", res);

        // 2. Simulate Device Lost / teardown by dropping DComp surface
        // DirectComposition target is released and unbound from hwnd during drop
        drop(dcomp);
    }

    // 3. Seamlessly create Win32LayeredSurface (GDI fallback) on the exact same HWND
    let mut gdi_surface = Win32LayeredSurface::new(hwnd, 120, 80)
        .expect("Win32LayeredSurface fallback creation failed");

    let mut pixmap_gdi = Pixmap::new(120, 80).expect("pixmap allocate");
    pixmap_gdi.fill(Color::from_rgba8(180, 50, 20, 255));
    let gdi_res = gdi_surface.present_dirty(&mut pixmap_gdi, 1.0, Rect::new(0, 0, 120, 80));
    assert!(
        gdi_res.is_ok(),
        "GDI layered surface presentation after DComp drop failed: {:?}",
        gdi_res
    );
}

#[test]
fn test_window_system_present_dirty_self_healing_recovery() {
    let mut window = NativeWindow::new(
        "Self Healing Test",
        Rect::new(100, 100, 150, 100),
        WindowFlags::FRAMELESS | WindowFlags::LAYERED,
    )
    .expect("native window create");

    let mut pixmap = Pixmap::new(150, 100).expect("pixmap");
    pixmap.fill(Color::from_rgba8(10, 100, 200, 240));

    // Initial present through WindowSystem trait
    let res = window.present_dirty(&mut pixmap, 0.9, Rect::new(0, 0, 150, 100));
    assert!(res.is_ok(), "Initial present_dirty failed: {:?}", res);

    // Present second frame with dynamic dirty rectangle
    let dirty = Rect::new(10, 10, 50, 40);
    let res2 = window.present_dirty(&mut pixmap, 0.9, dirty);
    assert!(res2.is_ok(), "Second frame dirty present failed: {:?}", res2);

    // Resize window geometry and present updated frame
    window.set_geometry(Rect::new(100, 100, 200, 120));
    let mut pixmap_resized = Pixmap::new(200, 120).expect("pixmap resized");
    pixmap_resized.fill(Color::from_rgba8(10, 150, 220, 255));
    let res3 = window.present(&mut pixmap_resized, 1.0);
    assert!(res3.is_ok(), "Resized frame present failed: {:?}", res3);
}

#[test]
fn test_get_or_create_layered_surface_error_reset_resilience() {
    let mut window = NativeWindow::new(
        "Surface Resilience Test",
        Rect::new(50, 50, 100, 80),
        WindowFlags::FRAMELESS | WindowFlags::LAYERED,
    )
    .expect("native window create");

    // 1. Get or create valid surface
    let surf = window.get_or_create_layered_surface(100, 80);
    assert!(surf.is_ok(), "Valid surface creation failed");

    // 2. Trigger invalid dimension (0, 0) resize failure
    let invalid_res = window.get_or_create_layered_surface(0, 0);
    assert!(
        invalid_res.is_err(),
        "Invalid dimension resize must return Err"
    );

    // 3. Verify internal cache was cleared and recovers cleanly on next valid request
    let recovered_surf = window.get_or_create_layered_surface(100, 80);
    assert!(
        recovered_surf.is_ok(),
        "Surface must cleanly recover after error reset"
    );
    let s = recovered_surf.unwrap();
    assert_eq!(s.width(), 100);
    assert_eq!(s.height(), 80);
}
