//! Parse a `*-bom.csv` entry from the uploaded release zip and
//! classify each component as SMT or through-hole from its Package /
//! Footprint column. The result is a designator → mount-type map the
//! Board stats panel and PCBWay fab-specs sheet can join against the
//! centroid CSV (which has designator → side).
//!
//! Header detection is loose — accepts CopperForge's canonical column
//! order (`Item,Quantity,Value,Package,Manufacturer,...,Designators`)
//! and JLCPCB-style variants. The CSV parser handles double-quoted
//! fields properly so Description text with commas doesn't shift the
//! column count.

use std::collections::{BTreeMap, HashMap, HashSet};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mount {
    Smt,
    Tht,
    /// Couldn't classify from the footprint name alone — mostly user
    /// libraries with non-standard suffixes. Counts toward "Total"
    /// but neither SMT nor THT totals.
    Unknown,
}

/// Find the BOM CSV inside an unpacked release zip and produce a
/// `designator → Mount` map. Returns empty if no BOM is present (the
/// SMT/THT split in the stats panel falls back to "unavailable").
pub fn find_and_parse(entries: &BTreeMap<String, Vec<u8>>) -> HashMap<String, Mount> {
    let candidate = entries.iter().find(|(name, _)| {
        let l = name.to_lowercase();
        l.ends_with(".csv") && l.contains("bom")
    });
    let Some((name, bytes)) = candidate else {
        return HashMap::new();
    };
    let Ok(text) = std::str::from_utf8(bytes) else {
        log::warn!("BOM CSV {} is not valid UTF-8 — skipping", name);
        return HashMap::new();
    };
    log::info!("Found BOM CSV: {}", name);
    parse(text)
}

/// Parse the CSV text. Expects a header row; columns identified by
/// lowercase substring match (`package`/`footprint` for the mount
/// classifier, `designator`/`reference` for the designator list).
pub fn parse(text: &str) -> HashMap<String, Mount> {
    let mut out = HashMap::new();
    let mut lines = text.lines();

    let Some(header_line) = lines.next() else {
        return out;
    };
    // Strip BOM marker if present.
    let header_line = header_line.trim_start_matches('\u{FEFF}');
    let header_cols = parse_csv_row(header_line);
    let pkg_idx = header_cols.iter().position(|c| {
        let l = c.trim().to_lowercase();
        l == "package" || l == "footprint" || l.contains("footprint")
    });
    let des_idx = header_cols.iter().position(|c| {
        let l = c.trim().to_lowercase();
        l == "designators"
            || l == "designator"
            || l == "reference"
            || l == "references"
            || l == "refdes"
    });
    let (Some(pkg_idx), Some(des_idx)) = (pkg_idx, des_idx) else {
        log::warn!(
            "BOM header missing Package/Footprint or Designators column — got {:?}",
            header_cols
        );
        return out;
    };

    for line in lines {
        if line.trim().is_empty() {
            continue;
        }
        let cols = parse_csv_row(line);
        let Some(package) = cols.get(pkg_idx) else { continue };
        let Some(designators) = cols.get(des_idx) else { continue };
        let mount = classify_package(package);
        // BOM's Designators column is space-separated refs from
        // copperforge-core's writer (`r.designators.join(" ")`).
        // Commas would also be tolerated since they'd land in
        // separate splits.
        for d in designators.split(|c: char| c.is_whitespace() || c == ',') {
            let d = d.trim();
            if !d.is_empty() {
                out.insert(d.to_string(), mount);
            }
        }
    }
    out
}

