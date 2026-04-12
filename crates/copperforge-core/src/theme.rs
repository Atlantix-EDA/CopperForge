//! Tokyo Night Storm color palette for the grid support UI.

use egui::Color32;

/// Tokyo Night Storm palette.
pub struct TokyoNight;

impl TokyoNight {
    // Backgrounds
    pub const BG:           Color32 = Color32::from_rgb(0x24, 0x28, 0x3b);
    pub const BG_DARK:      Color32 = Color32::from_rgb(0x1f, 0x23, 0x35);
    pub const BG_HIGHLIGHT: Color32 = Color32::from_rgb(0x2f, 0x33, 0x4d);

    // Foregrounds
    pub const FG:           Color32 = Color32::from_rgb(0xc0, 0xca, 0xf5);
    pub const FG_DIM:       Color32 = Color32::from_rgb(0xa9, 0xb1, 0xd6);
    pub const COMMENT:      Color32 = Color32::from_rgb(0x56, 0x5f, 0x89);

    // Accent colors
    pub const BLUE:         Color32 = Color32::from_rgb(0x7a, 0xa2, 0xf7);
    pub const CYAN:         Color32 = Color32::from_rgb(0x7d, 0xcf, 0xff);
    pub const GREEN:        Color32 = Color32::from_rgb(0x9e, 0xce, 0x6a);
    pub const MAGENTA:      Color32 = Color32::from_rgb(0xbb, 0x9a, 0xf7);
    pub const RED:          Color32 = Color32::from_rgb(0xf7, 0x76, 0x8e);
    pub const ORANGE:       Color32 = Color32::from_rgb(0xff, 0x9e, 0x64);
    pub const YELLOW:       Color32 = Color32::from_rgb(0xe0, 0xaf, 0x68);
    pub const TEAL:         Color32 = Color32::from_rgb(0x73, 0xda, 0xca);

    // Borders / UI chrome
    pub const BORDER:       Color32 = Color32::from_rgb(0x3b, 0x41, 0x61);
    pub const SELECTION:    Color32 = Color32::from_rgb(0x2e, 0x3c, 0x64);
}

/// Apply Tokyo Night Storm to the egui Visuals.
pub fn apply_visuals(ctx: &egui::Context) {
    let mut visuals = egui::Visuals::dark();

    visuals.panel_fill = TokyoNight::BG;
    visuals.window_fill = TokyoNight::BG_DARK;
    visuals.faint_bg_color = TokyoNight::BG_HIGHLIGHT;
    visuals.extreme_bg_color = TokyoNight::BG_DARK;
    visuals.selection.bg_fill = TokyoNight::SELECTION;
    visuals.selection.stroke = egui::Stroke::new(1.0, TokyoNight::BLUE);

    visuals.widgets.noninteractive.bg_fill = TokyoNight::BG;
    visuals.widgets.noninteractive.fg_stroke = egui::Stroke::new(1.0, TokyoNight::FG_DIM);
    visuals.widgets.noninteractive.bg_stroke = egui::Stroke::new(1.0, TokyoNight::BORDER);

    visuals.widgets.inactive.bg_fill = TokyoNight::BG_HIGHLIGHT;
    visuals.widgets.inactive.fg_stroke = egui::Stroke::new(1.0, TokyoNight::FG);
    visuals.widgets.inactive.bg_stroke = egui::Stroke::new(1.0, TokyoNight::BORDER);

    visuals.widgets.hovered.bg_fill = TokyoNight::SELECTION;
    visuals.widgets.hovered.fg_stroke = egui::Stroke::new(1.0, TokyoNight::CYAN);
    visuals.widgets.hovered.bg_stroke = egui::Stroke::new(1.0, TokyoNight::BLUE);

    visuals.widgets.active.bg_fill = TokyoNight::BLUE;
    visuals.widgets.active.fg_stroke = egui::Stroke::new(1.0, TokyoNight::BG_DARK);
    visuals.widgets.active.bg_stroke = egui::Stroke::new(1.0, TokyoNight::CYAN);

    visuals.override_text_color = Some(TokyoNight::FG);
    visuals.hyperlink_color = TokyoNight::CYAN;
    visuals.warn_fg_color = TokyoNight::ORANGE;
    visuals.error_fg_color = TokyoNight::RED;
    visuals.window_stroke = egui::Stroke::new(1.0, TokyoNight::BORDER);

    ctx.set_visuals(visuals);
}

/// Base font sizes.
const BASE_FONT_SIZES: &[(egui::TextStyle, f32)] = &[
    (egui::TextStyle::Small, 9.0),
    (egui::TextStyle::Body, 12.5),
    (egui::TextStyle::Monospace, 12.0),
    (egui::TextStyle::Button, 12.5),
    (egui::TextStyle::Heading, 18.0),
];

/// Apply a font scale percentage (100 = default egui sizes).
pub fn apply_font_scale(ctx: &egui::Context, scale_pct: u32) {
    let factor = scale_pct as f32 / 100.0;
    let mut style = (*ctx.style()).clone();
    for (text_style, base_size) in BASE_FONT_SIZES {
        if let Some(font_id) = style.text_styles.get_mut(text_style) {
            font_id.size = base_size * factor;
        }
    }
    ctx.set_style(style);
}
