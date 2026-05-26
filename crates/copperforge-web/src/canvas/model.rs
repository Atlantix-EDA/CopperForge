//! Parsed gerber scene — what the renderer paints.
//!
//! Built from a `LoadedRelease`'s in-memory `<basename, bytes>` map.
//! Each `.gbr` becomes a `RenderLayer` carrying its parsed
//! `GerberLayer`, an assigned `LayerKind`, a default color, and a
//! visibility flag.

use std::collections::BTreeMap;
use std::io::BufReader;

use egui::Color32;
use gerber_viewer::gerber_parser::parse;
use gerber_viewer::{BoundingBox, GerberLayer};

/// Coarse layer classification, derived purely from the filename's
/// KiCad-style suffix (`-F_Cu`, `-B_Mask`, etc). Drives the default
/// color, z-order, and initial visibility — no semantic parsing of
/// the gerber content needed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LayerKind {
    EdgeCuts,
    TopCopper,
    BottomCopper,
    InnerCopper(u8),
    TopMask,
    BottomMask,
    TopSilk,
    BottomSilk,
    TopPaste,
    BottomPaste,
    /// Anything we don't have a special case for (User.N, mechanical,
    /// fab notes, etc). Rendered in a neutral grey, hidden by default.
    Other,
}

impl LayerKind {
    /// Match KiCad's gerber-export naming. The suffixes are stable —
    /// the native viewer's `layer_store/detection.rs` does the same
    /// thing in more detail, but for the wasm demo this coarser
    /// classification is enough.
    pub fn from_filename(name: &str) -> Self {
        let l = name.to_lowercase();
        if l.contains("-edge_cuts") || l.contains("-edgecuts") {
            return Self::EdgeCuts;
        }
        if l.contains("-f_cu") {
            return Self::TopCopper;
        }
        if l.contains("-b_cu") {
            return Self::BottomCopper;
        }
        if let Some(idx) = l.find("-in") {
            // `-In1_Cu.gbr` / `-In2_Cu.gbr` style.
            let rest = &l[idx + 3..];
            if rest.starts_with(|c: char| c.is_ascii_digit()) && rest.contains("_cu") {
                let num: u8 = rest
                    .chars()
                    .take_while(|c| c.is_ascii_digit())
                    .collect::<String>()
                    .parse()
                    .unwrap_or(1);
                return Self::InnerCopper(num);
            }
        }
        if l.contains("-f_mask") {
            return Self::TopMask;
        }
        if l.contains("-b_mask") {
            return Self::BottomMask;
        }
        if l.contains("-f_silkscreen") || l.contains("-f_silks") {
            return Self::TopSilk;
        }
        if l.contains("-b_silkscreen") || l.contains("-b_silks") {
            return Self::BottomSilk;
        }
        if l.contains("-f_paste") {
            return Self::TopPaste;
        }
        if l.contains("-b_paste") {
            return Self::BottomPaste;
        }
        Self::Other
    }

    /// Default color picked to read against a dark panel background.
    /// Soldermask layers carry alpha so they sit *on top of* copper
    /// without occluding it.
    pub fn default_color(&self) -> Color32 {
        match self {
            Self::EdgeCuts => Color32::from_rgb(240, 230, 50),
            Self::TopCopper => Color32::from_rgb(200, 100, 50),
            Self::BottomCopper => Color32::from_rgb(40, 100, 200),
            Self::InnerCopper(_) => Color32::from_rgb(120, 160, 80),
            Self::TopMask => Color32::from_rgba_unmultiplied(20, 120, 30, 150),
            Self::BottomMask => Color32::from_rgba_unmultiplied(20, 95, 30, 150),
            Self::TopSilk => Color32::from_rgb(230, 230, 230),
            Self::BottomSilk => Color32::from_rgb(180, 180, 180),
            Self::TopPaste => Color32::from_rgb(170, 170, 170),
            Self::BottomPaste => Color32::from_rgb(150, 150, 150),
            Self::Other => Color32::from_rgb(110, 115, 125),
        }
    }

    /// What's visible by default after loading? Just enough to see the
    /// board shape + top routing on first paint. Everything else is
    /// toggled on from the side panel.
    pub fn default_visible(&self) -> bool {
        matches!(self, Self::EdgeCuts | Self::TopCopper)
    }

    /// Higher z = painted later = on top. Mirrors the native renderer's
    /// `z_order_for(LayerType)`.
    pub fn z_order(&self) -> i32 {
        match self {
            Self::EdgeCuts => 100,
            Self::TopPaste => 80,
            Self::TopSilk => 70,
            Self::TopMask => 60,
            Self::TopCopper => 50,
            Self::InnerCopper(_) => 40,
            Self::BottomCopper => 30,
            Self::BottomMask => 20,
            Self::BottomSilk => 10,
            Self::BottomPaste => 5,
            Self::Other => 0,
        }
    }

