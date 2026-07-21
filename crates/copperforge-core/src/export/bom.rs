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
use std::path::{Path, PathBuf};

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
    /// Distributor part numbers as `(vendor, pn)`, from the library symbol
    /// metadata. Digi-Key/Mouser first; open-ended for Newark, Arrow, etc.
    pub vendors: Vec<(String, String)>,
    pub designators: Vec<String>,
}

impl BomLine {
    /// This line's part number for `vendor`, or "" if it carries none.
    fn vendor_pn(&self, vendor: &str) -> &str {
        self.vendors
            .iter()
            .find(|(v, _)| v == vendor)
            .map(|(_, pn)| pn.as_str())
            .unwrap_or("")
    }
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
                vendors: sym.map(|m| m.vendors.clone()).unwrap_or_default(),
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

/// Columns before the (variable) vendor columns.
const BASE_HEADERS: [&str; 8] = [
    "Item",
    "Quantity",
    "Value",
    "Package",
    "Manufacturer",
    "Manufacturer P/N",
    "Description",
    "Datasheet",
];

/// The ordered set of vendors to emit columns for: Digi-Key and Mouser always
/// (the standard pair, even when empty), plus any other distributor that any
/// line actually carries (Newark, Arrow, …), in first-seen order.
fn vendor_columns(lines: &[BomLine]) -> Vec<String> {
    let mut vendors: Vec<String> = vec!["Digi-Key".to_string(), "Mouser".to_string()];
    for l in lines {
        for (v, _) in &l.vendors {
            if !vendors.iter().any(|x| x == v) {
                vendors.push(v.clone());
            }
        }
    }
    vendors
}

/// Full header row: base columns, then `Vendor N` / `Vendor N P/N` pairs, then
/// Designators. Vendor names live in the cells (matching the prototype output);
/// the headers stay positional so adding a distributor just adds columns.
fn headers(vendors: &[String]) -> Vec<String> {
    let mut h: Vec<String> = BASE_HEADERS.iter().map(|s| s.to_string()).collect();
    for i in 0..vendors.len() {
        h.push(format!("Vendor {}", i + 1));
        h.push(format!("Vendor {} P/N", i + 1));
    }
    h.push("Designators".to_string());
    h
}

/// Write the grouped, library-enriched BOM as a CSV file.
pub fn write_bom_csv(entries: &[BomEntry], path: &Path) -> Result<(), String> {
    let meta = load_global_symbol_index();
    let lines = group_bom(entries, &meta);
    let vendors = vendor_columns(&lines);

    let mut out = String::new();
    out.push_str(&headers(&vendors).join(","));
    out.push('\n');
    for l in &lines {
        let mut fields = vec![
            l.item.to_string(),
            l.quantity.to_string(),
            csv_field(&l.value),
            csv_field(&l.package),
            csv_field(&l.manufacturer),
            csv_field(&l.mpn),
            csv_field(&l.description),
            csv_field(&l.datasheet),
        ];
        for v in &vendors {
            let pn = l.vendor_pn(v);
            // Vendor name shows only when this line carries a P/N for it.
            fields.push(csv_field(if pn.is_empty() { "" } else { v }));
            fields.push(csv_field(pn));
        }
        fields.push(csv_field(&l.designators.join(" ")));
        out.push_str(&fields.join(","));
        out.push('\n');
    }
    std::fs::write(path, out)
        .map_err(|err| format!("Failed to write BOM CSV {}: {}", path.display(), err))
}

/// Cover-page metadata for a release BOM workbook. `stats` comes from
/// [`crate::bom::cover_stats`]; the board/rev/date/copper fields come from the
/// project + release context.
pub struct CoverInfo {
    pub board_pn: String,
    pub rev: String,
    pub date: String,
    pub copper: String,
    pub logo_path: Option<PathBuf>,
    pub stats: crate::bom::CoverStats,
}

/// Write the grouped, library-enriched BOM as a single-sheet XLSX workbook.
/// PCB-sourced; used by the interactive BOM panel.
pub fn write_bom_xlsx(entries: &[BomEntry], path: &Path) -> Result<(), String> {
    use rust_xlsxwriter::Workbook;

    let meta = load_global_symbol_index();
    let lines = group_bom(entries, &meta);

    let mut workbook = Workbook::new();
    let sheet = workbook.add_worksheet();
    sheet.set_name("BOM").map_err(xlsx_err)?;
    write_bom_sheet(sheet, &lines)?;

    workbook
        .save(path)
        .map_err(|e| format!("Failed to write BOM XLSX {}: {}", path.display(), e))
}

fn write_bom_sheet(
    sheet: &mut rust_xlsxwriter::Worksheet,
    lines: &[BomLine],
) -> Result<(), String> {
    use rust_xlsxwriter::Format;
    let header = Format::new().set_bold();
    let vendors = vendor_columns(lines);
    let cols = headers(&vendors);

    for (col, title) in cols.iter().enumerate() {
        sheet
            .write_with_format(0, col as u16, title.as_str(), &header)
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
        // Datasheet: clickable URL when it looks like one, else plain string.
        if !l.datasheet.is_empty() {
            if l.datasheet.starts_with("http://") || l.datasheet.starts_with("https://") {
                sheet.write_url(r, 7, l.datasheet.as_str()).map_err(xlsx_err)?;
            } else {
                sheet.write_string(r, 7, &l.datasheet).map_err(xlsx_err)?;
            }
        }
        // Vendor pairs: name only when a P/N is present, then the P/N.
        let mut col: u16 = 8;
        for v in &vendors {
            let pn = l.vendor_pn(v);
            sheet
                .write_string(r, col, if pn.is_empty() { "" } else { v.as_str() })
                .map_err(xlsx_err)?;
            sheet.write_string(r, col + 1, pn).map_err(xlsx_err)?;
            col += 2;
        }
        sheet
            .write_string(r, col, &l.designators.join(" "))
            .map_err(xlsx_err)?;
    }
    Ok(())
}

// ── Release BOM (schematic-sourced, kiverse generate_bom.py style) ──────────

use crate::bom::schematic::SchBomLine;

/// Atlantix-EDA cover logo, bundled with the app (no runtime path needed).
const LOGO_PNG: &[u8] = include_bytes!("../../assets/atlantix-logo.png");

/// Canonical release-BOM columns, matching the kiverse Python output.
const RELEASE_COLUMNS: [&str; 12] = [
    "Item", "Reference", "Qty", "Description",
    "Manufacturer", "Manufacturer P/N", "LCSC #",
    "Vendor 1", "Vendor 1 P/N", "Vendor 2", "Vendor 2 P/N", "DNP",
];

/// Each release column's value for `line`, in `RELEASE_COLUMNS` order.
fn release_cells(line: &SchBomLine) -> [String; 12] {
    [
        line.item.to_string(),
        line.reference.clone(),
        line.qty.clone(),
        line.description.clone(),
        line.manufacturer.clone(),
        line.mpn.clone(),
        line.lcsc.clone(),
        line.vendor1.clone(),
        line.vendor1_pn.clone(),
        line.vendor2.clone(),
        line.vendor2_pn.clone(),
        line.dnp.clone(),
    ]
}

/// Write the schematic-sourced release BOM as a CSV file.
pub fn write_release_bom_csv(lines: &[SchBomLine], path: &Path) -> Result<(), String> {
    let mut out = String::new();
    out.push_str(&RELEASE_COLUMNS.join(","));
    out.push('\n');
    for l in lines {
        let cells = release_cells(l);
        let row: Vec<String> = cells.iter().map(|c| csv_field(c)).collect();
        out.push_str(&row.join(","));
        out.push('\n');
    }
    std::fs::write(path, out)
        .map_err(|e| format!("Failed to write BOM CSV {}: {}", path.display(), e))
}

/// Write the two-page (Cover + BOM) release workbook, styled to match the
/// kiverse `generate_bom.py` output.
pub fn write_release_bom_xlsx(
    lines: &[SchBomLine],
    path: &Path,
    cover: &CoverInfo,
) -> Result<(), String> {
    use rust_xlsxwriter::Workbook;

    let mut workbook = Workbook::new();

    let cov = workbook.add_worksheet();
    cov.set_name("Cover").map_err(xlsx_err)?;
    write_release_cover_sheet(cov, cover)?;

    let bom = workbook.add_worksheet();
    bom.set_name("BOM").map_err(xlsx_err)?;
    write_release_bom_sheet(bom, lines)?;

    workbook
        .save(path)
        .map_err(|e| format!("Failed to write BOM XLSX {}: {}", path.display(), e))
}

fn write_release_cover_sheet(
    sheet: &mut rust_xlsxwriter::Worksheet,
    c: &CoverInfo,
) -> Result<(), String> {
    use rust_xlsxwriter::{Color, Format, FormatAlign, Image};

    const BLUE: u32 = 0x3A6EA5;
    const GRAY: u32 = 0xD9D9D9;
    const GREEN: u32 = 0x76B947;
    const LINK: u32 = 0x0563C1;

    let blue = Format::new().set_background_color(Color::RGB(BLUE));
    let gray = Format::new().set_background_color(Color::RGB(GRAY));
    // Blue-band text (white on blue).
    let title = Format::new().set_bold().set_font_size(16)
        .set_font_color(Color::White).set_background_color(Color::RGB(BLUE));
    let wbold = Format::new().set_bold()
        .set_font_color(Color::White).set_background_color(Color::RGB(BLUE));
    let wname = Format::new().set_bold().set_italic().set_font_size(12)
        .set_font_color(Color::White).set_background_color(Color::RGB(BLUE));
    let green_it = Format::new().set_bold().set_italic()
        .set_font_color(Color::RGB(GREEN)).set_background_color(Color::RGB(BLUE));
    // Gray-band text (black on gray).
    let lbl = Format::new().set_bold().set_background_color(Color::RGB(GRAY));
    let body = Format::new().set_background_color(Color::RGB(GRAY));
    let link = Format::new().set_font_color(Color::RGB(LINK)).set_underline(
        rust_xlsxwriter::FormatUnderline::Single,
    ).set_background_color(Color::RGB(GRAY));
    let _ = FormatAlign::Center; // (alignment defaults are fine for the cover)

    let s = &c.stats;

    // Banding: blue header (rows 0-6), gray body (rows 7-21), cols A-I (0-8).
    for r in 0..=6u32 {
        for col in 0..=8u16 {
            sheet.write_blank(r, col, &blue).map_err(xlsx_err)?;
        }
    }
    for r in 7..=21u32 {
        for col in 0..=8u16 {
            sheet.write_blank(r, col, &gray).map_err(xlsx_err)?;
        }
    }

    sheet.write_with_format(1, 2, "BILL OF MATERIALS", &title).map_err(xlsx_err)?;

    // Board ID block — white text on the blue header band.
    sheet.write_with_format(4, 2, "Board Part Number", &wbold).map_err(xlsx_err)?;
    sheet.write_with_format(4, 3, c.board_pn.as_str(), &wbold).map_err(xlsx_err)?;
    sheet.write_with_format(5, 2, "Revision", &wbold).map_err(xlsx_err)?;
    sheet.write_with_format(5, 3, c.rev.as_str(), &wbold).map_err(xlsx_err)?;
    sheet.write_with_format(6, 2, "Date", &wbold).map_err(xlsx_err)?;
    sheet.write_with_format(6, 3, c.date.as_str(), &wbold).map_err(xlsx_err)?;

    // Component counts (gray band).
    sheet.write_with_format(8, 2, "Number of Components", &lbl).map_err(xlsx_err)?;
    sheet.write_number_with_format(8, 3, s.total_components() as f64, &body).map_err(xlsx_err)?;
    sheet.write_with_format(9, 4, "TOP", &lbl).map_err(xlsx_err)?;
    sheet.write_with_format(9, 5, "BOTTOM", &lbl).map_err(xlsx_err)?;
    sheet.write_with_format(10, 3, "Through Hole", &body).map_err(xlsx_err)?;
    sheet.write_number_with_format(10, 4, s.top_th as f64, &body).map_err(xlsx_err)?;
    sheet.write_number_with_format(10, 5, s.bottom_th as f64, &body).map_err(xlsx_err)?;
    sheet.write_with_format(11, 3, "SMT", &body).map_err(xlsx_err)?;
    sheet.write_number_with_format(11, 4, s.top_smt as f64, &body).map_err(xlsx_err)?;
    sheet.write_number_with_format(11, 5, s.bottom_smt as f64, &body).map_err(xlsx_err)?;

    sheet.write_with_format(13, 2, "Copper Weight", &lbl).map_err(xlsx_err)?;
    sheet.write_with_format(13, 3, c.copper.as_str(), &body).map_err(xlsx_err)?;

    // Board size — both inches and mm (matches the kiverse cover).
    sheet.write_with_format(14, 2, "Board Size", &lbl).map_err(xlsx_err)?;
    sheet.write_with_format(14, 4, "X Dimension", &lbl).map_err(xlsx_err)?;
    sheet.write_with_format(14, 5, "Y Dimension", &lbl).map_err(xlsx_err)?;
    sheet.write_with_format(15, 3, "inches", &body).map_err(xlsx_err)?;
    sheet.write_with_format(16, 3, "mm", &body).map_err(xlsx_err)?;
    if let Some(d) = &s.dimensions {
        let r3 = |v: f64| (v * 1000.0).round() / 1000.0;
        sheet.write_number_with_format(15, 4, r3(d.width_mm / 25.4), &body).map_err(xlsx_err)?;
        sheet.write_number_with_format(15, 5, r3(d.height_mm / 25.4), &body).map_err(xlsx_err)?;
        sheet.write_number_with_format(16, 4, r3(d.width_mm), &body).map_err(xlsx_err)?;
        sheet.write_number_with_format(16, 5, r3(d.height_mm), &body).map_err(xlsx_err)?;
    }

    // Pad / hole counts (fab reference).
    sheet.write_with_format(18, 2, "Pad / Hole Count", &lbl).map_err(xlsx_err)?;
    sheet.write_with_format(19, 3, "SMT Pads", &body).map_err(xlsx_err)?;
    sheet.write_number_with_format(19, 4, s.smt_pads as f64, &body).map_err(xlsx_err)?;
    sheet.write_with_format(20, 3, "Through-Holes", &body).map_err(xlsx_err)?;
    sheet.write_number_with_format(20, 4, s.through_holes() as f64, &body).map_err(xlsx_err)?;
    sheet.write_with_format(
        20, 5,
        format!("({} TH pads + {} vias)", s.th_pads, s.vias).as_str(),
        &body,
    ).map_err(xlsx_err)?;
    if s.np_pads > 0 {
        sheet.write_with_format(21, 3, "Non-Plated Holes", &body).map_err(xlsx_err)?;
        sheet.write_number_with_format(21, 4, s.np_pads as f64, &body).map_err(xlsx_err)?;
    }

    // Branding block (right side).
    sheet.write_with_format(4, 8, "Atlantix-EDA", &wname).map_err(xlsx_err)?;
    sheet.write_with_format(5, 8, "Modern Electrical Engineering", &green_it).map_err(xlsx_err)?;
    sheet.write_with_format(6, 8, "Automating the future of Electronics", &green_it).map_err(xlsx_err)?;
    sheet.write_url_with_format(19, 8, "https://docs.copperforge.dev", &link).map_err(xlsx_err)?;

    // Bundled logo (best-effort — a decode failure must not fail the release).
    if let Ok(img) = Image::new_from_buffer(LOGO_PNG) {
        let img = img.set_scale_to_size(230u32, 230u32, true);
        let _ = sheet.insert_image(8, 8, &img);
    }

    sheet.set_column_width(2, 22).map_err(xlsx_err)?;
    sheet.set_column_width(3, 16).map_err(xlsx_err)?;
    sheet.set_column_width(4, 13).map_err(xlsx_err)?;
    sheet.set_column_width(5, 13).map_err(xlsx_err)?;
    sheet.set_column_width(8, 36).map_err(xlsx_err)?;
    Ok(())
}

fn write_release_bom_sheet(
    sheet: &mut rust_xlsxwriter::Worksheet,
    lines: &[SchBomLine],
) -> Result<(), String> {
    use rust_xlsxwriter::{Color, Format, FormatAlign, FormatBorder};

    let border = FormatBorder::Thin;
    let header = Format::new()
        .set_bold()
        .set_background_color(Color::RGB(0xD9E1F2))
        .set_border(border)
        .set_align(FormatAlign::Center)
        .set_align(FormatAlign::VerticalCenter);
    let center = Format::new()
        .set_border(border)
        .set_align(FormatAlign::Center)
        .set_align(FormatAlign::VerticalCenter);
    let left = Format::new()
        .set_border(border)
        .set_align(FormatAlign::VerticalCenter);
    let wrap = Format::new()
        .set_border(border)
        .set_align(FormatAlign::VerticalCenter)
        .set_text_wrap();

    for (col, title) in RELEASE_COLUMNS.iter().enumerate() {
        sheet.write_with_format(0, col as u16, *title, &header).map_err(xlsx_err)?;
    }
    for (row, line) in lines.iter().enumerate() {
        let r = row as u32 + 1;
        let cells = release_cells(line);
        for (ci, col_name) in RELEASE_COLUMNS.iter().enumerate() {
            let fmt = match *col_name {
                "Item" | "Qty" | "DNP" => &center,
                "Reference" | "Description" => &wrap,
                _ => &left,
            };
            // Item is numeric; everything else is text.
            if *col_name == "Item" {
                sheet
                    .write_number_with_format(r, ci as u16, line.item as f64, fmt)
                    .map_err(xlsx_err)?;
            } else {
                sheet
                    .write_with_format(r, ci as u16, cells[ci].as_str(), fmt)
                    .map_err(xlsx_err)?;
            }
        }
    }

    let widths = [6.0, 34.0, 6.0, 46.0, 18.0, 24.0, 14.0, 11.0, 18.0, 11.0, 18.0, 6.0];
    for (i, w) in widths.iter().enumerate() {
        sheet.set_column_width(i as u16, *w).map_err(xlsx_err)?;
    }
    sheet.set_freeze_panes(1, 0).map_err(xlsx_err)?;
    Ok(())
}

fn xlsx_err(e: rust_xlsxwriter::XlsxError) -> String {
    format!("XLSX write error: {}", e)
}
