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
    /// Distributor part numbers as `(vendor, part_number)`, e.g.
    /// `("Digi-Key", "541-301CCT-ND")`. Digi-Key and Mouser are the
    /// standard pair, but the list is open — Newark, Arrow, LCSC, … land
    /// here too as libraries start carrying their fields. Ordered with
    /// Digi-Key and Mouser first (see `commit`).
    pub vendors: Vec<(String, String)>,
}

/// Canonical distributor names and the normalised key prefix that identifies
/// their part-number fields. New distributors are a one-line addition here.
const KNOWN_VENDORS: &[(&str, &str)] = &[
    ("Digi-Key", "DIGIKEY"),
    ("Mouser", "MOUSER"),
    ("Newark", "NEWARK"),
    ("Arrow", "ARROW"),
    ("LCSC", "LCSC"),
    ("Farnell", "FARNELL"),
];

/// Output ordering rank: the standard pair first, everything else after in
/// first-seen order (a stable sort preserves it).
fn vendor_rank(name: &str) -> u8 {
    match name {
        "Digi-Key" => 0,
        "Mouser" => 1,
        _ => 2,
    }
}

/// If `norm` (a normalised property key) names a distributor's part-number
/// field, return that distributor's canonical name. Matches the bare vendor
/// (`MOUSER`) and the common P/N suffixes (`MOUSERPARTNUMBER`, `MOUSERPN`, …).
fn vendor_field(norm: &str) -> Option<&'static str> {
    for (display, prefix) in KNOWN_VENDORS {
        if let Some(rest) = norm.strip_prefix(prefix) {
            if matches!(rest, "" | "PN" | "PARTNUMBER" | "PARTNO" | "PARTNUM") {
                return Some(display);
            }
        }
    }
    None
}

/// Map a free-text `Supplier` value (e.g. "Digi-Key", "digikey") to a canonical
/// distributor name, so a generic Supplier/SupplierPN pair routes correctly.
fn canonical_vendor(supplier: &str) -> Option<&'static str> {
    let n = norm_key(supplier);
    KNOWN_VENDORS
        .iter()
        .find(|(_, prefix)| n.starts_with(prefix))
        .map(|(display, _)| *display)
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
    // Vendor data is split across libraries: the atlantix-eda resistor libs
    // carry a generic Supplier="Digi-Key" / SupplierPN pair, while capacitor
    // libs carry an explicit "Mouser Part Number". Collect both shapes and
    // resolve to a canonical (vendor, pn) list at commit().
    supplier: String,
    supplier_pn: String,
    vendor_pns: Vec<(String, String)>,
}

impl SymbolBuild {
    fn commit(self, index: &mut SymbolIndex) {
        if self.value.is_empty() {
            return;
        }
        // Explicit per-vendor fields first, then the generic Supplier pair
        // (routed to a canonical vendor, or kept verbatim if unrecognised).
        let mut vendors = self.vendor_pns;
        if !self.supplier_pn.is_empty() {
            let vendor = canonical_vendor(&self.supplier)
                .map(str::to_string)
                .unwrap_or(self.supplier);
            if !vendor.is_empty() && !vendors.iter().any(|(v, _)| *v == vendor) {
                vendors.push((vendor, self.supplier_pn));
            }
        }
        vendors.retain(|(_, pn)| !pn.is_empty());
        vendors.sort_by_key(|(v, _)| vendor_rank(v));
        let key = (self.value, footprint_name(&self.footprint).to_string());
        index.insert(
            key,
            SymbolMeta {
                description: self.description,
                manufacturer: self.manufacturer,
                mpn: self.mpn,
                datasheet: self.datasheet,
                vendors,
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
                        let nk = norm_key(&key);
                        match nk.as_str() {
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
                            // Generic supplier pair (atlantix-eda resistor libs).
                            "SUPPLIER" if b.supplier.is_empty() => {
                                b.supplier = clean(&val)
                            }
                            "SUPPLIERPN" | "SUPPLIERPARTNUMBER"
                                if b.supplier_pn.is_empty() =>
                            {
                                b.supplier_pn = clean(&val)
                            }
                            // Explicit per-vendor part-number fields, e.g.
                            // "Digi-Key Part Number" / "Mouser Part Number".
                            // First non-empty value per vendor wins.
                            _ => {
                                if let Some(vendor) = vendor_field(&nk) {
                                    if !b.vendor_pns.iter().any(|(v, _)| v == vendor) {
                                        b.vendor_pns
                                            .push((vendor.to_string(), clean(&val)));
                                    }
                                }
                            }
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
