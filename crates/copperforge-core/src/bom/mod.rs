//! BOM extraction from KiCad .kicad_pcb files via kiparse.
//!
//! Parses the PCB file directly — no live IPC connection to KiCad needed.

pub mod schematic;

use std::path::Path;
use kiparse::pcb::detail_parser::{DetailParser, ComponentInfo};

/// A BOM entry extracted from a .kicad_pcb file.
#[derive(Debug, Clone)]
pub struct BomEntry {
    pub item: usize,
    pub reference: String,
    pub value: String,
    pub description: String,
    pub footprint: String,
    pub x: f64,
    pub y: f64,
    pub rotation: f64,
    pub layer: String,
}

impl BomEntry {
    fn from_component(item: usize, c: &ComponentInfo) -> Self {
        Self {
            item,
            reference: c.reference.clone(),
            value: c.value.clone().unwrap_or_default(),
            description: c.description.clone().unwrap_or_default(),
            footprint: c.footprint.clone(),
            x: c.position.0,
            y: c.position.1,
            rotation: c.rotation,
            layer: c.layer.clone(),
        }
    }

    pub fn matches_filter(&self, filter: &str) -> bool {
        if filter.is_empty() { return true; }
        let f = filter.to_lowercase();
        self.reference.to_lowercase().contains(&f)
            || self.value.to_lowercase().contains(&f)
            || self.description.to_lowercase().contains(&f)
            || self.footprint.to_lowercase().contains(&f)
    }
}

/// Board dimensions from Edge.Cuts outline.
#[derive(Debug, Clone)]
pub struct BoardDimensions {
    pub width_mm: f64,
    pub height_mm: f64,
    pub area_mm2: f64,
}

/// Extract BOM from a .kicad_pcb file.
pub fn extract_bom(pcb_path: &Path) -> Result<Vec<BomEntry>, String> {
    let content = std::fs::read_to_string(pcb_path)
        .map_err(|e| format!("Failed to read PCB file: {}", e))?;

    let parser = DetailParser::new(&content);
    let components = parser.extract_components()
        .map_err(|e| format!("Failed to parse components: {}", e))?;

    let mut entries: Vec<BomEntry> = components.iter()
        .enumerate()
        .map(|(i, c)| BomEntry::from_component(i + 1, c))
        .collect();

    // Natural sort by reference (R1, R2, R10 — not R1, R10, R2)
    entries.sort_by(|a, b| natural_sort_key(&a.reference).cmp(&natural_sort_key(&b.reference)));

    // Re-number after sorting
    for (i, entry) in entries.iter_mut().enumerate() {
        entry.item = i + 1;
    }

    Ok(entries)
}

/// Extract board dimensions from Edge.Cuts outline.
///
/// Walks every `(gr_line | gr_arc | gr_rect | gr_circle | gr_poly ...)`
/// block on layer `Edge.Cuts` and returns the axis-aligned bounding box
/// of all referenced coordinates. Arcs are approximated by their start,
/// mid, and end points — accurate for corner rounds; circle outlines
/// are rare enough not to matter for sizing.
///
/// kiparse's `extract_board_outline` only matches `gr_line`, which gives
/// nonsense numbers on any board with rounded corners or rect-style
/// outlines (the alpha_gan_adc rev_01 bug: 1.12 × 0.00 instead of 66.7
/// × 36.75). Owning the parse here lets every consumer — BoM panel,
/// PCBWay fab specs, future fab targets — share the same fix.
pub fn extract_board_dimensions(pcb_path: &Path) -> Result<Option<BoardDimensions>, String> {
    let content = std::fs::read_to_string(pcb_path)
        .map_err(|e| format!("Failed to read PCB file: {}", e))?;
    Ok(compute_board_dimensions_from_str(&content))
}

