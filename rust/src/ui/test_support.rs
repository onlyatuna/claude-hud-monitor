//! Shared setup for tests that measure the cards-mode layout against PySide6 numbers.

/// Installs the cards-mode application sheet and the device pixel ratio the PySide6 oracle ran at
/// (`rust/tools/geometry_audit`: 1.25, DirectWrite text), and restores both on drop.
pub(crate) struct CardsOracleSetup;

impl CardsOracleSetup {
    pub(crate) fn new() -> Self {
        Self::at_dpr(1.25)
    }

    /// The same cards-mode sheet at another device pixel ratio (the oracle was also measured at 1.0).
    pub(crate) fn at_dpr(dpr: f32) -> Self {
        qtrs_gui::text::font_database::set_application_device_pixel_ratio(dpr);
        qtrs_widgets::application::Application::set_style_sheet(
            crate::ui::styles::get_cards_stylesheet(true),
        );
        Self
    }
}

impl Drop for CardsOracleSetup {
    fn drop(&mut self) {
        qtrs_widgets::application::Application::set_style_sheet("");
        qtrs_gui::text::font_database::set_application_device_pixel_ratio(1.0);
    }
}
