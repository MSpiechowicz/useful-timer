use eframe::egui::{self, Color32};
use serde::{Deserialize, Serialize};

pub const STORAGE_KEY: &str = "useful-timer.theme";

#[derive(Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Theme {
    Charcoal,
    #[default]
    GitHub,
}

pub struct Palette {
    pub background: Color32,
    pub sidebar: Color32,
    pub surface: Color32,
    pub raised: Color32,
    pub hovered: Color32,
    pub border: Color32,
    pub text: Color32,
    pub muted: Color32,
    pub accent: Color32,
    pub primary: Color32,
    pub primary_text: Color32,
    pub selected: Color32,
    pub success: Color32,
    pub danger: Color32,
    pub warning: Color32,
    pub warning_background: Color32,
}

const CHARCOAL: Palette = Palette {
    background: Color32::from_rgb(25, 27, 26),
    sidebar: Color32::from_rgb(30, 32, 30),
    surface: Color32::from_rgb(36, 39, 36),
    raised: Color32::from_rgb(47, 50, 46),
    hovered: Color32::from_rgb(65, 69, 60),
    border: Color32::from_rgb(62, 66, 59),
    text: Color32::from_rgb(242, 240, 233),
    muted: Color32::from_rgb(163, 169, 157),
    accent: Color32::from_rgb(233, 188, 120),
    primary: Color32::from_rgb(233, 188, 120),
    primary_text: Color32::from_rgb(25, 27, 26),
    selected: Color32::from_rgb(47, 50, 46),
    success: Color32::from_rgb(163, 196, 147),
    danger: Color32::from_rgb(239, 151, 136),
    warning: Color32::from_rgb(233, 188, 120),
    warning_background: Color32::from_rgb(67, 48, 32),
};

// Lighter blue for text, deeper blue for white-on-blue actions.
const GITHUB: Palette = Palette {
    background: Color32::from_rgb(13, 17, 23),
    sidebar: Color32::from_rgb(1, 4, 9),
    surface: Color32::from_rgb(22, 27, 34),
    raised: Color32::from_rgb(33, 40, 48),
    hovered: Color32::from_rgb(48, 54, 61),
    border: Color32::from_rgb(61, 68, 77),
    text: Color32::from_rgb(240, 246, 252),
    muted: Color32::from_rgb(177, 186, 196),
    accent: Color32::from_rgb(88, 166, 255),
    primary: Color32::from_rgb(31, 111, 235),
    primary_text: Color32::WHITE,
    selected: Color32::from_rgb(25, 47, 73),
    success: Color32::from_rgb(63, 185, 80),
    danger: Color32::from_rgb(255, 123, 114),
    warning: Color32::from_rgb(210, 153, 34),
    warning_background: Color32::from_rgb(52, 38, 11),
};

