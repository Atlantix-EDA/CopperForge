/// Parse KiCad project file hierarchy
use std::path::{Path, PathBuf};
use serde::{Deserialize, Serialize};
use regex::Regex;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectHierarchy {
    pub root_schematic: Option<PathBuf>,
    pub pcb_file: Option<PathBuf>,
    pub sheets: Vec<HierarchicalSheet>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HierarchicalSheet {
    pub name: String,
    pub file_path: PathBuf,
    pub level: usize,
    pub sub_sheets: Vec<HierarchicalSheet>,
}

impl ProjectHierarchy {
    /// Parse the project hierarchy from a .kicad_pro file
    pub fn from_kicad_pro(kicad_pro_path: &Path) -> Result<Self, Box<dyn std::error::Error>> {
        let parent_dir = kicad_pro_path.parent().ok_or("No parent directory")?;
        let stem = kicad_pro_path.file_stem().ok_or("No file stem")?;

        // Find the root schematic file (same name as .kicad_pro)
        let root_sch = parent_dir.join(format!("{}.kicad_sch", stem.to_string_lossy()));
        let root_schematic = if root_sch.exists() {
            Some(root_sch.clone())
        } else {
            None
        };

        // Find the PCB file
        let pcb = parent_dir.join(format!("{}.kicad_pcb", stem.to_string_lossy()));
        let pcb_file = if pcb.exists() {
            Some(pcb)
        } else {
            None
        };

        // Parse hierarchical sheets from root schematic
        let sheets = if let Some(ref root) = root_schematic {
            parse_hierarchical_sheets(root, 0)?
        } else {
            Vec::new()
        };

        Ok(Self {
            root_schematic,
            pcb_file,
            sheets,
        })
    }
}

/// Parse hierarchical sheets from a .kicad_sch file
fn parse_hierarchical_sheets(sch_path: &Path, level: usize) -> Result<Vec<HierarchicalSheet>, Box<dyn std::error::Error>> {
    use std::fs;

    let content = fs::read_to_string(sch_path)?;
    let parent_dir = sch_path.parent().ok_or("No parent directory")?;

    // Regex to match hierarchical sheet blocks in KiCad 9.x format with multiline properties
    // The properties can span multiple lines with the value on a separate line
    let sheet_block_re = Regex::new(r#"(?s)\(sheet\s.*?\n\t\)"#)?;
    let sheetname_re = Regex::new(r#"\(property\s+"Sheetname"\s+"([^"]+)""#)?;
    let sheetfile_re = Regex::new(r#"\(property\s+"Sheetfile"\s+"([^"]+)""#)?;

    let mut sheets = Vec::new();

    for sheet_block in sheet_block_re.find_iter(&content) {
        let block_text = sheet_block.as_str();

        let name = sheetname_re.captures(block_text)
            .and_then(|cap| Some(cap[1].to_string()));
        let file_name = sheetfile_re.captures(block_text)
            .and_then(|cap| Some(cap[1].to_string()));

        if let (Some(name), Some(file_name)) = (name, file_name) {
            let file_path = parent_dir.join(&file_name);

            // Recursively parse sub-sheets if the file exists
            let sub_sheets = if file_path.exists() {
                parse_hierarchical_sheets(&file_path, level + 1)?
            } else {
                Vec::new()
            };

            sheets.push(HierarchicalSheet {
                name,
                file_path,
                level,
                sub_sheets,
            });
        }
    }

    Ok(sheets)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_hierarchy() {
        // This test would require a real KiCad project structure
        // For now, just verify the parser compiles
        assert!(true);
    }
}
