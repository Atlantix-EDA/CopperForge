//! Parse the `*-centroid.csv` (CPL) entry from a CopperForge release zip.
//!
//! Format is the one `copperforge-core::export::centroid::write_cpl_csv`
//! emits: header row `Designator,Mid X,Mid Y,Layer,Rotation`, then one
//! row per component placement. Layer is `Top` or `Bottom`.
//!
//! We only need the side breakdown for the Board stats panel + the
//! PCBWay fab-specs sheet — designator/coords are kept on the entry
//! struct so future features (search, highlight) can use them.

use std::collections::BTreeMap;

// Designator/coords/rotation are kept for forthcoming features
// (component highlight, hover-to-identify), so the dead-code lint
// would fire otherwise. The `side` field is what drives the
// component-count tally today.
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct CentroidEntry {
    pub designator: String,
    pub x_mm: f64,
    pub y_mm: f64,
    pub side: Side,
    pub rotation_deg: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Side {
    Top,
    Bottom,
}

#[derive(Debug, Clone, Default)]
pub struct ComponentCounts {
    pub total: usize,
    pub top: usize,
    pub bottom: usize,
}

impl ComponentCounts {
    pub fn from_entries(entries: &[CentroidEntry]) -> Self {
        let mut c = ComponentCounts::default();
        for e in entries {
            c.total += 1;
            match e.side {
                Side::Top => c.top += 1,
                Side::Bottom => c.bottom += 1,
            }
        }
        c
    }
}

/// Find the centroid CSV inside an unpacked release zip and parse it.
///
/// Matches loosely — any `.csv` whose lowercase basename contains
/// `centroid`, `cpl`, or `pick`. Catches CopperForge's own
/// `<project>-centroid.csv`, JLCPCB-style `<project>_cpl.csv`, and
/// generic `pick_and_place.csv`. Returns `None` (the normal case for
/// a gerber-only bundle) when no candidate file is present.
pub fn find_and_parse(entries: &BTreeMap<String, Vec<u8>>) -> Option<Vec<CentroidEntry>> {
    let (name, bytes) = entries.iter().find(|(name, _)| {
        let l = name.to_lowercase();
        if !l.ends_with(".csv") {
            return false;
        }
        l.contains("centroid") || l.contains("cpl") || l.contains("pick")
    })?;
    let text = std::str::from_utf8(bytes).ok()?;
    log::info!("Found centroid CSV: {}", name);
    Some(parse(text))
}

/// Parse the CSV body. Tolerant of leading whitespace, trailing
/// commas, BOM markers, and extra columns; unparseable rows are
/// silently skipped rather than aborting the whole file.
pub fn parse(text: &str) -> Vec<CentroidEntry> {
    let mut out = Vec::new();
    let mut lines = text.lines();

    // Skip an optional BOM-prefixed first line if it looks like a header.
    if let Some(first) = lines.next() {
        let trimmed = first.trim_start_matches('\u{FEFF}');
        if !trimmed
            .to_lowercase()
            .starts_with("designator,")
        {
            // First line wasn't a header — parse it as data instead.
            if let Some(entry) = parse_row(trimmed) {
                out.push(entry);
            }
        }
    }

    for line in lines {
        if line.trim().is_empty() {
            continue;
        }
        if let Some(entry) = parse_row(line) {
            out.push(entry);
        }
    }
    out
}

fn parse_row(line: &str) -> Option<CentroidEntry> {
    // Naive split — the writer doesn't emit quoted fields with commas
    // for any of these columns, so a plain split is safe.
    let cols: Vec<&str> = line.split(',').collect();
    if cols.len() < 4 {
        return None;
    }
    let designator = cols[0].trim().to_string();
    let x_mm = cols[1].trim().parse().ok()?;
    let y_mm = cols[2].trim().parse().ok()?;
    let side = match cols[3].trim().to_ascii_lowercase().as_str() {
        "top" | "f.cu" | "f_cu" => Side::Top,
        "bottom" | "b.cu" | "b_cu" => Side::Bottom,
        _ => return None,
    };
    let rotation_deg = cols
        .get(4)
        .and_then(|c| c.trim().parse().ok())
        .unwrap_or(0.0);
    Some(CentroidEntry {
        designator,
        x_mm,
        y_mm,
        side,
        rotation_deg,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_standard_format() {
        let csv = "Designator,Mid X,Mid Y,Layer,Rotation\n\
                   R1,10.5,20.3,Top,90\n\
                   C1,15.2,25.1,Bottom,0\n\
                   U1,30.0,40.0,Top,180\n";
        let entries = parse(csv);
        assert_eq!(entries.len(), 3);
        let counts = ComponentCounts::from_entries(&entries);
        assert_eq!(counts.total, 3);
        assert_eq!(counts.top, 2);
        assert_eq!(counts.bottom, 1);
    }

    #[test]
    fn tolerates_bom_and_blanks() {
        let csv = "\u{FEFF}Designator,Mid X,Mid Y,Layer,Rotation\n\n\
                   R1,1.0,2.0,Top,0\n\
                   \n";
        let entries = parse(csv);
        assert_eq!(entries.len(), 1);
    }
}
