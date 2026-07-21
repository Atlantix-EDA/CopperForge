//! Schematic-sourced BOM via `kicad-cli sch export bom`.
//!
//! Ports the kiverse `generate_bom.py` data path: KiCad schematics from these
//! libraries use inconsistent field names for the same data (Manufacturer /
//! Manufacturer_Name / MANUFACTURER / MF; Manufacturer_Part_Number / MPN /
//! Part Number). We request the *union* of those fields from `kicad-cli sch
//! export bom`, then coalesce them into clean columns. The schematic — not the
//! board — is the canonical source of part fields, DNP, "exclude from BOM",
//! hierarchical sheets and multi-unit symbols.

use std::path::Path;

/// One grouped BOM line, sourced and coalesced from the schematic export.
#[derive(Debug, Clone, Default)]
pub struct SchBomLine {
    pub item: usize,
    /// Expanded reference list for the group (e.g. "R1, R2, R3").
    pub reference: String,
    pub qty: String,
    pub description: String,
    pub manufacturer: String,
    pub mpn: String,
    /// LCSC / JLCPCB part number (e.g. "C42411119"), from the "LCSC Part #"
    /// symbol field. Empty when the part carries none.
    pub lcsc: String,
    pub vendor1: String,
    pub vendor1_pn: String,
    pub vendor2: String,
    pub vendor2_pn: String,
    /// "DNP" when the group is marked Do-Not-Populate, else empty.
    pub dnp: String,
}

// Source field labels (as requested from kicad-cli) that feed each clean
// column. kicad-cli may case-insensitively merge same-named fields, so we
// coalesce on whatever labels actually appear in the exported header.
const MFR_LABELS: [&str; 4] = ["Mfr", "Mfr_Name", "MANUF", "MF"];
const MPN_LABELS: [&str; 6] = ["MPN_a", "MPN_b", "PartNum", "PartNum2", "PartNum3", "PartNum4"];
const LCSC_LABELS: [&str; 3] = ["LCSC_a", "LCSC_b", "LCSC_c"];

/// Export and parse the schematic BOM. Stages the raw CSV in `stage_dir`
/// (kicad-cli under Flatpak is sandboxed away from /tmp, so we stage inside
/// the project tree, where it already has read access), then cleans it up.
pub fn export_sch_bom(
    kicad_cli_method: &str,
    sch_path: &Path,
    stage_dir: &Path,
    vendor1: &str,
    vendor2: &str,
) -> Result<Vec<SchBomLine>, String> {
    // Mirror the kiverse generate_bom.py field/label union exactly.
    let fields = "Reference,${QUANTITY},Value,Description,Footprint,\
        Manufacturer,Manufacturer_Name,MANUFACTURER,MF,\
        Manufacturer_Part_Number,MPN,Part Number,PART NUMBER,PARTNUMBER,Manufacturer P/N,\
        Supplier,SupplierPN,Digi-Key Part Number,Mouser Part Number,\
        LCSC Part #,LCSC PN,LCSC,${DNP}";
    let labels = "Reference,Qty,Value,Description,Footprint,\
        Mfr,Mfr_Name,MANUF,MF,\
        MPN_a,MPN_b,PartNum,PartNum2,PartNum3,PartNum4,\
        Supplier,SupplierPN,DigiKeyPNField,MouserPNField,\
        LCSC_a,LCSC_b,LCSC_c,DNP";

    let raw_csv = stage_dir.join(".bom_raw.csv");
    let mut cmd = crate::app::CopperForgeApp::build_kicad_cli_command(kicad_cli_method);
    let output = cmd
        .arg("sch")
        .arg("export")
        .arg("bom")
        .arg("--fields")
        .arg(fields)
        .arg("--labels")
        .arg(labels)
        .arg("--group-by")
        .arg("Value,Footprint")
        .arg("--sort-field")
        .arg("Reference")
        .arg("--ref-range-delimiter") // expand ranges -> match existing BOM style
        .arg("")
        .arg("-o")
        .arg(&raw_csv)
        .arg(sch_path)
        .output()
        .map_err(|e| format!("kicad-cli sch export bom failed to spawn: {}", e))?;

    if !output.status.success() {
        let _ = std::fs::remove_file(&raw_csv);
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("kicad-cli sch export bom failed: {}", stderr.trim()));
    }

    let content = std::fs::read_to_string(&raw_csv)
        .map_err(|e| format!("Failed to read exported BOM CSV: {}", e));
    let _ = std::fs::remove_file(&raw_csv);
    let lines = parse_bom_csv(&content?, vendor1, vendor2);
    Ok(lines)
}

