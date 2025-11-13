/// Parse metadata from KiCad project files (.kicad_pro)
use std::path::Path;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KicadProjectMetadata {
    #[serde(rename = "AUTHOR")]
    pub author: Option<String>,

    #[serde(rename = "COMPANY")]
    pub company: Option<String>,

    #[serde(rename = "DATE")]
    pub date: Option<String>,

    #[serde(rename = "DESCRIPTION")]
    pub description: Option<String>,
}

#[derive(Debug, Deserialize)]
struct KicadProject {
    text_variables: Option<KicadProjectMetadata>,
}

/// Read metadata from a .kicad_pro file
pub fn read_kicad_metadata(kicad_pro_path: &Path) -> Result<KicadProjectMetadata, Box<dyn std::error::Error>> {
    let content = std::fs::read_to_string(kicad_pro_path)?;
    let project: KicadProject = serde_json::from_str(&content)?;

    Ok(project.text_variables.unwrap_or(KicadProjectMetadata {
        author: None,
        company: None,
        date: None,
        description: None,
    }))
}

/// Get the .kicad_pro path from a .kicad_pcb path
pub fn get_kicad_pro_path(pcb_path: &Path) -> Option<std::path::PathBuf> {
    let parent = pcb_path.parent()?;
    let stem = pcb_path.file_stem()?;

    Some(parent.join(format!("{}.kicad_pro", stem.to_string_lossy())))
}
