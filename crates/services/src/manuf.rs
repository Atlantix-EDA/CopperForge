//! Board statistics + manufacturability scoring.
//!
//! Input: the parsed BOM (one entry per placed component — value, footprint,
//! layer) plus board dimensions. Output: the full stats block the client
//! draws — component breakdown, part commonality, and the v1
//! manufacturability index. The scoring heuristic lives here so it can
//! evolve (the planned v2 fuzzy-inference swap point) in one place.

use serde::{Deserialize, Serialize};
use std::collections::HashSet;

/// One placed component, as the client extracts it from the board.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Entry {
    pub value: String,
    pub footprint: String,
    /// KiCad layer name (e.g. "F.Cu" / "B.Cu") — decides top vs bottom.
    pub layer: String,
}

/// Board-stats request: the placements, board dimensions, and the outer
/// outline contour (mm) used for centroid + weight. `outline` may be empty —
/// centroid/weight then fall back to the bbox.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Request {
    pub entries: Vec<Entry>,
    pub width_mm: f64,
    pub height_mm: f64,
    #[serde(default)]
    pub outline: Vec<[f64; 2]>,
}

/// FR-4 substrate density, g/mm³ (~1.85 g/cm³).
const FR4_DENSITY_G_MM3: f64 = 0.00185;
/// Default finished board thickness, mm, when unknown.
const DEFAULT_THICKNESS_MM: f64 = 1.6;

/// Full stats block the client draws.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Stats {
    pub width_mm: f64,
    pub height_mm: f64,
    pub area_mm2: f64,
    pub total: usize,
    pub top: usize,
    pub bottom: usize,
    pub smt: usize,
    pub tht: usize,
    pub unclassified: usize,
    pub unique: usize,
    /// unique / total, 0..=1.
    pub unique_ratio: f32,
    /// total / unique (placements per distinct part).
    pub reuse: f32,
    /// 0..=100 manufacturability index, higher = easier to assemble.
    pub score: u32,
    /// Letter grade A..F derived from `score`.
    pub grade: char,
    /// Board geometric centroid (mm), from the outline polygon (or bbox
    /// center if no outline was supplied).
    pub centroid: [f64; 2],
    /// Estimated bare-board (FR-4) weight, grams — outline area × thickness ×
    /// density. A substrate estimate; copper/mask/components not included.
    pub weight_g: f64,
}

/// Compute the full statistics block for a board.
pub fn board_stats(req: &Request) -> Stats {
    let entries = &req.entries;
    let total = entries.len();
    let mut keys: HashSet<(String, String)> = HashSet::new();
    let (mut top, mut bottom, mut smt, mut tht, mut unclassified) = (0usize, 0, 0, 0, 0);
    for e in entries {
        keys.insert((e.value.trim().to_lowercase(), normalize_footprint(&e.footprint)));
        if is_bottom(&e.layer) {
            bottom += 1;
        } else {
            top += 1;
        }
        match classify(&e.footprint) {
            Mount::Smt => smt += 1,
            Mount::Tht => tht += 1,
            Mount::Unknown => unclassified += 1,
        }
    }
    let unique = keys.len();
    let unique_ratio = if total == 0 { 0.0 } else { unique as f32 / total as f32 };
    let tht_fraction = if total == 0 { 0.0 } else { tht as f32 / total as f32 };
    let reuse = if unique == 0 { 0.0 } else { total as f32 / unique as f32 };
    let (score, grade) = score(unique_ratio, tht_fraction, total);

    let bbox_area = req.width_mm * req.height_mm;
    // Polygon area (shoelace) + centroid from the outline when available —
    // more accurate than the bbox. Fall back to the bbox otherwise.
    let (poly_area, centroid) = polygon_area_centroid(&req.outline)
        .unwrap_or((bbox_area, [req.width_mm / 2.0, req.height_mm / 2.0]));
    let weight_g = poly_area * DEFAULT_THICKNESS_MM * FR4_DENSITY_G_MM3;

    Stats {
        width_mm: req.width_mm,
        height_mm: req.height_mm,
        area_mm2: req.width_mm * req.height_mm,
        total,
        top,
        bottom,
        smt,
        tht,
        unclassified,
        unique,
        unique_ratio,
        reuse,
        score,
        grade,
        centroid,
        weight_g,
    }
}

/// Signed-area (shoelace) polygon area + area-weighted centroid, mm.
/// Returns `None` for degenerate input (< 3 points or zero area).
fn polygon_area_centroid(pts: &[[f64; 2]]) -> Option<(f64, [f64; 2])> {
    if pts.len() < 3 {
        return None;
    }
    let n = pts.len();
    let (mut a2, mut cx, mut cy) = (0.0, 0.0, 0.0);
    for i in 0..n {
        let p = pts[i];
        let q = pts[(i + 1) % n];
        let cross = p[0] * q[1] - q[0] * p[1];
        a2 += cross;
        cx += (p[0] + q[0]) * cross;
        cy += (p[1] + q[1]) * cross;
    }
    if a2.abs() < 1e-9 {
        return None;
    }
    let area = a2.abs() / 2.0;
    let centroid = [cx / (3.0 * a2), cy / (3.0 * a2)];
    Some((area, centroid))
}

