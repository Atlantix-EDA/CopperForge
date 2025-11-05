/// Module for creating new KiCad projects from scratch
use std::path::PathBuf;
use std::fs;
use chrono::Utc;

#[derive(Debug)]
pub enum KicadProjectError {
    IoError(std::io::Error),
    ProjectExists(String),
    InvalidPath(String),
}

impl std::fmt::Display for KicadProjectError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            KicadProjectError::IoError(e) => write!(f, "I/O error: {}", e),
            KicadProjectError::ProjectExists(p) => write!(f, "Project already exists at: {}", p),
            KicadProjectError::InvalidPath(p) => write!(f, "Invalid project path: {}", p),
        }
    }
}

impl From<std::io::Error> for KicadProjectError {
    fn from(err: std::io::Error) -> Self {
        KicadProjectError::IoError(err)
    }
}

/// Metadata for creating a new KiCad project
pub struct NewKicadProjectInfo {
    pub name: String,
    pub location: PathBuf,
    pub author: String,
    pub description: String,
    pub company: String,
    pub include_kiverse: bool,
    pub include_atlantix_resistors: bool,
    pub kiverse_path: Option<PathBuf>,
}

impl NewKicadProjectInfo {
    pub fn new(name: String, location: PathBuf) -> Self {
        Self {
            name,
            location,
            author: String::new(),
            description: String::new(),
            company: String::new(),
            include_kiverse: true,
            include_atlantix_resistors: true,
            kiverse_path: None,
        }
    }

    /// Get the full project directory path
    pub fn project_dir(&self) -> PathBuf {
        self.location.join(&self.name)
    }

    /// Get the path to the .kicad_pro file
    pub fn project_file_path(&self) -> PathBuf {
        self.project_dir().join(format!("{}.kicad_pro", self.name))
    }

    /// Get the path to the .kicad_sch file
    pub fn schematic_file_path(&self) -> PathBuf {
        self.project_dir().join(format!("{}.kicad_sch", self.name))
    }

    /// Get the path to the .kicad_pcb file
    pub fn pcb_file_path(&self) -> PathBuf {
        self.project_dir().join(format!("{}.kicad_pcb", self.name))
    }
}

/// Create a new KiCad project from scratch
pub fn create_kicad_project(info: &NewKicadProjectInfo) -> Result<PathBuf, KicadProjectError> {
    let project_dir = info.project_dir();

    // Check if project already exists
    if project_dir.exists() {
        return Err(KicadProjectError::ProjectExists(project_dir.display().to_string()));
    }

    // Create project directory
    fs::create_dir_all(&project_dir)?;

    // Create .kicad_pro file
    create_kicad_pro_file(info)?;

    // Create .kicad_sch file
    create_kicad_sch_file(info)?;

    // Create .kicad_pcb file
    create_kicad_pcb_file(info)?;

    // Create sym-lib-table
    if info.include_kiverse || info.include_atlantix_resistors {
        create_sym_lib_table(info)?;
    }

    // Create fp-lib-table
    if info.include_kiverse || info.include_atlantix_resistors {
        create_fp_lib_table(info)?;
    }

    // Create README
    create_readme(info)?;

    Ok(project_dir)
}

/// Create the .kicad_pro project file
fn create_kicad_pro_file(info: &NewKicadProjectInfo) -> Result<(), KicadProjectError> {
    let path = info.project_file_path();
    let now = Utc::now();

    let content = format!(r#"{{
  "board": {{
    "design_settings": {{
      "defaults": {{
        "board_outline_line_width": 0.1,
        "copper_line_width": 0.2,
        "copper_text_size_h": 1.5,
        "copper_text_size_v": 1.5,
        "copper_text_thickness": 0.3,
        "other_line_width": 0.15,
        "silk_line_width": 0.15,
        "silk_text_size_h": 1.0,
        "silk_text_size_v": 1.0,
        "silk_text_thickness": 0.15
      }},
      "diff_pair_dimensions": [],
      "drc_exclusions": [],
      "rules": {{
        "min_copper_edge_clearance": 0.0,
        "solder_mask_clearance": 0.0,
        "solder_mask_min_width": 0.0
      }},
      "track_widths": [],
      "via_dimensions": []
    }}
  }},
  "meta": {{
    "version": 1
  }},
  "net_settings": {{
    "classes": [
      {{
        "clearance": 0.2,
        "diff_pair_gap": 0.25,
        "diff_pair_width": 0.2,
        "microvia_diameter": 0.3,
        "microvia_drill": 0.1,
        "name": "Default",
        "track_width": 0.25,
        "via_diameter": 0.8,
        "via_drill": 0.4
      }}
    ],
    "meta": {{
      "version": 2
    }}
  }},
  "pcbnew": {{
    "last_paths": {{
      "gencad": "",
      "idf": "",
      "netlist": "",
      "specctra_dsn": "",
      "step": "",
      "vrml": ""
    }},
    "page_layout_descr_file": ""
  }},
  "schematic": {{
    "legacy_lib_dir": "",
    "legacy_lib_list": []
  }},
  "text_variables": {{
    "AUTHOR": "{}",
    "COMPANY": "{}",
    "DATE": "{}",
    "DESCRIPTION": "{}"
  }}
}}
"#,
        info.author.replace('"', "\\\""),
        info.company.replace('"', "\\\""),
        now.format("%Y-%m-%d"),
        info.description.replace('"', "\\\"")
    );

    fs::write(path, content)?;
    Ok(())
}