/// Aggregate grouped-part statistics for the v1 manufacturability
/// metric. A "unique part" is a distinct identity key — the MPN when
/// the BOM carries one, otherwise `(value, normalized-package)` (the
/// same join key `copperforge-core::export::bom::group_bom` uses).
///
/// `total_parts` is the placed-component count (sum of designators), so
/// `unique / total` measures part *commonality*: fewer unique parts per
/// placement = fewer pick-and-place feeders = easier to assemble.
#[derive(Debug, Clone, Default)]
pub struct PartStats {
    pub unique_parts: usize,
    pub total_parts: usize,
    pub smt_parts: usize,
    pub tht_parts: usize,
    pub unknown_parts: usize,
    /// True when an MPN column drove the uniqueness key; false when we
    /// fell back to `(value, package)`. Informational for the panel.
    pub keyed_by_mpn: bool,
}

impl PartStats {
    /// Unique-to-total ratio in `0..=1`. Lower = more reuse = easier.
    pub fn unique_ratio(&self) -> f32 {
        if self.total_parts == 0 {
            0.0
        } else {
            self.unique_parts as f32 / self.total_parts as f32
        }
    }

    /// Average placements per unique part (`total / unique`). The
    /// intuitive inverse of the ratio: "4.7× means each part type is
    /// placed ~4.7 times."
    pub fn reuse(&self) -> f32 {
        if self.unique_parts == 0 {
            0.0
        } else {
            self.total_parts as f32 / self.unique_parts as f32
        }
    }

    /// Through-hole fraction of placements (manual / selective solder).
    pub fn tht_fraction(&self) -> f32 {
        if self.total_parts == 0 {
            0.0
        } else {
            self.tht_parts as f32 / self.total_parts as f32
        }
    }
}

/// Find the BOM CSV and compute grouped-[`PartStats`]. Returns `None`
/// when there's no BOM, it isn't UTF-8, or it lacks a Value column
/// (without a value, footprint alone can't distinguish parts — all
/// 0402s are not one part). The panel shows "needs a Value column" then.
pub fn find_and_parse_parts(entries: &BTreeMap<String, Vec<u8>>) -> Option<PartStats> {
    let (name, bytes) = entries.iter().find(|(name, _)| {
        let l = name.to_lowercase();
        l.ends_with(".csv") && l.contains("bom")
    })?;
    let text = std::str::from_utf8(bytes)
        .map_err(|_| log::warn!("BOM CSV {} is not valid UTF-8 — skipping", name))
        .ok()?;
    parse_parts(text)
}

/// Group the BOM rows into unique parts and tally placements. `None` if
/// the header has no Value column or no rows yield placements.
pub fn parse_parts(text: &str) -> Option<PartStats> {
    let mut lines = text.lines();
    let header_line = lines.next()?.trim_start_matches('\u{FEFF}');
    let header = parse_csv_row(header_line);

    let col = |pred: &dyn Fn(&str) -> bool| {
        header.iter().position(|c| pred(&c.trim().to_lowercase()))
    };
    let pkg_idx = col(&|l| l == "package" || l == "footprint" || l.contains("footprint"))?;
    let des_idx = col(&|l| {
        l == "designators" || l == "designator" || l == "reference"
            || l == "references" || l == "refdes"
    })?;
    // Value is required — it's the heart of the uniqueness key.
    let val_idx = col(&|l| l == "value" || l == "val" || l == "comment")?;
    // MPN is preferred when present, but optional.
    let mpn_idx = col(&|l| {
        l == "mpn" || l == "part number" || l == "part#" || l == "part_number"
            || l.contains("manufacturer p/n") || l.contains("mfr p/n")
            || l.contains("manufacturer part")
    });

    let mut keys: HashSet<String> = HashSet::new();
    let mut stats = PartStats {
        keyed_by_mpn: mpn_idx.is_some(),
        ..Default::default()
    };

    for line in lines {
        if line.trim().is_empty() {
            continue;
        }
        let cols = parse_csv_row(line);
        let (Some(package), Some(value), Some(designators)) =
            (cols.get(pkg_idx), cols.get(val_idx), cols.get(des_idx))
        else {
            continue;
        };
        let count = designators
            .split(|c: char| c.is_whitespace() || c == ',')
            .filter(|d| !d.trim().is_empty())
            .count();
        if count == 0 {
            continue;
        }

        // Key: MPN when present + non-empty, else value + normalized package.
        let key = mpn_idx
            .and_then(|i| cols.get(i))
            .map(|m| m.trim())
            .filter(|m| !m.is_empty())
            .map(|m| m.to_lowercase())
            .unwrap_or_else(|| {
                format!("{}|{}", value.trim().to_lowercase(), normalize_package(package))
            });
        keys.insert(key);

        stats.total_parts += count;
        match classify_package(package) {
            Mount::Smt => stats.smt_parts += count,
            Mount::Tht => stats.tht_parts += count,
            Mount::Unknown => stats.unknown_parts += count,
        }
    }

    if stats.total_parts == 0 {
        return None;
    }
    stats.unique_parts = keys.len();
    Some(stats)
}

