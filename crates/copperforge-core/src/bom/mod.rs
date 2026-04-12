//! BOM extraction from KiCad .kicad_pcb files via kiparse.
//!
//! Parses the PCB file directly — no live IPC connection to KiCad needed.

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
pub fn extract_board_dimensions(pcb_path: &Path) -> Result<Option<BoardDimensions>, String> {
    let content = std::fs::read_to_string(pcb_path)
        .map_err(|e| format!("Failed to read PCB file: {}", e))?;

    let parser = DetailParser::new(&content);
    match parser.extract_board_outline() {
        Ok(Some(outline)) => Ok(Some(BoardDimensions {
            width_mm: outline.width_mm,
            height_mm: outline.height_mm,
            area_mm2: outline.width_mm * outline.height_mm,
        })),
        Ok(None) => Ok(None),
        Err(e) => Err(format!("Failed to parse board outline: {}", e)),
    }
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