/// Parse the raw kicad-cli CSV into coalesced BOM lines.
fn parse_bom_csv(content: &str, vendor1: &str, vendor2: &str) -> Vec<SchBomLine> {
    let rows = parse_csv(content);
    let mut iter = rows.into_iter();
    let Some(header) = iter.next() else {
        return Vec::new();
    };

    let mut out: Vec<SchBomLine> = Vec::new();
    for record in iter {
        let get = |label: &str| -> String {
            header
                .iter()
                .position(|h| h == label)
                .and_then(|i| record.get(i))
                .map(|s| s.trim().to_string())
                .unwrap_or_default()
        };
        let reference = get("Reference");
        if reference.is_empty() {
            continue;
        }
        let pn1 = vendor_pn(&header, &record, vendor1);
        let pn2 = vendor_pn(&header, &record, vendor2);
        out.push(SchBomLine {
            item: 0, // assigned below
            reference,
            qty: get("Qty"),
            description: get("Description"),
            manufacturer: first_nonempty(&header, &record, &MFR_LABELS),
            mpn: first_nonempty(&header, &record, &MPN_LABELS),
            lcsc: first_nonempty(&header, &record, &LCSC_LABELS),
            vendor1: if pn1.is_empty() { String::new() } else { vendor1.to_string() },
            vendor1_pn: pn1,
            vendor2: if pn2.is_empty() { String::new() } else { vendor2.to_string() },
            vendor2_pn: pn2,
            dnp: if get("DNP").is_empty() { String::new() } else { "DNP".to_string() },
        });
    }
    for (i, line) in out.iter_mut().enumerate() {
        line.item = i + 1;
    }
    out
}

/// First non-empty value among `labels` for this record.
fn first_nonempty(header: &[String], record: &[String], labels: &[&str]) -> String {
    for lbl in labels {
        if let Some(i) = header.iter().position(|h| h == lbl) {
            if let Some(v) = record.get(i) {
                let v = v.trim();
                if !v.is_empty() {
                    return v.to_string();
                }
            }
        }
    }
    String::new()
}

/// Part number for `vendor`, from an explicit per-vendor field or the generic
/// Supplier/SupplierPN pair when the Supplier value names this vendor.
fn vendor_pn(header: &[String], record: &[String], vendor: &str) -> String {
    let explicit = match vendor.to_lowercase().as_str() {
        "digi-key" => &["DigiKeyPNField"][..],
        "mouser" => &["MouserPNField"][..],
        _ => &[][..],
    };
    let v = first_nonempty(header, record, explicit);
    if !v.is_empty() {
        return v;
    }
    let supplier = cell(header, record, "Supplier");
    let vnorm = norm(vendor);
    let prefix: String = vnorm.chars().take(5).collect();
    if !prefix.is_empty() && norm(&supplier).starts_with(&prefix) {
        return cell(header, record, "SupplierPN");
    }
    String::new()
}

fn cell(header: &[String], record: &[String], label: &str) -> String {
    header
        .iter()
        .position(|h| h == label)
        .and_then(|i| record.get(i))
        .map(|s| s.trim().to_string())
        .unwrap_or_default()
}

/// Lowercase, strip dashes and spaces — matches the Python `_norm`.
fn norm(s: &str) -> String {
    s.trim()
        .to_lowercase()
        .chars()
        .filter(|c| *c != '-' && *c != ' ')
        .collect()
}

