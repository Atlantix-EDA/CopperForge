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

/// Read metadata from a .kicad_pro file and associated .kicad_sch files
pub fn read_kicad_metadata(kicad_pro_path: &Path) -> Result<KicadProjectMetadata, Box<dyn std::error::Error>> {
    // First try to read from .kicad_pro text_variables
    let content = std::fs::read_to_string(kicad_pro_path)?;
    let project: KicadProject = serde_json::from_str(&content)?;

    let mut metadata = project.text_variables.unwrap_or(KicadProjectMetadata {
        author: None,
        company: None,
        date: None,
        description: None,
    });

    // If we didn't find pedigree in .kicad_pro, try reading from .kicad_sch files
    if metadata.author.is_none() || metadata.company.is_none() {
        if let Some(sch_metadata) = read_schematic_metadata(kicad_pro_path)? {
            if metadata.author.is_none() {
                metadata.author = sch_metadata.author;
            }
            if metadata.company.is_none() {
                metadata.company = sch_metadata.company;
            }
            if metadata.date.is_none() {
                metadata.date = sch_metadata.date;
            }
            if metadata.description.is_none() {
                metadata.description = sch_metadata.description;
            }
        }
    }

    Ok(metadata)
}

/// Read metadata from .kicad_sch files (schematic title block)
fn read_schematic_metadata(kicad_pro_path: &Path) -> Result<Option<KicadProjectMetadata>, Box<dyn std::error::Error>> {
    use std::fs;
    use regex::Regex;

    let parent = kicad_pro_path.parent().ok_or("No parent directory")?;

    // Find all .kicad_sch files in the same directory
    let sch_files: Vec<_> = fs::read_dir(parent)?
        .filter_map(|e| e.ok())
        .filter(|e| {
            e.path()
                .extension()
                .and_then(|s| s.to_str())
                .map(|s| s == "kicad_sch")
                .unwrap_or(false)
        })
        .collect();

    // Regular expressions to match KiCad schematic title block fields
    let company_re = Regex::new(r#"\(company\s+"([^"]+)"\)"#)?;
    let author_re = Regex::new(r#"\(comment\s+1\s+"Author:\s*([^"]+)"\)"#)?;
    let date_re = Regex::new(r#"\(date\s+"([^"]+)"\)"#)?;

    let mut metadata = KicadProjectMetadata {
        author: None,
        company: None,
        date: None,
        description: None,
    };

    // Read the first schematic file we find
    for sch_file in sch_files {
        let content = fs::read_to_string(sch_file.path())?;

        if let Some(cap) = company_re.captures(&content) {
            metadata.company = Some(cap[1].to_string());
        }

        if let Some(cap) = author_re.captures(&content) {
            metadata.author = Some(cap[1].to_string());
        }

        if let Some(cap) = date_re.captures(&content) {
            metadata.date = Some(cap[1].to_string());
        }

        // If we found something, return it
        if metadata.company.is_some() || metadata.author.is_some() {
            return Ok(Some(metadata));
        }
    }

    Ok(None)
}

/// Get the .kicad_pro path from a .kicad_pcb path
pub fn get_kicad_pro_path(pcb_path: &Path) -> Option<std::path::PathBuf> {
    let parent = pcb_path.parent()?;
    let stem = pcb_path.file_stem()?;

    Some(parent.join(format!("{}.kicad_pro", stem.to_string_lossy())))
}
