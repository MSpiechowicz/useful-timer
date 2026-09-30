use std::sync::{Arc, LazyLock};

use eframe::egui;

static ICON: LazyLock<Arc<egui::IconData>> = LazyLock::new(|| {
    Arc::new(
        eframe::icon_data::from_png_bytes(include_bytes!("../assets/useful-timer.png"))
            .expect("bundled app icon must be a valid PNG"),
    )
});

pub fn icon() -> Arc<egui::IconData> {
    Arc::clone(&ICON)
}

pub fn texture(ctx: &egui::Context) -> egui::TextureHandle {
    let icon = icon();
    ctx.load_texture(
        "useful-timer-logo",
        egui::ColorImage::from_rgba_unmultiplied(
            [icon.width as usize, icon.height as usize],
            &icon.rgba,
        ),
        // The 256 px asset is displayed much smaller; mipmaps preserve edge coverage.
        egui::TextureOptions::LINEAR.with_mipmap_mode(Some(egui::TextureFilter::Linear)),
    )
}