/// Minimal RFC-4180 CSV parser: handles quoted fields, escaped quotes (`""`),
/// and newlines inside quotes. Returns rows of fields.
fn parse_csv(content: &str) -> Vec<Vec<String>> {
    let mut rows: Vec<Vec<String>> = Vec::new();
    let mut row: Vec<String> = Vec::new();
    let mut field = String::new();
    let mut in_quotes = false;
    let mut chars = content.chars().peekable();

    while let Some(c) = chars.next() {
        if in_quotes {
            match c {
                '"' => {
                    if chars.peek() == Some(&'"') {
                        field.push('"');
                        chars.next();
                    } else {
                        in_quotes = false;
                    }
                }
                _ => field.push(c),
            }
        } else {
            match c {
                '"' => in_quotes = true,
                ',' => {
                    row.push(std::mem::take(&mut field));
                }
                '\r' => {}
                '\n' => {
                    row.push(std::mem::take(&mut field));
                    rows.push(std::mem::take(&mut row));
                }
                _ => field.push(c),
            }
        }
    }
    // Trailing field/row with no final newline.
    if !field.is_empty() || !row.is_empty() {
        row.push(field);
        rows.push(row);
    }
    rows
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn coalesces_messy_fields_and_vendor_pns() {
        // MF supplies the manufacturer (Mfr/Mfr_Name empty); MPN_b supplies the
        // part number; the generic Supplier/SupplierPN pair names Mouser.
        let csv = "Reference,Qty,Value,Description,Mfr,Mfr_Name,MANUF,MF,MPN_a,MPN_b,PartNum,PartNum2,PartNum3,PartNum4,Supplier,SupplierPN,DigiKeyPNField,MouserPNField,DNP\n\
            \"R1, R2\",2,10k,Resistor,,,,Yageo,,RC0603,,,,,Mouser,603-RC0603,,,\n";
        let lines = parse_bom_csv(csv, "Digi-Key", "Mouser");
        assert_eq!(lines.len(), 1);
        let l = &lines[0];
        assert_eq!(l.item, 1);
        assert_eq!(l.reference, "R1, R2");
        assert_eq!(l.qty, "2");
        assert_eq!(l.manufacturer, "Yageo");
        assert_eq!(l.mpn, "RC0603");
        assert_eq!(l.vendor1, ""); // no Digi-Key PN
        assert_eq!(l.vendor1_pn, "");
        assert_eq!(l.vendor2, "Mouser");
        assert_eq!(l.vendor2_pn, "603-RC0603");
    }

    #[test]
    fn carries_lcsc_part_number() {
        // The "LCSC Part #" field (label LCSC_a) flows into the lcsc column.
        let csv = "Reference,Qty,Value,Description,Mfr,Mfr_Name,MANUF,MF,MPN_a,MPN_b,PartNum,PartNum2,PartNum3,PartNum4,Supplier,SupplierPN,DigiKeyPNField,MouserPNField,LCSC_a,LCSC_b,LCSC_c,DNP\n\
            C1,1,100n,Cap,KEMET,,,,C100,,,,,,,,,,C42411119,,,\n";
        let lines = parse_bom_csv(csv, "Digi-Key", "Mouser");
        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0].lcsc, "C42411119");
    }

    #[test]
    fn dnp_flag_and_skips_blank_reference() {
        let csv = "Reference,Qty,Value,Description,Mfr,Mfr_Name,MANUF,MF,MPN_a,MPN_b,PartNum,PartNum2,PartNum3,PartNum4,Supplier,SupplierPN,DigiKeyPNField,MouserPNField,DNP\n\
            C1,1,100n,Cap,KEMET,,,,C100,,,,,,,,,,DNP\n\
            ,,,,,,,,,,,,,,,,,,\n";
        let lines = parse_bom_csv(csv, "Digi-Key", "Mouser");
        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0].dnp, "DNP");
    }
}