/// String-input variant — same logic, no file I/O. Used by tests and by
/// any caller that already has the .kicad_pcb content in memory.
pub fn compute_board_dimensions_from_str(content: &str) -> Option<BoardDimensions> {
    use once_cell::sync::Lazy;
    use regex::Regex;

    // Anchor for each graphical shape on the board (not inside footprints,
    // which use `fp_*` instead — those have their own layer per element
    // and aren't board outlines).
    static SHAPE_START: Lazy<Regex> = Lazy::new(|| {
        Regex::new(r#"\n\s*\(gr_(line|arc|rect|circle|poly)\b"#).unwrap()
    });
    // Pull any (start | end | mid | center | xy) coord pair from the block.
    // Covers gr_line (start/end), gr_arc (start/mid/end), gr_rect (start/end
    // corners), gr_circle (center + a point on the perimeter as `end`),
    // and gr_poly (a list of `xy` inside `pts`).
    static COORD: Lazy<Regex> = Lazy::new(|| {
        Regex::new(r#"\((start|end|mid|center|xy)\s+(-?\d+(?:\.\d+)?)\s+(-?\d+(?:\.\d+)?)\)"#).unwrap()
    });
    // Layer must be exactly Edge.Cuts — silkscreen/courtyard/etc gr_* shapes
    // would otherwise distort the bbox.
    static EDGE_CUTS_LAYER: Lazy<Regex> = Lazy::new(|| {
        Regex::new(r#"\(layer\s+"Edge\.Cuts"\)"#).unwrap()
    });

    let starts: Vec<usize> = SHAPE_START.find_iter(content).map(|m| m.start()).collect();
    let mut min_x = f64::INFINITY;
    let mut min_y = f64::INFINITY;
    let mut max_x = f64::NEG_INFINITY;
    let mut max_y = f64::NEG_INFINITY;
    let mut any = false;

    for (i, &start) in starts.iter().enumerate() {
        let end = starts.get(i + 1).copied().unwrap_or(content.len());
        let block = &content[start..end];
        if !EDGE_CUTS_LAYER.is_match(block) {
            continue;
        }
        for cap in COORD.captures_iter(block) {
            let x: f64 = cap[2].parse().unwrap_or(0.0);
            let y: f64 = cap[3].parse().unwrap_or(0.0);
            min_x = min_x.min(x);
            max_x = max_x.max(x);
            min_y = min_y.min(y);
            max_y = max_y.max(y);
            any = true;
        }
    }

    if !any {
        return None;
    }
    let width_mm = max_x - min_x;
    let height_mm = max_y - min_y;
    Some(BoardDimensions {
        width_mm,
        height_mm,
        area_mm2: width_mm * height_mm,
    })
}

/// Component summary — count by reference prefix (R, C, U, J, etc.)
pub fn component_summary(entries: &[BomEntry]) -> Vec<(String, usize)> {
    use std::collections::HashMap;
    let mut counts: HashMap<String, usize> = HashMap::new();
    for entry in entries {
        let prefix = entry.reference.chars()
            .take_while(|c| c.is_alphabetic())
            .collect::<String>();
        *counts.entry(prefix).or_default() += 1;
    }
    let mut sorted: Vec<_> = counts.into_iter().collect();
    sorted.sort_by(|a, b| b.1.cmp(&a.1));
    sorted
}

/// Cover-page statistics for a release BOM workbook. Mirrors the metrics the
/// Board stats panel surfaces, gathered in one pass for the export.
#[derive(Debug, Clone, Default)]
pub struct CoverStats {
    pub top_smt: usize,
    pub top_th: usize,
    pub bottom_smt: usize,
    pub bottom_th: usize,
    pub dimensions: Option<BoardDimensions>,
    pub smt_pads: usize,
    pub th_pads: usize,
    /// Non-plated through-hole pads (mechanical holes, tooling).
    pub np_pads: usize,
    pub vias: usize,
}

impl CoverStats {
    pub fn total_components(&self) -> usize {
        self.top_smt + self.top_th + self.bottom_smt + self.bottom_th
    }
    pub fn through_holes(&self) -> usize {
        self.th_pads + self.vias
    }
}

/// SMT vs through-hole from the footprint name. Heuristic — KiCad footprint
/// libraries name through-hole parts distinctly. The exact answer would read
/// pad types from kiparse; this matches the stats-panel classification.
fn is_through_hole(footprint: &str) -> bool {
    let f = footprint.to_lowercase();
    f.contains("tht")
        || f.contains("through")
        || f.contains("pinheader")
        || f.contains("pin_header")
        || f.contains("_th_")
        || f.contains("radial")
        || f.contains("axial")
        || f.contains("dip-")
        || f.contains("to-220")
        || f.contains("to-92")
}

/// Count SMT pads, plated and non-plated through-hole pads, and vias from the
/// raw PCB text. Board-level and exact (no footprint heuristic involved here).
fn pad_hole_counts(content: &str) -> (usize, usize, usize, usize) {
    use once_cell::sync::Lazy;
    use regex::Regex;
    static SMD: Lazy<Regex> =
        Lazy::new(|| Regex::new(r#"\(pad\s+"[^"]*"\s+smd\b"#).unwrap());
    static TH: Lazy<Regex> =
        Lazy::new(|| Regex::new(r#"\(pad\s+"[^"]*"\s+thru_hole\b"#).unwrap());
    static NP: Lazy<Regex> =
        Lazy::new(|| Regex::new(r#"\(pad\s+"[^"]*"\s+np_thru_hole\b"#).unwrap());
    static VIA: Lazy<Regex> = Lazy::new(|| Regex::new(r#"\(via\b"#).unwrap());
    (
        SMD.find_iter(content).count(),
        TH.find_iter(content).count(),
        NP.find_iter(content).count(),
        VIA.find_iter(content).count(),
    )
}

/// Compute cover-page statistics from the parsed entries plus the raw
/// `.kicad_pcb` content (the caller already read the file for `extract_bom`).
pub fn cover_stats(entries: &[BomEntry], pcb_content: &str) -> CoverStats {
    let mut s = CoverStats::default();
    for e in entries {
        let bottom = {
            let l = e.layer.to_lowercase();
            l.starts_with('b') || l.contains("bottom")
        };
        match (bottom, is_through_hole(&e.footprint)) {
            (false, false) => s.top_smt += 1,
            (false, true) => s.top_th += 1,
            (true, false) => s.bottom_smt += 1,
            (true, true) => s.bottom_th += 1,
        }
    }
    s.dimensions = compute_board_dimensions_from_str(pcb_content);
    let (smt, th, np, via) = pad_hole_counts(pcb_content);
    s.smt_pads = smt;
    s.th_pads = th;
    s.np_pads = np;
    s.vias = via;
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gr_rect_outline_gives_real_size() {
        // alpha_gan_adc shape: a single gr_rect on Edge.Cuts. kiparse's
        // gr_line-only matcher returned 1.12 × 0.00; ours must give
        // (182.24 - 115.54) × (110.0005 - 73.2505) = 66.7 × 36.75.
        let pcb = r#"(kicad_pcb
  (gr_rect
    (start 115.54 73.2505)
    (end 182.24 110.0005)
    (stroke (width 0.05) (type default))
    (fill no)
    (layer "Edge.Cuts")
  )
)"#;
        let d = compute_board_dimensions_from_str(pcb).expect("dims");
        assert!((d.width_mm - 66.7).abs() < 0.001, "width={}", d.width_mm);
        assert!((d.height_mm - 36.75).abs() < 0.001, "height={}", d.height_mm);
    }

    #[test]
    fn ignores_non_edge_cuts_shapes() {
        // gr_line on silkscreen should NOT enlarge the board bbox.
        let pcb = r#"(kicad_pcb
  (gr_rect
    (start 0 0)
    (end 10 5)
    (layer "Edge.Cuts")
  )
  (gr_line
    (start -100 -100)
    (end 100 100)
    (layer "F.SilkS")
  )
)"#;
        let d = compute_board_dimensions_from_str(pcb).expect("dims");
        assert!((d.width_mm - 10.0).abs() < 0.001);
        assert!((d.height_mm - 5.0).abs() < 0.001);
    }

    #[test]
    fn rounded_corners_with_arcs() {
        // Edge.Cuts made of 4 gr_lines + 4 gr_arcs (rounded rectangle).
        // Arc bbox is approximated by start/mid/end — close enough for sizing.
        let pcb = r#"(kicad_pcb
  (gr_line (start 1 0)  (end 9 0)  (layer "Edge.Cuts"))
  (gr_line (start 10 1) (end 10 4) (layer "Edge.Cuts"))
  (gr_line (start 9 5)  (end 1 5)  (layer "Edge.Cuts"))
  (gr_line (start 0 4)  (end 0 1)  (layer "Edge.Cuts"))
  (gr_arc (start 1 0) (mid 0.29 0.29) (end 0 1) (layer "Edge.Cuts"))
  (gr_arc (start 10 1) (mid 9.71 0.29) (end 9 0) (layer "Edge.Cuts"))
  (gr_arc (start 9 5) (mid 9.71 4.71) (end 10 4) (layer "Edge.Cuts"))
  (gr_arc (start 0 4) (mid 0.29 4.71) (end 1 5) (layer "Edge.Cuts"))
)"#;
        let d = compute_board_dimensions_from_str(pcb).expect("dims");
        assert!((d.width_mm - 10.0).abs() < 0.001, "width={}", d.width_mm);
        assert!((d.height_mm - 5.0).abs() < 0.001, "height={}", d.height_mm);
    }

    #[test]
    fn no_edge_cuts_returns_none() {
        let pcb = r#"(kicad_pcb
  (gr_line (start 0 0) (end 1 1) (layer "F.SilkS"))
)"#;
        assert!(compute_board_dimensions_from_str(pcb).is_none());
    }
}

/// Natural sort key: splits "R10" into ("R", 10).
fn natural_sort_key(s: &str) -> (String, u32) {
    let prefix: String = s.chars().take_while(|c| !c.is_ascii_digit()).collect();
    let num: u32 = s.chars()
        .skip_while(|c| !c.is_ascii_digit())
        .take_while(|c| c.is_ascii_digit())
        .collect::<String>()
        .parse()
        .unwrap_or(0);
    (prefix, num)
}