/// Create the .kicad_sch schematic file
fn create_kicad_sch_file(info: &NewKicadProjectInfo) -> Result<(), KicadProjectError> {
    let path = info.schematic_file_path();

    // Minimal KiCad 9.0 schematic format
    let content = r#"(kicad_sch (version 20231120) (generator eeschema)

  (uuid "00000000-0000-0000-0000-000000000000")

  (paper "A4")

  (lib_symbols
  )

  (sheet_instances
    (path "/" (page "1"))
  )
)
"#;

    fs::write(path, content)?;
    Ok(())
}

/// Create the .kicad_pcb board file
fn create_kicad_pcb_file(info: &NewKicadProjectInfo) -> Result<(), KicadProjectError> {
    let path = info.pcb_file_path();

    // Minimal KiCad 9.0 PCB format
    let content = r#"(kicad_pcb (version 20231120) (generator pcbnew)

  (general
    (thickness 1.6)
  )

  (paper "A4")
  (layers
    (0 "F.Cu" signal)
    (31 "B.Cu" signal)
    (32 "B.Adhes" user "B.Adhesive")
    (33 "F.Adhes" user "F.Adhesive")
    (34 "B.Paste" user)
    (35 "F.Paste" user)
    (36 "B.SilkS" user "B.Silkscreen")
    (37 "F.SilkS" user "F.Silkscreen")
    (38 "B.Mask" user)
    (39 "F.Mask" user)
    (40 "Dwgs.User" user "User.Drawings")
    (41 "Cmts.User" user "User.Comments")
    (42 "Eco1.User" user "User.Eco1")
    (43 "Eco2.User" user "User.Eco2")
    (44 "Edge.Cuts" user)
    (45 "Margin" user)
    (46 "B.CrtYd" user "B.Courtyard")
    (47 "F.CrtYd" user "F.Courtyard")
    (48 "B.Fab" user)
    (49 "F.Fab" user)
  )

  (setup
    (pad_to_mask_clearance 0)
    (pcbplotparams
      (layerselection 0x00010fc_ffffffff)
      (plot_on_all_layers_selection 0x0000000_00000000)
      (disableapertmacros false)
      (usegerberextensions false)
      (usegerberattributes true)
      (usegerberadvancedattributes true)
      (creategerberjobfile true)
      (dashed_line_dash_ratio 12.000000)
      (dashed_line_gap_ratio 3.000000)
      (svgprecision 4)
      (plotframeref false)
      (viasonmask false)
      (mode 1)
      (useauxorigin false)
      (hpglpennumber 1)
      (hpglpenspeed 20)
      (hpglpendiameter 15.000000)
      (dxfpolygonmode true)
      (dxfimperialunits true)
      (dxfusepcbnewfont true)
      (psnegative false)
      (psa4output false)
      (plotreference true)
      (plotvalue true)
      (plotinvisibletext false)
      (sketchpadsonfab false)
      (subtractmaskfromsilk false)
      (outputformat 1)
      (mirror false)
      (drillshape 1)
      (scaleselection 1)
      (outputdirectory "")
    )
  )

  (net 0 "")
)
"#;

    fs::write(path, content)?;
    Ok(())
}

