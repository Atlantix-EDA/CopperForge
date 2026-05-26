//! PCBWay-specific release packaging.
//!
//! When a release targets PCBWay, the standard release zip is augmented
//! with `PCBWAY_FAB_SPECS.md` — a short fact-sheet listing the part and
//! pad counts PCBWay needs to quote an assembly order:
//!
//! - SMT parts (footprints with `(attr smd)`)
//! - Through-hole parts (footprints with `(attr through_hole)`, or no
//!   `attr` at all — KiCad's older default)
//! - SMT pads (count of `(pad ...)` inside SMT footprints)
//! - Parts on top (`F.Cu`) / bottom (`B.Cu`)
//!
//! The .kicad_pcb is scanned with the same per-footprint-block regex
//! approach kiparse uses internally. kiparse's `ComponentInfo` doesn't
//! expose `attr` or pad counts, so we re-scan rather than wait on a
//! kiparse PR.

use std::path::Path;

use once_cell::sync::Lazy;
use regex::Regex;

/// Fabrication metrics for a single PCB, suitable for a PCBWay quote.
///
/// `Eq` is intentionally absent — `f64` fields don't implement it.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct FabStats {
    pub smt_parts: usize,
    pub tht_parts: usize,
    pub smt_pads: usize,
    pub parts_top: usize,
    pub parts_bottom: usize,
    /// Board dimensions from the Edge.Cuts outline, when parseable.
    /// `None` if the board has no closed outline or the parse failed —
    /// the markdown writer simply omits the row.
    pub board_width_mm: Option<f64>,
    pub board_height_mm: Option<f64>,
}

/// Scan a `.kicad_pcb` for the part/pad metrics PCBWay's order form
/// asks for, plus the board's bounding-box dimensions from Edge.Cuts.
pub fn compute_fab_stats(pcb_path: &Path) -> Result<FabStats, String> {
    let content = std::fs::read_to_string(pcb_path)
        .map_err(|e| format!("Failed to read PCB file: {}", e))?;
    let mut stats = compute_fab_stats_from_str(&content);

    // Board outline lives in a separate kiparse path — non-fatal if it
    // fails, since part counts alone are still useful for a PCBWay quote.
    match crate::bom::extract_board_dimensions(pcb_path) {
        Ok(Some(dims)) => {
            stats.board_width_mm = Some(dims.width_mm);
            stats.board_height_mm = Some(dims.height_mm);
        }
        Ok(None) => {} // No closed outline — leave as None.
        Err(_) => {}   // Parse failure — same, caller logs via warn elsewhere.
    }

    Ok(stats)
}