/// Strip a `library:` prefix and lower-case so `Resistor_SMD:R_0603`
/// and `R_0603` group together.
fn normalize_package(pkg: &str) -> String {
    pkg.rsplit(':').next().unwrap_or(pkg).trim().to_lowercase()
}

/// Classify a KiCad footprint name as SMT / THT / Unknown based on
/// substring matching against the package text. Order matters: SMT
/// signals are checked first because they're more specific (a 4-digit
/// imperial code is unambiguously surface-mount), then THT signals,
/// then the fall-through.
pub fn classify_package(package: &str) -> Mount {
    let l = package.to_lowercase();

    // ── SMT signals ────────────────────────────────────────────────
    // KiCad library prefixes that put SMD right in the name.
    if l.contains("_smd") || l.contains("smd:") || l.contains("smt:") {
        return Mount::Smt;
    }
    // Imperial chip codes (passives).
    for code in [
        "0201", "0402", "0603", "0805", "1206", "1210", "1812", "2010", "2512",
    ] {
        if l.contains(code) {
            return Mount::Smt;
        }
    }
    // Common SMT IC families.
    for s in [
        "soic", "sop", "tsop", "tssop", "msop", "ssop",
        "qfp", "lqfp", "tqfp", "pqfp",
        "qfn", "dfn", "lga", "bga", "csp",
        "sot-23", "sot-89", "sot-223", "sot-353", "sot-553", "sot-666",
        "sot23", "sot89", "sot223",
        "dpak", "d-pak", "d2pak", "to-252", "to-263",
        "wson", "uson", "uqfn",
    ] {
        if l.contains(s) {
            return Mount::Smt;
        }
    }

    // ── THT signals ───────────────────────────────────────────────
    for s in [
        "pinheader", "pin_header",
        "pinsocket", "pin_socket",
        "_tht", "_dip", ":dip-", "dip_",
        "terminalblock", "terminal_block",
        "to-92", "to-220", "to-247", "to-218", "to-3",
        "_sip", "thru", "through_hole", "thruhole",
        "screw_terminal", "screwterminal",
        "barrel", "barrel_jack",
    ] {
        if l.contains(s) {
            return Mount::Tht;
        }
    }

    Mount::Unknown
}