impl Stats {
    /// Grade color as RGBA — green (A/B), amber (C), red (D/F). Handy for
    /// the overlay; kept here so client code doesn't re-derive it.
    pub fn grade_rgba(&self) -> [u8; 4] {
        match self.grade {
            'A' | 'B' => [110, 220, 150, 255],
            'C' => [235, 205, 90, 255],
            _ => [240, 115, 100, 255],
        }
    }
}

/// KiCad places footprints on F.Cu (top) or B.Cu (bottom).
fn is_bottom(layer: &str) -> bool {
    let l = layer.to_lowercase();
    l.starts_with("b.") || l.contains("bottom") || l == "back"
}

/// Transparent 0..=100 manufacturability index (v1 heuristic). Part
/// commonality dominates, through-hole fraction is a real penalty, size is
/// minor. The planned v2 fuzzy-inference swap point.
fn score(unique_ratio: f32, tht_fraction: f32, total: usize) -> (u32, char) {
    let f_common = unique_ratio.clamp(0.0, 1.0);
    let f_tht = tht_fraction.clamp(0.0, 1.0);
    let f_size = (total as f32 / 1000.0).min(1.0);
    let difficulty = (0.55 * f_common + 0.30 * f_tht + 0.15 * f_size).clamp(0.0, 1.0);
    let score = (100.0 * (1.0 - difficulty)).round() as u32;
    let grade = match score {
        85..=100 => 'A',
        70..=84 => 'B',
        55..=69 => 'C',
        40..=54 => 'D',
        _ => 'F',
    };
    (score, grade)
}

enum Mount {
    Smt,
    Tht,
    Unknown,
}

fn normalize_footprint(fp: &str) -> String {
    fp.rsplit(':').next().unwrap_or(fp).trim().to_lowercase()
}

/// Minimal KiCad-footprint SMT/THT classifier.
fn classify(footprint: &str) -> Mount {
    let l = footprint.to_lowercase();
    if l.contains("_smd") || l.contains("smd:") {
        return Mount::Smt;
    }
    for code in ["0201", "0402", "0603", "0805", "1206", "1210", "2010", "2512"] {
        if l.contains(code) {
            return Mount::Smt;
        }
    }
    for s in [
        "soic", "sop", "tssop", "ssop", "qfp", "lqfp", "tqfp", "qfn", "dfn", "bga", "csp",
        "sot-23", "sot23", "sot-223", "dpak", "to-252", "to-263", "wson",
    ] {
        if l.contains(s) {
            return Mount::Smt;
        }
    }
    for s in [
        "pinheader", "pin_header", "pinsocket", "_tht", "_dip", ":dip-", "terminalblock",
        "to-92", "to-220", "to-247", "thru", "through_hole", "screw_terminal", "barrel",
    ] {
        if l.contains(s) {
            return Mount::Tht;
        }
    }
    Mount::Unknown
}

/// Convenience constructor for one entry.
pub fn entry(value: &str, footprint: &str, layer: &str) -> Entry {
    Entry {
        value: value.to_string(),
        footprint: footprint.to_string(),
        layer: layer.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn req(entries: Vec<Entry>, w: f64, h: f64) -> Request {
        Request { entries, width_mm: w, height_mm: h, outline: Vec::new() }
    }

    #[test]
    fn counts_and_area() {
        let r = board_stats(&req(
            vec![
                entry("100nF", "Capacitor_SMD:C_0402", "F.Cu"),
                entry("100nF", "Capacitor_SMD:C_0402", "F.Cu"),
                entry("10k", "Resistor_SMD:R_0603", "B.Cu"),
                entry("CONN", "Connector:PinHeader_1x04", "F.Cu"),
            ],
            50.0,
            20.0,
        ));
        assert_eq!(r.total, 4);
        assert_eq!(r.top, 3);
        assert_eq!(r.bottom, 1);
        assert_eq!(r.smt, 3);
        assert_eq!(r.tht, 1);
        assert_eq!(r.unique, 3);
        assert!((r.area_mm2 - 1000.0).abs() < 1e-9);
    }

    #[test]
    fn high_reuse_all_smt_scores_a() {
        let entries: Vec<_> = (0..50)
            .map(|_| entry("100nF", "Capacitor_SMD:C_0402", "F.Cu"))
            .collect();
        let r = board_stats(&req(entries, 30.0, 30.0));
        assert!(r.score >= 85, "score {}", r.score);
        assert_eq!(r.grade, 'A');
    }

    #[test]
    fn empty_board_is_safe() {
        let r = board_stats(&req(vec![], 10.0, 10.0));
        assert_eq!(r.total, 0);
        assert_eq!(r.reuse, 0.0);
    }
}
