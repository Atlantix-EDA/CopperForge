//! BOM export to CSV and XLSX.
//!
//! Groups the per-component [`BomEntry`] list into BOM line items — one row per
//! unique value + footprint, with a quantity and a designator list.
//!
//! Each line is enriched from the KiCad symbol libraries (see
//! [`crate::export::symbol_meta`]): a board's footprints only carry
//! Reference / Value / Footprint / Description, so Manufacturer, manufacturer
//! part number and a curated Description are looked up from the library symbol
//! whose name matches the component's `Value`.

use std::collections::HashMap;
use std::path::Path;

use crate::bom::BomEntry;
use crate::export::csv_field;
use crate::export::symbol_meta::{load_global_symbol_index, SymbolIndex};

/// A grouped BOM line: all components sharing a value and footprint.
pub struct BomLine {
    pub item: usize,
    pub quantity: usize,
    pub value: String,
    /// Physical package (e.g. `0603`), derived from the footprint name.
    pub package: String,
    /// Manufacturer, from the library symbol metadata.
    pub manufacturer: String,
    /// Manufacturer part number, from the library symbol metadata.
    pub mpn: String,
    pub description: String,
    /// Datasheet URL, from the library symbol metadata (empty if not set
    /// or the KiCad placeholder `~`).
    pub datasheet: String,
    pub designators: Vec<String>,
}

/// Group per-component entries into BOM line items, enriched from `meta`.
///
/// Lines come out in first-seen order; since `extract_bom` returns entries
/// natural-sorted by reference, the grouped lines are reference-ordered too.
pub fn group_bom(entries: &[BomEntry], meta: &SymbolIndex) -> Vec<BomLine> {
    let mut index: HashMap<(String, String), usize> = HashMap::new();
    let mut lines: Vec<BomLine> = Vec::new();

    for e in entries {
        // Mounting holes, fiducials and the like are not BOM parts.
        if crate::export::is_bom_excluded(&e.footprint) {
            continue;
        }
        // Group on the footprint name (sans `library:` prefix); the same
        // (value, footprint-name) pair is the symbol-library join key.
        let fp_name = crate::export::footprint_name(&e.footprint).to_string();
        let key = (e.value.clone(), fp_name);
        if let Some(&i) = index.get(&key) {
            lines[i].designators.push(e.reference.clone());
        } else {
            let sym = meta.get(&key);
            // The library Description is the curated source; fall back to
            // whatever the board itself carries.
            let description = match sym {
                Some(m) if !m.description.is_empty() => m.description.clone(),
                _ => e.description.clone(),
            };
            let line_idx = lines.len();
            lines.push(BomLine {
                item: line_idx + 1,
                quantity: 0,
                value: e.value.clone(),
                package: crate::export::package_from_footprint(&e.footprint),
                manufacturer: sym.map(|m| m.manufacturer.clone()).unwrap_or_default(),
                mpn: sym.map(|m| m.mpn.clone()).unwrap_or_default(),
                description,
                datasheet: sym.map(|m| m.datasheet.clone()).unwrap_or_default(),
                designators: vec![e.reference.clone()],
            });
            index.insert(key, line_idx);
        }
    }
    for line in &mut lines {
        line.quantity = line.designators.len();
    }
    lines
}

const HEADERS: [&str; 9] = [
    "Item",
    "Quantity",
    "Value",
    "Package",
    "Manufacturer",
    "Manufacturer P/N",
    "Description",
    "Datasheet",
    "Designators",
];

/// Write the grouped, library-enriched BOM as a CSV file.
pub fn write_bom_csv(entries: &[BomEntry], path: &Path) -> Result<(), String> {
    let meta = load_global_symbol_index();
    let lines = group_bom(entries, &meta);

    let mut out = String::new();
    out.push_str(&HEADERS.join(","));
    out.push('\n');
    for l in &lines {
        out.push_str(&format!(
            "{},{},{},{},{},{},{},{},{}\n",
            l.item,
            l.quantity,
            csv_field(&l.value),
            csv_field(&l.package),
            csv_field(&l.manufacturer),
            csv_field(&l.mpn),
            csv_field(&l.description),
            csv_field(&l.datasheet),
            csv_field(&l.designators.join(" ")),
        ));
    }
    std::fs::write(path, out)
        .map_err(|err| format!("Failed to write BOM CSV {}: {}", path.display(), err))
}

/// Write the grouped, library-enriched BOM as an XLSX workbook.
pub fn write_bom_xlsx(entries: &[BomEntry], path: &Path) -> Result<(), String> {
    use rust_xlsxwriter::{Format, Workbook};

    let meta = load_global_symbol_index();
    let lines = group_bom(entries, &meta);

    let mut workbook = Workbook::new();
    let sheet = workbook.add_worksheet();
    let header = Format::new().set_bold();

    for (col, title) in HEADERS.iter().enumerate() {
        sheet
            .write_with_format(0, col as u16, *title, &header)
            .map_err(xlsx_err)?;
    }
    for (row, l) in lines.iter().enumerate() {
        let r = row as u32 + 1;
        sheet.write_number(r, 0, l.item as f64).map_err(xlsx_err)?;
        sheet.write_number(r, 1, l.quantity as f64).map_err(xlsx_err)?;
        sheet.write_string(r, 2, &l.value).map_err(xlsx_err)?;
        sheet.write_string(r, 3, &l.package).map_err(xlsx_err)?;
        sheet.write_string(r, 4, &l.manufacturer).map_err(xlsx_err)?;
        sheet.write_string(r, 5, &l.mpn).map_err(xlsx_err)?;
        sheet.write_string(r, 6, &l.description).map_err(xlsx_err)?;
        // Datasheet: emit as a clickable URL when it looks like one,
        // otherwise as a plain string. Empty values write nothing.
        if !l.datasheet.is_empty() {
            if l.datasheet.starts_with("http://") || l.datasheet.starts_with("https://") {
                sheet
                    .write_url(r, 7, l.datasheet.as_str())
                    .map_err(xlsx_err)?;
            } else {
                sheet.write_string(r, 7, &l.datasheet).map_err(xlsx_err)?;
            }
        }
        sheet
            .write_string(r, 8, &l.designators.join(" "))
            .map_err(xlsx_err)?;
    }
    workbook
        .save(path)
        .map_err(|e| format!("Failed to write BOM XLSX {}: {}", path.display(), e))
}

fn xlsx_err(e: rust_xlsxwriter::XlsxError) -> String {
    format!("XLSX write error: {}", e)
}