    /// Short human-readable label for the side panel's checkbox list.
    /// Pulls in the layer index for inner copper so the user can tell
    /// In1 from In2. `Other` falls through to a filename-derived label
    /// — see `RenderLayer::display_label`.
    pub fn label(&self) -> String {
        match self {
            Self::EdgeCuts => "Edge cuts".to_string(),
            Self::TopCopper => "F.Cu (top)".to_string(),
            Self::BottomCopper => "B.Cu (bottom)".to_string(),
            Self::InnerCopper(n) => format!("In{}.Cu", n),
            Self::TopMask => "F.Mask".to_string(),
            Self::BottomMask => "B.Mask".to_string(),
            Self::TopSilk => "F.SilkS".to_string(),
            Self::BottomSilk => "B.SilkS".to_string(),
            Self::TopPaste => "F.Paste".to_string(),
            Self::BottomPaste => "B.Paste".to_string(),
            Self::Other => "Other".to_string(),
        }
    }

    /// Which physical side of the board the layer belongs to — drives
    /// the Top-only / Bottom-only preset buttons. EdgeCuts and inner
    /// copper count as Neutral; presets only force-show EdgeCuts and
    /// otherwise leave neutrals off.
    pub fn side(&self) -> LayerSide {
        match self {
            Self::TopCopper | Self::TopMask | Self::TopSilk | Self::TopPaste => LayerSide::Top,
            Self::BottomCopper
            | Self::BottomMask
            | Self::BottomSilk
            | Self::BottomPaste => LayerSide::Bottom,
            Self::EdgeCuts | Self::InnerCopper(_) | Self::Other => LayerSide::Neutral,
        }
    }
}

/// Physical board side a layer belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LayerSide {
    Top,
    Bottom,
    /// Edge cuts, inner copper, user/mechanical/info layers — not
    /// inherently top or bottom.
    Neutral,
}

/// One gerber layer in the scene — wraps the parsed `GerberLayer` with
/// the metadata the renderer needs.
pub struct RenderLayer {
    /// Source filename (e.g. `alpha_gan_sense-F_Cu.gbr`). Used to
    /// derive a panel label for `Other` layers.
    pub filename: String,
    pub kind: LayerKind,
    pub color: Color32,
    pub visible: bool,
    /// Parsed gerber commands + tessellated primitives + bounding box,
    /// all owned by `gerber_viewer::GerberLayer`.
    pub gerber: GerberLayer,
}

impl RenderLayer {
    /// What to show in the layer-checkbox row. Known kinds use their
    /// canonical label (`F.Cu (top)`, `B.Mask`, etc); `Other` falls
    /// back to the descriptor extracted from the filename — strip
    /// `<project>-` prefix and `.gbr` suffix. Replaces the wall of
    /// "Other" with meaningful per-layer names like
    /// `M10 Fab Notes` or `Top 3D Body`.
    pub fn display_label(&self) -> String {
        if !matches!(self.kind, LayerKind::Other) {
            return self.kind.label();
        }
        let trimmed = self
            .filename
            .strip_suffix(".gbr")
            .or_else(|| self.filename.strip_suffix(".GBR"))
            .unwrap_or(&self.filename);
        // KiCad output is `<project>-<descriptor>`. Take everything
        // after the first dash; if there's no dash, the whole thing.
        match trimmed.find('-') {
            Some(idx) => trimmed[idx + 1..].to_string(),
            None => trimmed.to_string(),
        }
    }
}

/// All renderable layers from one uploaded release. Layers are
/// pre-sorted by z-order so the render pass is a straight iteration.
#[derive(Default)]
pub struct GerberScene {
    pub layers: Vec<RenderLayer>,
}

impl GerberScene {
    /// Parse every `.gbr` in `entries` into a `RenderLayer`. `.drl`
    /// drill files are skipped — Excellon parsing is separate work and
    /// the demo's first job is to show that gerbers render.
    pub fn from_entries(entries: &BTreeMap<String, Vec<u8>>) -> Self {
        let mut layers = Vec::with_capacity(entries.len());
        for (name, bytes) in entries {
            if !name.to_lowercase().ends_with(".gbr") {
                continue;
            }
            let reader = BufReader::new(bytes.as_slice());
            let doc = match parse(reader) {
                Ok(d) => d,
                Err((_partial, e)) => {
                    log::warn!("gerber parse failed for {}: {}", name, e);
                    continue;
                }
            };
            let kind = LayerKind::from_filename(name);
            layers.push(RenderLayer {
                filename: name.clone(),
                kind,
                color: kind.default_color(),
                visible: kind.default_visible(),
                gerber: GerberLayer::new(doc.into_commands()),
            });
        }
        // Render-order is z_order ascending (top layers last so they
        // paint on top of bottom ones).
        layers.sort_by_key(|l| l.kind.z_order());
        Self { layers }
    }

    /// Combined bounding box of every parsed layer (visible or not).
    /// Used to fit the initial view — the user sees the whole board on
    /// first paint, regardless of which layers are currently toggled on.
    pub fn combined_bbox(&self) -> Option<BoundingBox> {
        let mut acc: Option<BoundingBox> = None;
        for layer in &self.layers {
            let bbox = layer.gerber.bounding_box().clone();
            if bbox.is_empty() {
                continue;
            }
            acc = Some(match acc {
                None => bbox,
                Some(mut a) => {
                    a.expand(&bbox);
                    a
                }
            });
        }
        acc
    }
}
