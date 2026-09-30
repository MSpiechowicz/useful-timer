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
