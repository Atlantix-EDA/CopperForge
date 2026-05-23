//! Symbol-library metadata lookup.
//!
//! A KiCad board's footprints carry only Reference / Value / Footprint /
//! Description. The richer fields a real BOM needs — Manufacturer,
//! manufacturer part number, a curated Description — live in the **symbol
//! library**. This module reads KiCad's symbol library tables, parses every
//! referenced `.kicad_sym` file, and builds an index so the BOM export can be
//! enriched without touching the board or schematic.
//!
//! Symbols are indexed by `(value, footprint-name)` because a symbol's name is
//! not a reliable join key — generic passive symbols are named by package
//! (`R0603_1.00`) while their `Value` property holds the actual value
//! (`1.00`), whereas per-part symbols (`Murata_GRM188R71C225KE15`) use the
//! part number for both. The `(value, footprint)` pair matches a board
//! component's footprint Value + Footprint fields in every case.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::export::footprint_name;

/// Metadata pulled from a library symbol's properties.
#[derive(Debug, Clone, Default)]
pub struct SymbolMeta {
    pub description: String,
    pub manufacturer: String,
    pub mpn: String,
    pub datasheet: String,
}

/// Index keyed by `(value, footprint-name)`.
pub type SymbolIndex = HashMap<(String, String), SymbolMeta>;

/// Build a symbol-metadata index from KiCad's global symbol library tables.
///
/// Reads every `~/.config/kicad/<version>/sym-lib-table`, follows each
/// `(lib ... (uri ...))` entry to its `.kicad_sym` file and indexes every
/// symbol. Unreadable tables and missing files are skipped silently — the BOM
/// still exports, just without enrichment.
pub fn load_global_symbol_index() -> SymbolIndex {
    let mut index = SymbolIndex::new();
    for table in global_sym_lib_tables() {
        for lib in parse_sym_lib_table(&table) {
            index_kicad_sym(&lib, &mut index);
        }
    }
    index
}

/// All `~/.config/kicad/<version>/sym-lib-table` files found on disk.
fn global_sym_lib_tables() -> Vec<PathBuf> {
    let mut tables = Vec::new();
    if let Some(home) = dirs::home_dir() {
        if let Ok(entries) = std::fs::read_dir(home.join(".config/kicad")) {
            for e in entries.flatten() {
                let t = e.path().join("sym-lib-table");
                if t.is_file() {
                    tables.push(t);
                }
            }
        }
    }
    tables
}

/// Extract the absolute `.kicad_sym` paths from a `sym-lib-table` file.
///
/// Env-var-templated uris (KiCad's bundled defaults) are skipped — only the
/// user's own absolute-path libraries are indexed.
fn parse_sym_lib_table(table: &Path) -> Vec<PathBuf> {
    let content = match std::fs::read_to_string(table) {
        Ok(c) => c,
        Err(_) => return Vec::new(),
    };
    let mut paths = Vec::new();
    for line in content.lines() {
        if !line.contains("(type \"KiCad\")") {
            continue;
        }
        if let Some(uri) = quoted_after(line, "(uri \"") {
            if uri.ends_with(".kicad_sym") && !uri.contains("${") {
                paths.push(PathBuf::from(uri));
            }
        }
    }
    paths
}

/// Properties collected for one library symbol while scanning.
#[derive(Default)]
struct SymbolBuild {
    value: String,
    footprint: String,
    description: String,
    manufacturer: String,
    mpn: String,
    datasheet: String,
}

impl SymbolBuild {
    fn commit(self, index: &mut SymbolIndex) {
        if self.value.is_empty() {
            return;
        }
        let key = (self.value, footprint_name(&self.footprint).to_string());
        index.insert(
            key,
            SymbolMeta {
                description: self.description,
                manufacturer: self.manufacturer,
                mpn: self.mpn,
                datasheet: self.datasheet,
            },
        );
    }
}

