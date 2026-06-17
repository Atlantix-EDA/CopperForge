//! Drill/via hole extraction.
//!
//! Two input formats, both produced by KiCad and both placed in the *same*
//! coordinate frame as the gerbers (same origin), so a hole at gerber
//! coordinate `(x, y)` lands on the copper pad at `(x, y)`:
//!
//! - **Excellon `.drl`** — the classic NC drill format. Header declares
//!   units (`INCH` / `METRIC`) and tool diameters (`T1C0.0220`); the body
//!   selects a tool (`T1`) then lists hole coordinates (`X..Y..`). This is
//!   KiCad's default drill export.
//! - **Drill-as-gerber `*-PTH-drl.gbr` / `*-NPTH-drl.gbr`** — KiCad can
//!   also emit drills as a normal gerber of flashed circular apertures. We
//!   route these straight through the copper walker (a flashed circle is a
//!   flashed circle) and recover each hole's centre + radius from its
//!   contour.
//!
//! Output holes are transformed exactly like copper/mask — shifted by the
//! *outline's* bbox centre, no Y-flip — so they share the board's world
//! frame vertex-for-vertex. The 3D view then draws each as a dark disk on
//! the top and bottom surfaces, reading as a drilled hole through the pad.

use std::fs::File;
use std::io::{BufReader, Cursor, Read};
use std::path::Path;

use gerber_parser::parse;
use gerber_types::Unit;
use gerber_viewer::BoundingBox;

use super::copper::walk_copper;

/// One drilled hole, in the board's world frame (mm, bbox-centred).
#[derive(Debug, Clone, Copy)]
pub struct Hole {
    pub center: [f32; 2],
    pub radius: f32,
}

/// All holes recovered from one drill source.
#[derive(Debug, Default, Clone)]
pub struct DrillData {
    pub holes: Vec<Hole>,
}

impl DrillData {
    pub fn is_empty(&self) -> bool {
        self.holes.is_empty()
    }
}

/// Parse an Excellon `.drl` from in-memory bytes. `outline_bbox` frames the
/// holes into the board's world space (same shift copper/mask use). Returns
/// `None` if no holes are recovered.
pub fn extract_drill_excellon_from_bytes(
    bytes: &[u8],
    outline_bbox: &BoundingBox,
) -> Option<DrillData> {
    let text = String::from_utf8_lossy(bytes);
    let raw = parse_excellon(&text);
    finalize(raw, outline_bbox)
}

/// File-path variant of [`extract_drill_gerber_from_bytes`] — the native app
/// has the drill-gerber on disk (kicad-cli exports drills as gerber). Returns
/// `None` if the file can't be opened.
pub fn extract_drill_gerber(path: &Path, outline_bbox: &BoundingBox) -> Option<DrillData> {
    let mut bytes = Vec::new();
    File::open(path).ok()?.read_to_end(&mut bytes).ok()?;
    extract_drill_gerber_from_bytes(&bytes, outline_bbox)
}

/// Recover holes from a drill-*gerber* (flashed circles) via the copper
/// walker. Same world frame as the copper/board meshes by construction.
pub fn extract_drill_gerber_from_bytes(
    bytes: &[u8],
    outline_bbox: &BoundingBox,
) -> Option<DrillData> {
    let reader = BufReader::new(Cursor::new(bytes));
    let doc = match parse(reader) {
        Ok(d) => d,
        Err((d, _)) => d,
    };
    let unit_scale = match doc.units {
        Some(Unit::Millimeters) => 1.0_f64,
        Some(Unit::Inches) => 25.4_f64,
        None => 1.0_f64,
    };
    let (contours, _counts) = walk_copper(&doc, unit_scale);
    // Each flashed-circle contour → centre (mean) + radius (mean distance).
    let raw: Vec<(f64, f64, f64)> = contours
        .iter()
        .filter(|c| c.len() >= 3)
        .map(|c| {
            let n = c.len() as f64;
            let (mut sx, mut sy) = (0.0, 0.0);
            for p in c {
                sx += p.x as f64;
                sy += p.y as f64;
            }
            let (cx, cy) = (sx / n, sy / n);
            let mut sr = 0.0;
            for p in c {
                let dx = p.x as f64 - cx;
                let dy = p.y as f64 - cy;
                sr += (dx * dx + dy * dy).sqrt();
            }
            // (cx, cy, diameter)
            (cx, cy, (sr / n) * 2.0)
        })
        .collect();
    finalize(raw, outline_bbox)
}

/// Shift raw `(x_mm, y_mm, diameter_mm)` triples into the board world frame
/// and drop degenerate holes.
fn finalize(raw: Vec<(f64, f64, f64)>, outline_bbox: &BoundingBox) -> Option<DrillData> {
    let cx = (outline_bbox.min.x + outline_bbox.max.x) * 0.5;
    let cy = (outline_bbox.min.y + outline_bbox.max.y) * 0.5;
    let holes: Vec<Hole> = raw
        .into_iter()
        .filter(|(_, _, dia)| *dia > 0.0)
        .map(|(x, y, dia)| Hole {
            center: [(x - cx) as f32, (y - cy) as f32],
            radius: (dia * 0.5) as f32,
        })
        .collect();
    if holes.is_empty() {
        None
    } else {
        Some(DrillData { holes })
    }
}

