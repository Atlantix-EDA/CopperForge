//! PCB layer types — LayerType, Side, and their display/color logic.

use egui::Color32;

/// Represents different PCB layers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum LayerType {
    Copper(u8),
    Silkscreen(Side),
    Soldermask(Side),
    Paste(Side),
    MechanicalOutline,
    /// Drill holes exported by `kicad-cli pcb export drill --format gerber`.
    /// Visible from both top and bottom; always drawn on top of copper.
    Drill,
    /// KiCad 10 via-plugging layers (via filling / tenting). Side-specific,
    /// exported as `<project>-plugging-front.gbr` / `...-back.gbr`.
    ViaPlugging(Side),
    /// KiCad user-defined layer (User.1..User.45). Boards often name these
    /// `M1 Board Outline`, `M10 Fab Notes`, `M12 Stackup`, etc. — the `u8`
    /// is the canonical KiCad user-layer index. Side-agnostic.
    UserLayer(u8),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum Side { Top, Bottom }

impl LayerType {
    pub const TOP_COPPER: Self = Self::Copper(1);
    pub const BOTTOM_COPPER: Self = Self::Copper(2);
    pub const TOP_SILK: Self = Self::Silkscreen(Side::Top);
    pub const BOTTOM_SILK: Self = Self::Silkscreen(Side::Bottom);
    pub const TOP_SOLDERMASK: Self = Self::Soldermask(Side::Top);
    pub const BOTTOM_SOLDERMASK: Self = Self::Soldermask(Side::Bottom);
    pub const TOP_PASTE: Self = Self::Paste(Side::Top);
    pub const BOTTOM_PASTE: Self = Self::Paste(Side::Bottom);

    pub fn standard_2_layer() -> Vec<Self> {
        vec![
            Self::Copper(1), Self::Copper(2),
            Self::Silkscreen(Side::Top), Self::Silkscreen(Side::Bottom),
            Self::Soldermask(Side::Top), Self::Soldermask(Side::Bottom),
            Self::Paste(Side::Top), Self::Paste(Side::Bottom),
            Self::MechanicalOutline,
            Self::Drill,
            Self::ViaPlugging(Side::Top), Self::ViaPlugging(Side::Bottom),
        ]
    }

    pub fn standard_4_layer() -> Vec<Self> {
        vec![
            Self::Copper(1), Self::Copper(2), Self::Copper(3), Self::Copper(4),
            Self::Silkscreen(Side::Top), Self::Silkscreen(Side::Bottom),
            Self::Soldermask(Side::Top), Self::Soldermask(Side::Bottom),
            Self::Paste(Side::Top), Self::Paste(Side::Bottom),
            Self::MechanicalOutline,
            Self::Drill,
            Self::ViaPlugging(Side::Top), Self::ViaPlugging(Side::Bottom),
        ]
    }

    pub fn for_layer_count(n: u8) -> Vec<Self> {
        let mut layers: Vec<Self> = (1..=n).map(Self::Copper).collect();
        layers.extend_from_slice(&[
            Self::Silkscreen(Side::Top), Self::Silkscreen(Side::Bottom),
            Self::Soldermask(Side::Top), Self::Soldermask(Side::Bottom),
            Self::Paste(Side::Top), Self::Paste(Side::Bottom),
            Self::MechanicalOutline,
            Self::Drill,
            Self::ViaPlugging(Side::Top), Self::ViaPlugging(Side::Bottom),
        ]);
        layers
    }

    /// Every layer type the UI should be aware of. Includes all 45 KiCad
    /// user-layer slots; unused ones are filtered out by the View Settings
    /// panel via `layer_store.find(layer_type)`.
    pub fn all() -> Vec<Self> {
        let mut v = Self::standard_2_layer();
        v.extend((1..=45).map(Self::UserLayer));
        v
    }

    pub fn display_name(&self) -> String {
        match self {
            Self::Copper(1) => "Top Copper (L1)".into(),
            Self::Copper(2) => "Bottom Copper (L2)".into(),
            Self::Copper(n) => format!("Inner Copper (L{})", n),
            Self::Silkscreen(Side::Top) => "Top Silkscreen".into(),
            Self::Silkscreen(Side::Bottom) => "Bottom Silkscreen".into(),
            Self::Soldermask(Side::Top) => "Top Soldermask".into(),
            Self::Soldermask(Side::Bottom) => "Bottom Soldermask".into(),
            Self::Paste(Side::Top) => "Top Paste".into(),
            Self::Paste(Side::Bottom) => "Bottom Paste".into(),
            Self::MechanicalOutline => "Mechanical Outline".into(),
            Self::Drill => "Drill Holes".into(),
            Self::ViaPlugging(Side::Top) => "Top Via Plugging".into(),
            Self::ViaPlugging(Side::Bottom) => "Bottom Via Plugging".into(),
            Self::UserLayer(n) => format!("User.{} (M{})", n, n),
        }
    }

