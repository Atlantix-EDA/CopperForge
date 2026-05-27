//! Approximate bare-board weight (no components) from board geometry.
//!
//! Formula:
//!   substrate_mass = bbox_area × thickness × FR4_density
//!   copper_mass    = bbox_area × fill_pct × (oz × 35 µm) × Cu_density
//!                    summed per copper layer
//!
//! Densities are at room temperature, FR4 is the "standard" Tg-130/140
//! glass-epoxy figure. `copper_fill_pct` is a user-supplied knob — the
//! actual fill ratio would require tessellating every gerber polygon,
//! which `gerber_viewer` doesn't expose (primitives are `pub(crate)`).
//! Same approximation every online PCB-weight calculator uses; off by
//! roughly ±20% for unusual boards.

use crate::canvas::model::{GerberScene, LayerKind};

/// Densities (g/cm³). Constants instead of magic numbers in the math.
const FR4_DENSITY_G_PER_CM3: f64 = 1.85;
const COPPER_DENSITY_G_PER_CM3: f64 = 8.96;
/// Copper thickness in mm per ounce of copper weight (1 oz/ft² = 35 µm).
const COPPER_MM_PER_OZ: f64 = 0.035;

#[derive(Debug, Clone, Copy)]
pub struct WeightInputs {
    pub board_thickness_mm: f64,
    pub copper_oz_outer: f64,
    pub copper_oz_inner: f64,
    /// 0–100. Estimated fraction of bbox area covered by copper on a
    /// typical layer. 50% is the calculator-default; ground-plane-heavy
    /// boards land closer to 80–90, signal-only layers closer to 20–30.
    pub copper_fill_pct: f64,
}

impl Default for WeightInputs {
    fn default() -> Self {
        Self {
            board_thickness_mm: 1.6,
            copper_oz_outer: 1.0,
            copper_oz_inner: 0.5,
            copper_fill_pct: 50.0,
        }
    }
}

/// Per-component weight breakdown, all in grams.
#[derive(Debug, Clone, Copy, Default)]
pub struct WeightBreakdown {
    pub substrate_g: f64,
    pub copper_outer_g: f64,
    pub copper_inner_g: f64,
    pub total_g: f64,
    pub outer_copper_layers: u8,
    pub inner_copper_layers: u8,
}

pub fn compute(scene: &GerberScene, inputs: &WeightInputs) -> Option<WeightBreakdown> {
    let bbox = scene.combined_bbox()?;
    let bbox_area_mm2 = bbox.width() * bbox.height();
    if bbox_area_mm2 <= 0.0 {
        return None;
    }
    // Count copper layers by kind.
    let mut outer = 0u8;
    let mut inner = 0u8;
    for layer in &scene.layers {
        match layer.kind {
            LayerKind::TopCopper | LayerKind::BottomCopper => outer += 1,
            LayerKind::InnerCopper(_) => inner += 1,
            _ => {}
        }
    }

    let fill = (inputs.copper_fill_pct / 100.0).clamp(0.0, 1.0);

    // mm² → cm² for density math: divide by 100.
    let bbox_area_cm2 = bbox_area_mm2 / 100.0;

    // Substrate. mm thickness → cm via /10.
    let substrate_volume_cm3 = bbox_area_cm2 * (inputs.board_thickness_mm / 10.0);
    let substrate_g = substrate_volume_cm3 * FR4_DENSITY_G_PER_CM3;

    // Per-layer copper volume = bbox_area × fill × layer_thickness.
    let outer_thickness_cm = (inputs.copper_oz_outer * COPPER_MM_PER_OZ) / 10.0;
    let inner_thickness_cm = (inputs.copper_oz_inner * COPPER_MM_PER_OZ) / 10.0;

    let one_outer_volume_cm3 = bbox_area_cm2 * fill * outer_thickness_cm;
    let one_inner_volume_cm3 = bbox_area_cm2 * fill * inner_thickness_cm;

    let copper_outer_g =
        one_outer_volume_cm3 * outer as f64 * COPPER_DENSITY_G_PER_CM3;
    let copper_inner_g =
        one_inner_volume_cm3 * inner as f64 * COPPER_DENSITY_G_PER_CM3;

    Some(WeightBreakdown {
        substrate_g,
        copper_outer_g,
        copper_inner_g,
        total_g: substrate_g + copper_outer_g + copper_inner_g,
        outer_copper_layers: outer,
        inner_copper_layers: inner,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    // Bare-bones sanity test: a 100x100 mm board, 1.6mm thick, 1oz outer,
    // no inner copper, 50% fill, two outer layers — should weigh roughly
    // ~32 g, of which substrate dominates (~30 g) and copper ~3 g.
    // The numbers should at least be in the right order of magnitude;
    // a real test would need a fixture GerberScene.

    #[test]
    fn defaults_are_reasonable() {
        let i = WeightInputs::default();
        assert_eq!(i.board_thickness_mm, 1.6);
        assert_eq!(i.copper_oz_outer, 1.0);
        assert_eq!(i.copper_oz_inner, 0.5);
        assert_eq!(i.copper_fill_pct, 50.0);
    }
}
