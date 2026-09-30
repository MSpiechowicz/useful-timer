#![cfg_attr(
    all(target_os = "windows", not(debug_assertions)),
    windows_subsystem = "windows"
)]

mod app;
mod branding;
mod theme;
mod visuals;

use eframe::egui;

fn main() -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Useful Timer")
            .with_app_id("useful-timer")
            .with_icon(branding::icon())
            // All glow viewports share the root GL config, so request alpha here.
            // The control panel paints an opaque background itself.
            .with_transparent(true)
            .with_inner_size([1060.0, 820.0])
            .with_min_inner_size([800.0, 640.0]),
        renderer: eframe::Renderer::Glow,
        ..Default::default()
    };

    // Native Wayland cannot position these independent draggable desktop widgets.
    // X11 also works through XWayland when running a Wayland desktop session.
    #[cfg(target_os = "linux")]
    let options = {
        use winit::platform::x11::EventLoopBuilderExtX11;
        eframe::NativeOptions {
            event_loop_builder: Some(Box::new(|builder| {
                builder.with_x11();
            })),
            ..options
        }
    };

    eframe::run_native(
        "Useful Timer",
        options,
        Box::new(|context| Ok(Box::new(app::UsefulTimerApp::new(context)))),
    )
}