impl Theme {
    pub fn label(self) -> &'static str {
        match self {
            Self::Charcoal => "Charcoal",
            Self::GitHub => "GitHub",
        }
    }

    pub fn palette(self) -> &'static Palette {
        match self {
            Self::Charcoal => &CHARCOAL,
            Self::GitHub => &GITHUB,
        }
    }

    pub fn apply(self, ctx: &egui::Context) {
        // Both app palettes are dark; OS theme changes must not select an unstyled light style.
        ctx.set_theme(egui::Theme::Dark);
        let colors = self.palette();
        let mut visuals = egui::Visuals::dark();
        visuals.panel_fill = colors.background;
        visuals.override_text_color = Some(colors.text);
        visuals.weak_text_color = Some(colors.muted);
        visuals.window_fill = colors.surface;
        visuals.window_stroke = egui::Stroke::new(1.0, colors.border);
        visuals.hyperlink_color = colors.accent;
        visuals.warn_fg_color = colors.warning;
        visuals.error_fg_color = colors.danger;
        visuals.text_cursor.stroke = egui::Stroke::new(2.0, colors.accent);
        visuals.extreme_bg_color = colors.background;
        visuals.faint_bg_color = colors.surface;
        visuals.selection.bg_fill = colors.selected;
        visuals.selection.stroke = egui::Stroke::new(1.0, colors.accent);
        visuals.slider_trailing_fill = true;
        visuals.widgets.noninteractive.bg_stroke = egui::Stroke::new(1.0, colors.border);
        visuals.widgets.noninteractive.bg_fill = colors.surface;
        visuals.widgets.noninteractive.weak_bg_fill = colors.surface;
        visuals.widgets.noninteractive.fg_stroke = egui::Stroke::new(1.0, colors.text);
        for widget in [
            &mut visuals.widgets.inactive,
            &mut visuals.widgets.hovered,
            &mut visuals.widgets.active,
            &mut visuals.widgets.open,
        ] {
            widget.corner_radius = egui::CornerRadius::same(8);
            widget.fg_stroke = egui::Stroke::new(1.0, colors.text);
            widget.bg_stroke = egui::Stroke::new(1.0, colors.border);
            widget.bg_fill = colors.raised;
            widget.weak_bg_fill = colors.raised;
            widget.expansion = 0.0;
        }
        visuals.widgets.hovered.bg_fill = colors.hovered;
        visuals.widgets.hovered.weak_bg_fill = colors.hovered;
        visuals.widgets.hovered.bg_stroke = egui::Stroke::new(1.0, colors.accent);
        visuals.widgets.active.bg_stroke = egui::Stroke::new(1.5, colors.accent);
        visuals.widgets.active.bg_fill = colors.selected;
        visuals.widgets.active.weak_bg_fill = colors.selected;
        visuals.widgets.open.bg_fill = colors.selected;
        visuals.widgets.open.weak_bg_fill = colors.selected;
        visuals.widgets.open.bg_stroke = egui::Stroke::new(1.0, colors.accent);
        ctx.set_visuals(visuals);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn app_themes_keep_readable_text_and_control_sizes_across_os_theme_changes() {
        for preference in [egui::ThemePreference::System, egui::ThemePreference::Light] {
            let ctx = egui::Context::default();
            ctx.set_os(egui::os::OperatingSystem::Mac);
            ctx.set_theme(preference);

            for theme in [Theme::GitHub, Theme::Charcoal] {
                theme.apply(&ctx);
                ctx.global_style_mut(|style| {
                    style.spacing.interact_size.y = 40.0;
                    for (kind, size) in [
                        (egui::TextStyle::Body, 18.0),
                        (egui::TextStyle::Button, 18.0),
                        (egui::TextStyle::Small, 14.0),
                    ] {
                        style
                            .text_styles
                            .insert(kind, egui::FontId::proportional(size));
                    }
                });

                for system_theme in [egui::Theme::Light, egui::Theme::Dark, egui::Theme::Light] {
                    let mut control_heights = [0.0; 3];
                    let mut size = 300.0;
                    let mut output = ctx.run_ui(
                        egui::RawInput {
                            system_theme: Some(system_theme),
                            screen_rect: Some(egui::Rect::from_min_size(
                                egui::Pos2::ZERO,
                                egui::vec2(800.0, 600.0),
                            )),
                            ..Default::default()
                        },
                        |ui| {
                            ui.label("Useful Timer");
                            ui.label("Your timers");
                            ui.label("Duration");
                            control_heights[0] = egui::ComboBox::from_label("Theme")
                                .selected_text(theme.label())
                                .show_ui(ui, |_| {})
                                .response
                                .rect
                                .height();
                            control_heights[1] = ui
                                .add(egui::Slider::new(&mut size, 220.0..=460.0).suffix(" px"))
                                .rect
                                .height();
                            control_heights[2] = ui
                                .add(egui::DragValue::new(&mut size).suffix(" px"))
                                .rect
                                .height();
                            let _ = ui.button("Save duration as default");
                            ui.label(
                                egui::RichText::new("Default: 25:00")
                                    .small()
                                    .color(theme.palette().muted),
                            );
                            ui.label(
                                egui::RichText::new(
                                    "Drag a desktop widget to move it. Right-click it for controls.",
                                )
                                .small()
                                .color(theme.palette().muted),
                            );
                        },
                    );
                    output.textures_delta.clear();

                    for (label, expected_size, expected_color) in [
                        ("Useful Timer", 18.0, theme.palette().text),
                        ("Your timers", 18.0, theme.palette().text),
                        ("Duration", 18.0, theme.palette().text),
                        ("Save duration as default", 18.0, theme.palette().text),
                        ("Default: 25:00", 14.0, theme.palette().muted),
                        (
                            "Drag a desktop widget to move it. Right-click it for controls.",
                            14.0,
                            theme.palette().muted,
                        ),
                    ] {
                        let text = output
                            .shapes
                            .iter()
                            .find_map(|shape| match &shape.shape {
                                egui::epaint::Shape::Text(text)
                                    if text.galley.job.text == label =>
                                {
                                    Some(text)
                                }
                                _ => None,
                            })
                            .expect("label must be painted");
                        for section in &text.galley.job.sections {
                            let color = text.override_text_color.unwrap_or_else(|| {
                                if section.format.color == Color32::PLACEHOLDER {
                                    text.fallback_color
                                } else {
                                    section.format.color
                                }
                            });
                            assert_eq!(color, expected_color, "{label} must stay readable");
                            assert_eq!(
                                section.format.font_id.size, expected_size,
                                "{label} must keep its configured font size"
                            );
                        }
                    }
                    assert!(
                        control_heights.into_iter().all(|height| height >= 40.0),
                        "OS theme must not shrink styled controls: {control_heights:?}"
                    );
                }
            }
        }
    }

    #[test]
    fn theme_text_and_toast_statuses_have_readable_contrast() {
        let luminance = |color: Color32| {
            let linear = |channel: u8| {
                let channel = f64::from(channel) / 255.0;
                if channel <= 0.04045 {
                    channel / 12.92
                } else {
                    ((channel + 0.055) / 1.055).powf(2.4)
                }
            };
            0.2126 * linear(color.r()) + 0.7152 * linear(color.g()) + 0.0722 * linear(color.b())
        };

        for theme in [Theme::GitHub, Theme::Charcoal] {
            let colors = theme.palette();
            for (foreground, background) in [
                (colors.text, colors.background),
                (colors.text, colors.sidebar),
                (colors.text, colors.surface),
                (colors.text, colors.raised),
                (colors.text, colors.hovered),
                (colors.muted, colors.background),
                (colors.muted, colors.sidebar),
                (colors.muted, colors.surface),
                (colors.primary_text, colors.primary),
                (colors.success, colors.surface),
                (colors.danger, colors.surface),
            ] {
                let foreground = luminance(foreground);
                let background = luminance(background);
                let ratio =
                    (foreground.max(background) + 0.05) / (foreground.min(background) + 0.05);
                assert!(
                    ratio >= 4.5,
                    "{} text contrast is {ratio:.2}:1",
                    theme.label()
                );
            }
        }
    }
}