/// Create sym-lib-table for symbol libraries
fn create_sym_lib_table(info: &NewKicadProjectInfo) -> Result<(), KicadProjectError> {
    let path = info.project_dir().join("sym-lib-table");

    let mut entries = Vec::new();

    // Add KiVerse if requested
    if info.include_kiverse {
        let kiverse_base = info.kiverse_path.as_ref()
            .map(|p| p.display().to_string())
            .unwrap_or_else(|| "${HOME}/.kicad_libs/kiverse".to_string());

        entries.push(format!(
            r#"  (lib (name "KiVerse")(type "KiCad")(uri "{}/symbols/KiVerse.kicad_sym")(options "")(descr "KiVerse Symbol Library"))"#,
            kiverse_base
        ));
    }

    // Add Atlantix resistors if requested
    if info.include_atlantix_resistors {
        let kiverse_base = info.kiverse_path.as_ref()
            .map(|p| p.display().to_string())
            .unwrap_or_else(|| "${HOME}/.kicad_libs/kiverse".to_string());

        // Assuming atlantix resistors will be in KiVerse repo
        let packages = vec!["0402", "0603", "0805", "1206", "1210", "2512"];
        for pkg in packages {
            entries.push(format!(
                r#"  (lib (name "Atlantix_R_{}")(type "KiCad")(uri "{}/symbols/atlantix-eda/Atlantix_R_{}.kicad_sym")(options "")(descr "Atlantix Resistor Library {}"))"#,
                pkg, kiverse_base, pkg, pkg
            ));
        }
    }

    let content = format!(
        "(sym_lib_table\n{}\n)\n",
        entries.join("\n")
    );

    fs::write(path, content)?;
    Ok(())
}

/// Create fp-lib-table for footprint libraries
fn create_fp_lib_table(info: &NewKicadProjectInfo) -> Result<(), KicadProjectError> {
    let path = info.project_dir().join("fp-lib-table");

    let mut entries = Vec::new();

    // Add KiVerse if requested
    if info.include_kiverse {
        let kiverse_base = info.kiverse_path.as_ref()
            .map(|p| p.display().to_string())
            .unwrap_or_else(|| "${HOME}/.kicad_libs/kiverse".to_string());

        entries.push(format!(
            r#"  (lib (name "KiVerse")(type "KiCad")(uri "{}/footprints/KiVerse.pretty")(options "")(descr "KiVerse Footprint Library"))"#,
            kiverse_base
        ));
    }

    // Add Atlantix resistors if requested
    if info.include_atlantix_resistors {
        let kiverse_base = info.kiverse_path.as_ref()
            .map(|p| p.display().to_string())
            .unwrap_or_else(|| "${HOME}/.kicad_libs/kiverse".to_string());

        entries.push(format!(
            r#"  (lib (name "Atlantix_Resistors")(type "KiCad")(uri "{}/footprints/atlantix-eda/Atlantix_Resistors.pretty")(options "")(descr "Atlantix Resistor Footprints"))"#,
            kiverse_base
        ));
    }

    let content = format!(
        "(fp_lib_table\n{}\n)\n",
        entries.join("\n")
    );

    fs::write(path, content)?;
    Ok(())
}

/// Create a README.md for the project
fn create_readme(info: &NewKicadProjectInfo) -> Result<(), KicadProjectError> {
    let path = info.project_dir().join("README.md");
    let now = Utc::now();

    let content = format!(
r#"# {}

**Author:** {}
**Company:** {}
**Created:** {}

## Description

{}

## Project Structure

- `{}.kicad_pro` - KiCad project file
- `{}.kicad_sch` - Schematic file
- `{}.kicad_pcb` - PCB layout file
- `sym-lib-table` - Symbol library table
- `fp-lib-table` - Footprint library table

## Libraries

{}{}

## Notes

Created with CopperForge v{} - PCB & CAM for KiCad

---
*Generated on {}*
"#,
        info.name,
        info.author,
        info.company,
        now.format("%Y-%m-%d"),
        info.description,
        info.name,
        info.name,
        info.name,
        if info.include_kiverse { "- KiVerse symbol and footprint library\n" } else { "" },
        if info.include_atlantix_resistors { "- Atlantix-EDA resistor library (E96 series, 0402-2512)\n" } else { "" },
        env!("CARGO_PKG_VERSION"),
        now.format("%Y-%m-%d %H:%M:%S UTC")
    );

    fs::write(path, content)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;

    #[test]
    fn test_new_project_paths() {
        let info = NewKicadProjectInfo::new(
            "test_project".to_string(),
            PathBuf::from("/tmp"),
        );

        assert_eq!(info.project_dir(), PathBuf::from("/tmp/test_project"));
        assert_eq!(info.project_file_path(), PathBuf::from("/tmp/test_project/test_project.kicad_pro"));
        assert_eq!(info.schematic_file_path(), PathBuf::from("/tmp/test_project/test_project.kicad_sch"));
        assert_eq!(info.pcb_file_path(), PathBuf::from("/tmp/test_project/test_project.kicad_pcb"));
    }
}