/// Index every top-level symbol in a `.kicad_sym` file.
///
/// Whitespace-agnostic: nesting depth is tracked by counting parentheses,
/// because some KiCad libraries indent with tabs and others with spaces. A
/// top-level symbol is a `(symbol ...)` list at depth 2; its fields are
/// `(property ...)` lists at depth 3 — per-unit sub-symbols sit deeper and are
/// ignored.
fn index_kicad_sym(path: &Path, index: &mut SymbolIndex) {
    let content = match std::fs::read_to_string(path) {
        Ok(c) => c,
        Err(_) => return,
    };
    let bytes = content.as_bytes();
    let mut depth: usize = 0;
    let mut in_string = false;
    let mut escape = false;
    let mut current: Option<SymbolBuild> = None;
    let mut i = 0;

    while i < bytes.len() {
        let c = bytes[i];
        if in_string {
            if escape {
                escape = false;
            } else if c == b'\\' {
                escape = true;
            } else if c == b'"' {
                in_string = false;
            }
            i += 1;
            continue;
        }
        match c {
            b'"' => in_string = true,
            b')' => depth = depth.saturating_sub(1),
            b'(' => {
                let open_depth = depth + 1;
                let head = head_token(&content, i + 1);
                if head == "symbol" && open_depth == 2 {
                    if let Some(b) = current.take() {
                        b.commit(index);
                    }
                    current = Some(SymbolBuild::default());
                } else if head == "property" && open_depth == 3 {
                    if let Some(b) = current.as_mut() {
                        let (key, val) = two_quoted(&content, i + 1);
                        // Field names are not standardised across KiCad
                        // libraries: parts imported from different sources use
                        // `Manufacturer` / `MANUFACTURER_NAME` / `MF`,
                        // `Part Number` / `MPN` / `MP`, etc. Match on a
                        // normalised key, and keep the first non-empty hit.
                        match norm_key(&key).as_str() {
                            "VALUE" if b.value.is_empty() => b.value = val,
                            "FOOTPRINT" if b.footprint.is_empty() => {
                                b.footprint = val
                            }
                            "DESCRIPTION" if b.description.is_empty() => {
                                b.description = clean(&val)
                            }
                            "MANUFACTURER" | "MANUFACTURERNAME" | "MF" | "MFR"
                            | "MFG"
                                if b.manufacturer.is_empty() =>
                            {
                                b.manufacturer = clean(&val)
                            }
                            "PARTNUMBER" | "MPN" | "MANUFACTURERPARTNUMBER"
                            | "MFRPARTNUMBER" | "MFRPN" | "MP"
                                if b.mpn.is_empty() =>
                            {
                                b.mpn = clean(&val)
                            }
                            "DATASHEET" if b.datasheet.is_empty() => {
                                // KiCad's default placeholder; treat as empty.
                                if val != "~" {
                                    b.datasheet = clean(&val)
                                }
                            }
                            _ => {}
                        }
                    }
                }
                depth += 1;
            }
            _ => {}
        }
        i += 1;
    }
    if let Some(b) = current.take() {
        b.commit(index);
    }
}

/// The bare token immediately after a `(` at byte index `from`.
fn head_token(content: &str, from: usize) -> &str {
    let bytes = content.as_bytes();
    let mut j = from;
    while j < bytes.len() {
        let c = bytes[j];
        if c.is_ascii_whitespace() || c == b'(' || c == b')' || c == b'"' {
            break;
        }
        j += 1;
    }
    &content[from..j]
}

/// The first two quoted strings at or after byte index `from`.
fn two_quoted(content: &str, from: usize) -> (String, String) {
    let bytes = content.as_bytes();
    let mut out: Vec<String> = Vec::new();
    let mut i = from;
    while i < bytes.len() && out.len() < 2 {
        if bytes[i] == b'"' {
            let start = i + 1;
            let mut j = start;
            while j < bytes.len() {
                if bytes[j] == b'\\' {
                    j += 2;
                    continue;
                }
                if bytes[j] == b'"' {
                    break;
                }
                j += 1;
            }
            let end = j.min(bytes.len());
            out.push(content[start..end].to_string());
            i = end + 1;
        } else {
            i += 1;
        }
    }
    (
        out.first().cloned().unwrap_or_default(),
        out.get(1).cloned().unwrap_or_default(),
    )
}

/// The contents of the first `"..."` string that follows `marker` in `line`.
fn quoted_after<'a>(line: &'a str, marker: &str) -> Option<&'a str> {
    let after = &line[line.find(marker)? + marker.len()..];
    let end = after.find('"')?;
    Some(&after[..end])
}

/// Normalise a property key for matching: uppercase, alphanumeric only
/// (so `Manufacturer`, `MANUFACTURER_NAME` and `Mfr` are comparable).
fn norm_key(k: &str) -> String {
    k.chars()
        .filter(|c| c.is_alphanumeric())
        .flat_map(|c| c.to_uppercase())
        .collect()
}

/// Collapse all whitespace runs (incl. newlines) to single spaces and trim;
/// SnapEDA-imported descriptions are often wrapped in newlines.
fn clean(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}