/// Same as `compute_fab_stats` but takes the file contents directly —
/// useful for testing and to avoid a re-read when the caller already has
/// the string in memory.
pub fn compute_fab_stats_from_str(content: &str) -> FabStats {
    // Footprint block starts — same anchor kiparse uses.
    static FOOTPRINT_START: Lazy<Regex> =
        Lazy::new(|| Regex::new(r#"\n\s*\(footprint\s+""#).unwrap());
    // `(attr smd)` / `(attr through_hole)` — `attr` may carry extra
    // flags after the mount type (e.g. `exclude_from_bom`), so match the
    // first token only.
    static ATTR_RE: Lazy<Regex> =
        Lazy::new(|| Regex::new(r#"\(attr\s+(smd|through_hole|virtual)\b"#).unwrap());
    // Footprint layer — first `(layer "...")` inside the block is the
    // footprint's own layer assignment.
    static LAYER_RE: Lazy<Regex> =
        Lazy::new(|| Regex::new(r#"\(layer\s+"([^"]+)""#).unwrap());
    // Pads — `(pad "1" smd ...)` or `(pad 1 thru_hole ...)`. Counted
    // regardless of mount type; SMT-pad totals come from pads in SMT
    // footprints only.
    static PAD_RE: Lazy<Regex> =
        Lazy::new(|| Regex::new(r#"\(pad\s"#).unwrap());

    let starts: Vec<usize> = FOOTPRINT_START
        .find_iter(content)
        .map(|m| m.start())
        .collect();

    let mut stats = FabStats::default();

    for (i, &start) in starts.iter().enumerate() {
        let end = starts.get(i + 1).copied().unwrap_or(content.len());
        let block = &content[start..end];

        // Mount type — KiCad emits `(attr smd)` or `(attr through_hole)`.
        // Older boards sometimes omit `attr` entirely; treat that as
        // through-hole to match KiCad's historical default.
        let mount = ATTR_RE
            .captures(block)
            .map(|c| c[1].to_string())
            .unwrap_or_else(|| "through_hole".to_string());

        // Skip `virtual` footprints — they're not real parts (mounting
        // holes, fiducials sometimes, test points marked as virtual).
        if mount == "virtual" {
            continue;
        }

        // Side — F.Cu = top, B.Cu = bottom. Defensive default: top.
        let on_top = LAYER_RE
            .captures(block)
            .map(|c| !c[1].starts_with("B."))
            .unwrap_or(true);

        let pad_count = PAD_RE.find_iter(block).count();

        match mount.as_str() {
            "smd" => {
                stats.smt_parts += 1;
                stats.smt_pads += pad_count;
            }
            _ => {
                stats.tht_parts += 1;
            }
        }
        if on_top {
            stats.parts_top += 1;
        } else {
            stats.parts_bottom += 1;
        }
    }

    stats
}

/// Render the PCBWay fab-specs markdown. Goes into the release zip as
/// `PCBWAY_FAB_SPECS.md` so the assembly-quote conversation with PCBWay
/// starts with the part counts on the table.
pub fn write_fab_specs_md(
    stats: &FabStats,
    project_stem: &str,
    rev_tag: &str,
    path: &Path,
) -> Result<(), String> {
    let mut out = String::new();
    out.push_str(&format!("# {} — {} · PCBWay Fab Specs\n\n", project_stem, rev_tag));
    out.push_str(
        "Part and pad counts pulled directly from the `.kicad_pcb` at \
         release time. Paste these into PCBWay's assembly quote form.\n\n",
    );
    if let (Some(w), Some(h)) = (stats.board_width_mm, stats.board_height_mm) {
        out.push_str("## Board dimensions\n\n");
        out.push_str("| Dimension | mm |\n");
        out.push_str("|-----------|---:|\n");
        out.push_str(&format!("| Width  | {:.2} |\n", w));
        out.push_str(&format!("| Height | {:.2} |\n", h));
        out.push_str(&format!("| Area   | {:.2} mm² |\n", w * h));
        out.push_str("\n");
    }
    out.push_str("## Assembly counts\n\n");
    out.push_str("| Metric             | Count |\n");
    out.push_str("|--------------------|------:|\n");
    out.push_str(&format!("| SMT parts          | {} |\n", stats.smt_parts));
    out.push_str(&format!("| Through-hole parts | {} |\n", stats.tht_parts));
    out.push_str(&format!("| SMT pads           | {} |\n", stats.smt_pads));
    out.push_str(&format!("| Parts on top       | {} |\n", stats.parts_top));
    out.push_str(&format!("| Parts on bottom    | {} |\n", stats.parts_bottom));
    out.push_str("\n");
    out.push_str(
        "_Generated by CopperForge. \
         See `RELEASE_NOTES.md` and the bundled BOM/centroid files for the rest._\n",
    );
    std::fs::write(path, out).map_err(|e| format!("Failed to write {}: {}", path.display(), e))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counts_smt_tht_and_sides() {
        // Two footprints: one SMD on top with 2 pads, one through-hole on bottom.
        let pcb = r#"(kicad_pcb
  (footprint "Resistor_SMD:R_0402"
    (layer "F.Cu")
    (attr smd)
    (pad "1" smd rect (size 0.5 0.6))
    (pad "2" smd rect (size 0.5 0.6))
  )
  (footprint "Connector:PinHeader"
    (layer "B.Cu")
    (attr through_hole)
    (pad "1" thru_hole circle (size 1.7 1.7))
    (pad "2" thru_hole circle (size 1.7 1.7))
    (pad "3" thru_hole circle (size 1.7 1.7))
  )
)"#;
        let s = compute_fab_stats_from_str(pcb);
        assert_eq!(s.smt_parts, 1);
        assert_eq!(s.tht_parts, 1);
        assert_eq!(s.smt_pads, 2);
        assert_eq!(s.parts_top, 1);
        assert_eq!(s.parts_bottom, 1);
    }

    #[test]
    fn missing_attr_defaults_to_through_hole() {
        // KiCad sometimes omits `(attr ...)` on legacy through-hole parts.
        let pcb = r#"(kicad_pcb
  (footprint "Legacy:DIP_8"
    (layer "F.Cu")
    (pad "1" thru_hole circle (size 1.7 1.7))
  )
)"#;
        let s = compute_fab_stats_from_str(pcb);
        assert_eq!(s.tht_parts, 1);
        assert_eq!(s.smt_parts, 0);
        assert_eq!(s.smt_pads, 0);
        assert_eq!(s.parts_top, 1);
    }

    #[test]
    fn virtual_attr_skipped() {
        // Virtual footprints (mounting holes, fiducials) shouldn't count
        // as assembled parts.
        let pcb = r#"(kicad_pcb
  (footprint "Mounting:Hole"
    (layer "F.Cu")
    (attr virtual)
    (pad "1" np_thru_hole circle (size 3 3))
  )
)"#;
        let s = compute_fab_stats_from_str(pcb);
        assert_eq!(s.smt_parts, 0);
        assert_eq!(s.tht_parts, 0);
        assert_eq!(s.parts_top, 0);
    }
}