    pub fn display_name_with_context(&self, total_copper: u8) -> String {
        match self {
            Self::Copper(1) => "Top Copper (L1)".into(),
            Self::Copper(n) if *n == total_copper => format!("Bottom Copper (L{})", n),
            Self::Copper(n) => format!("Inner Copper (L{})", n),
            other => other.display_name(),
        }
    }

    pub fn color(&self) -> Color32 {
        match self {
            Self::Copper(1) => Color32::from_rgba_premultiplied(184, 115, 51, 220),
            Self::Copper(2) => Color32::from_rgba_premultiplied(115, 184, 51, 220),
            Self::Copper(n) => {
                let hue = (*n as f32 * 60.0) % 360.0;
                let (r, g, b) = hsv_to_rgb(hue, 0.7, 0.8);
                Color32::from_rgba_premultiplied((r * 255.0) as u8, (g * 255.0) as u8, (b * 255.0) as u8, 220)
            }
            Self::Silkscreen(_) => Color32::from_rgba_premultiplied(255, 255, 255, 250),
            Self::Soldermask(Side::Top) => Color32::from_rgba_premultiplied(0, 132, 80, 180),
            Self::Soldermask(Side::Bottom) => Color32::from_rgba_premultiplied(0, 80, 132, 180),
            Self::Paste(Side::Top) => Color32::from_rgba_premultiplied(192, 192, 192, 200),
            Self::Paste(Side::Bottom) => Color32::from_rgba_premultiplied(128, 128, 128, 200),
            Self::MechanicalOutline => Color32::from_rgba_premultiplied(255, 255, 0, 250),
            // Drill holes drawn nearly opaque dark to punch through copper.
            Self::Drill => Color32::from_rgba_premultiplied(20, 20, 20, 240),
            // Via plugging — muted cyan/magenta to distinguish top/bottom.
            Self::ViaPlugging(Side::Top) => Color32::from_rgba_premultiplied(0, 180, 200, 160),
            Self::ViaPlugging(Side::Bottom) => Color32::from_rgba_premultiplied(200, 0, 180, 160),
            // User layers get hue-rotated distinct colors per index so the
            // eye can distinguish M1/M2/M10/M11/M12 at a glance.
            Self::UserLayer(n) => {
                let hue = ((*n as f32) * 47.0) % 360.0; // 47° steps visit all 45 slots distinctly
                let (r, g, b) = hsv_to_rgb(hue, 0.55, 0.85);
                Color32::from_rgba_premultiplied(
                    (r * 255.0) as u8,
                    (g * 255.0) as u8,
                    (b * 255.0) as u8,
                    180,
                )
            }
        }
    }

    pub fn should_render(&self, showing_top: bool) -> bool {
        match self {
            Self::Copper(1) => showing_top,
            Self::Copper(_) => true,
            Self::Silkscreen(Side::Top) | Self::Soldermask(Side::Top) | Self::Paste(Side::Top) => showing_top,
            Self::Silkscreen(Side::Bottom) | Self::Soldermask(Side::Bottom) | Self::Paste(Side::Bottom) => !showing_top,
            Self::MechanicalOutline => true,
            // Holes punch through the board — always visible from either side.
            Self::Drill => true,
            Self::ViaPlugging(Side::Top) => showing_top,
            Self::ViaPlugging(Side::Bottom) => !showing_top,
            // User layers are typically annotation/documentation — show on both sides.
            Self::UserLayer(_) => true,
        }
    }

    pub fn is_copper(&self) -> bool { matches!(self, Self::Copper(_)) }
    pub fn copper_layer_number(&self) -> Option<u8> { match self { Self::Copper(n) => Some(*n), _ => None } }

    pub fn is_top(&self) -> bool {
        matches!(
            self,
            Self::Copper(1)
                | Self::Silkscreen(Side::Top)
                | Self::Soldermask(Side::Top)
                | Self::Paste(Side::Top)
                | Self::ViaPlugging(Side::Top)
        )
    }

    pub fn is_bottom(&self, total_copper: u8) -> bool {
        match self {
            Self::Copper(n) => *n == total_copper,
            Self::Silkscreen(Side::Bottom)
            | Self::Soldermask(Side::Bottom)
            | Self::Paste(Side::Bottom)
            | Self::ViaPlugging(Side::Bottom) => true,
            _ => false,
        }
    }
}

fn hsv_to_rgb(h: f32, s: f32, v: f32) -> (f32, f32, f32) {
    let c = v * s;
    let x = c * (1.0 - ((h / 60.0) % 2.0 - 1.0).abs());
    let m = v - c;
    let (r, g, b) = match (h / 60.0) as u32 {
        0 => (c, x, 0.0), 1 => (x, c, 0.0), 2 => (0.0, c, x),
        3 => (0.0, x, c), 4 => (x, 0.0, c), _ => (c, 0.0, x),
    };
    (r + m, g + m, b + m)
}