/// Minimal RFC-4180 CSV row parser. Handles quoted fields, escaped
/// double-quotes (`""`), and bare unquoted fields. Returns the
/// columns in order.
fn parse_csv_row(line: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut field = String::new();
    let mut in_quotes = false;
    let mut chars = line.chars().peekable();
    while let Some(c) = chars.next() {
        if in_quotes {
            if c == '"' {
                if chars.peek() == Some(&'"') {
                    field.push('"');
                    chars.next();
                } else {
                    in_quotes = false;
                }
            } else {
                field.push(c);
            }
        } else if c == ',' {
            out.push(std::mem::take(&mut field));
        } else if c == '"' && field.is_empty() {
            in_quotes = true;
        } else {
            field.push(c);
        }
    }
    out.push(field);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classify_smt_passives() {
        assert_eq!(
            classify_package("Resistor_SMD:R_0603_1608Metric"),
            Mount::Smt
        );
        assert_eq!(
            classify_package("Capacitor_SMD:C_0402_1005Metric"),
            Mount::Smt
        );
        assert_eq!(classify_package("R_0805"), Mount::Smt);
    }

    #[test]
    fn classify_smt_ics() {
        assert_eq!(classify_package("Package_SO:SOIC-8_3.9x4.9mm"), Mount::Smt);
        assert_eq!(classify_package("Package_QFP:LQFP-100"), Mount::Smt);
        assert_eq!(classify_package("Package_DFN_QFN:QFN-32"), Mount::Smt);
        assert_eq!(classify_package("Package_TO_SOT_SMD:SOT-23"), Mount::Smt);
    }

    #[test]
    fn classify_tht() {
        assert_eq!(
            classify_package("Connector_PinHeader_2.54mm:PinHeader_1x04_P2.54mm_Vertical"),
            Mount::Tht
        );
        assert_eq!(classify_package("Package_DIP:DIP-8_W7.62mm"), Mount::Tht);
        assert_eq!(classify_package("TerminalBlock:TerminalBlock_2-pin"), Mount::Tht);
        assert_eq!(classify_package("Package_TO_SOT_THT:TO-220-3"), Mount::Tht);
    }

    #[test]
    fn parse_with_quoted_description_field() {
        // Description column has a comma — must not shift Designators.
        let csv = "Item,Quantity,Value,Package,Manufacturer,Manufacturer P/N,Description,Datasheet,Designators\n\
                   1,3,10k,R_0603,Yageo,RC0603,\"Resistor, 10k, 1%\",,R1 R2 R3\n\
                   2,1,DIP-8,Package_DIP:DIP-8_W7.62mm,,,Op-amp 8-pin DIP,,U1\n";
        let map = parse(csv);
        assert_eq!(map.get("R1"), Some(&Mount::Smt));
        assert_eq!(map.get("R2"), Some(&Mount::Smt));
        assert_eq!(map.get("R3"), Some(&Mount::Smt));
        assert_eq!(map.get("U1"), Some(&Mount::Tht));
    }

    #[test]
    fn part_stats_groups_and_counts() {
        // Two distinct R values (one with 3 placements), plus a DIP.
        // 3 unique parts across 5 placements; 4 SMT + 1 THT.
        let csv = "Item,Quantity,Value,Package,Manufacturer P/N,Designators\n\
                   1,3,10k,R_0603,RC0603-10K,R1 R2 R3\n\
                   2,1,1k,R_0603,RC0603-1K,R4\n\
                   3,1,DIP-8,Package_DIP:DIP-8_W7.62mm,OP07,U1\n";
        let s = parse_parts(csv).expect("has a Value column");
        assert_eq!(s.unique_parts, 3);
        assert_eq!(s.total_parts, 5);
        assert_eq!(s.smt_parts, 4);
        assert_eq!(s.tht_parts, 1);
        assert!(s.keyed_by_mpn);
        assert!((s.reuse() - 5.0 / 3.0).abs() < 1e-4);
    }

    #[test]
    fn part_stats_none_without_value_column() {
        // Footprint-only BOM: can't tell parts apart → None.
        let csv = "Package,Designators\nR_0603,R1 R2\n";
        assert!(parse_parts(csv).is_none());
    }

    #[test]
    fn part_stats_falls_back_to_value_package_key() {
        // No MPN column → key on (value, package); same value+package
        // across rows collapses to one unique part.
        let csv = "Value,Package,Designators\n\
                   10k,Resistor_SMD:R_0603_1608Metric,R1\n\
                   10k,R_0603,R2\n";
        let s = parse_parts(csv).expect("has a Value column");
        assert_eq!(s.unique_parts, 1); // normalized package collapses the prefix
        assert_eq!(s.total_parts, 2);
        assert!(!s.keyed_by_mpn);
    }
}