// ────────────────────────────────────────────────────────────────────────
// Excellon parser
//
// Handles KiCad's default output: `M48` header, `INCH`/`METRIC` units,
// `TnC<dia>` tool defs, decimal coordinates with modal X/Y, `G90` absolute.
// Returns raw `(x, y, diameter)` triples already scaled to mm.
// ────────────────────────────────────────────────────────────────────────

fn parse_excellon(text: &str) -> Vec<(f64, f64, f64)> {
    use std::collections::HashMap;

    let mut unit_scale = 1.0_f64; // mm by default
    let mut tools: HashMap<u32, f64> = HashMap::new(); // tool id → diameter (file unit)
    let mut cur_dia: Option<f64> = None;
    let mut cur_x = 0.0_f64;
    let mut cur_y = 0.0_f64;
    let mut in_header = true;
    let mut out: Vec<(f64, f64, f64)> = Vec::new();

    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with(';') {
            continue;
        }
        // Units (header or inline M-codes).
        if line == "INCH" || line.starts_with("INCH") || line == "M72" {
            unit_scale = 25.4;
            continue;
        }
        if line == "METRIC" || line.starts_with("METRIC") || line == "M71" {
            unit_scale = 1.0;
            continue;
        }
        if line == "M48" {
            in_header = true;
            continue;
        }
        if line == "%" || line == "G90" || line == "G05" || line == "G90G05" {
            // End of header / mode set — coordinates follow.
            in_header = false;
            continue;
        }
        if line == "M30" || line == "M00" {
            break;
        }

        // Tool definition (header) vs tool select (body) — both start `T`.
        if let Some(rest) = line.strip_prefix('T') {
            let tool_id: u32 = rest
                .chars()
                .take_while(|c| c.is_ascii_digit())
                .collect::<String>()
                .parse()
                .unwrap_or(0);
            if let Some(cpos) = rest.find('C') {
                // Definition: `T1C0.0220` — capture the diameter.
                let dia = read_number(&rest[cpos + 1..]).unwrap_or(0.0);
                tools.insert(tool_id, dia);
                if !in_header {
                    cur_dia = Some(dia);
                }
            } else {
                // Selection: `T1` — set current diameter from the table.
                cur_dia = tools.get(&tool_id).copied();
            }
            continue;
        }

        // Coordinate line — only meaningful in the body with a tool active.
        if (line.starts_with('X') || line.starts_with('Y')) && !in_header {
            if let Some(x) = read_coord(line, 'X') {
                cur_x = x;
            }
            if let Some(y) = read_coord(line, 'Y') {
                cur_y = y;
            }
            if let Some(dia) = cur_dia {
                out.push((cur_x * unit_scale, cur_y * unit_scale, dia * unit_scale));
            }
            continue;
        }
    }

    out
}

/// Read the signed decimal that follows `axis` in an Excellon coordinate
/// line, e.g. `read_coord("X3.532Y-1.673", 'Y') == Some(-1.673)`. Only the
/// decimal-point form (KiCad's default) is supported; coordinate blocks
/// without a `.` are skipped rather than mis-scaled.
fn read_coord(line: &str, axis: char) -> Option<f64> {
    let idx = line.find(axis)?;
    let tail = &line[idx + 1..];
    // Stop at the next axis/command letter.
    let end = tail
        .find(|c: char| c.is_ascii_alphabetic())
        .unwrap_or(tail.len());
    let token = &tail[..end];
    if !token.contains('.') {
        return None;
    }
    token.parse().ok()
}

/// Read a leading signed decimal from `s` (used for tool diameters).
fn read_number(s: &str) -> Option<f64> {
    let end = s
        .find(|c: char| !(c.is_ascii_digit() || c == '.' || c == '-'))
        .unwrap_or(s.len());
    s[..end].parse().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_kicad_inch_excellon() {
        let sample = "M48\nINCH\nT1C0.0220\nT2C0.0400\n%\nG90\nG05\nT1\nX3.532Y-1.673\nX3.532Y-1.798\nT2\nX4.024Y-1.2826\nM30\n";
        let holes = parse_excellon(sample);
        assert_eq!(holes.len(), 3, "three holes across two tools");
        // First hole: 0.022 in dia → 0.5588 mm; X 3.532 in → 89.7128 mm.
        let (x, _y, dia) = holes[0];
        assert!((dia - 0.022 * 25.4).abs() < 1e-6, "diameter scaled to mm");
        assert!((x - 3.532 * 25.4).abs() < 1e-3, "X scaled to mm");
        // Third hole uses T2's 0.040 in diameter.
        assert!((holes[2].2 - 0.040 * 25.4).abs() < 1e-6);
    }

    #[test]
    fn modal_coordinates_carry_over() {
        // Second line omits X — should reuse the previous X.
        let sample = "M48\nMETRIC\nT1C0.300\n%\nG90\nT1\nX10.0Y20.0\nY25.0\nM30\n";
        let holes = parse_excellon(sample);
        assert_eq!(holes.len(), 2);
        assert!((holes[1].0 - 10.0).abs() < 1e-6, "X carried over");
        assert!((holes[1].1 - 25.0).abs() < 1e-6, "Y updated");
    }
}
