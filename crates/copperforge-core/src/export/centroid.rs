//! Centroid (pick-and-place) file export.
//!
//! Writes a PCBWay / JLCPCB-style CPL CSV — one row per component placement,
//! columns `Designator, Mid X, Mid Y, Layer, Rotation`. Positions are in
//! millimetres and rotation in degrees, taken straight from the `.kicad_pcb`.
//!
//! Note: some assembly houses expect per-footprint rotation corrections; this
//! exports KiCad's rotation as-is, so spot-check rotations before assembly.

use std::path::Path;

use crate::bom::BomEntry;
use crate::export::csv_field;

/// Map a KiCad copper-layer name to a CPL side label.
fn side(layer: &str) -> &'static str {
    if layer.starts_with('B') || layer.eq_ignore_ascii_case("bottom") {
        "Bottom"
    } else {
        "Top"
    }
}

/// Write a PCBWay / JLCPCB CPL centroid file for the given components.
pub fn write_cpl_csv(entries: &[BomEntry], path: &Path) -> Result<(), String> {
    let mut out = String::from("Designator,Mid X,Mid Y,Layer,Rotation\n");
    for e in entries {
        out.push_str(&format!(
            "{},{:.4},{:.4},{},{:.4}\n",
            csv_field(&e.reference),
            e.x,
            e.y,
            side(&e.layer),
            e.rotation,
        ));
    }
    std::fs::write(path, out)
        .map_err(|err| format!("Failed to write centroid file {}: {}", path.display(), err))
}
