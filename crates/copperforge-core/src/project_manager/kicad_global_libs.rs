/// Module for managing KiCad global library configuration
use std::path::PathBuf;
use std::fs;

#[derive(Debug)]
pub enum GlobalLibError {
    IoError(std::io::Error),
    ConfigNotFound(String),
}

impl std::fmt::Display for GlobalLibError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            GlobalLibError::IoError(e) => write!(f, "I/O error: {}", e),
            GlobalLibError::ConfigNotFound(p) => write!(f, "KiCad config not found at: {}", p),
        }
    }
}

impl From<std::io::Error> for GlobalLibError {
    fn from(err: std::io::Error) -> Self {
        GlobalLibError::IoError(err)
    }
}

/// Get the path to KiCad's global configuration directory
fn get_kicad_config_dir() -> Option<PathBuf> {
    let home = std::env::var("HOME").ok()?;

    // Try KiCad 9.99 (development version) first, then stable versions
    let versions = vec!["9.99", "9.0", "8.0", "7.0"];

    for version in versions {
        let path = PathBuf::from(&home).join(".config/kicad").join(version);
        if path.exists() {
            return Some(path);
        }
    }

    None
}

/// Check if a library is already present in the library table
fn is_lib_in_table(table_content: &str, lib_name: &str) -> bool {
    // Look for lib entries with the given name
    table_content.contains(&format!(r#"(lib (name "{}")"#, lib_name))
}

/// Add KiVerse libraries to KiCad's global symbol library table
pub fn add_kiverse_to_global_sym_libs(kiverse_path: Option<PathBuf>) -> Result<(), GlobalLibError> {
    let config_dir = get_kicad_config_dir()
        .ok_or_else(|| GlobalLibError::ConfigNotFound("KiCad config directory not found".to_string()))?;

    let sym_lib_table_path = config_dir.join("sym-lib-table");

    // Determine KiVerse base path
    let kiverse_base = kiverse_path.as_ref()
        .map(|p| p.display().to_string())
        .unwrap_or_else(|| "${HOME}/.kicad_libs/kiverse".to_string());

    // Find all KiVerse symbol files
    let kiverse_symbols_dir = if let Some(path_str) = &kiverse_path {
        path_str.join("kicad/symbols")
    } else {
        let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
        PathBuf::from(format!("{}/.kicad_libs/kiverse/kicad/symbols", home))
    };

    if !kiverse_symbols_dir.exists() {
        return Err(GlobalLibError::ConfigNotFound(
            format!("KiVerse symbols directory not found: {}", kiverse_symbols_dir.display())
        ));
    }

    // Read existing library table
    let existing_content = if sym_lib_table_path.exists() {
        fs::read_to_string(&sym_lib_table_path)?
    } else {
        // Create minimal library table if it doesn't exist
        "(sym_lib_table\n  (version 7)\n)\n".to_string()
    };

    // Collect new library entries
    let mut new_entries = Vec::new();

    if let Ok(entries) = std::fs::read_dir(&kiverse_symbols_dir) {
        for entry in entries.flatten() {
            if let Some(filename) = entry.file_name().to_str() {
                if filename.ends_with(".kicad_sym") {
                    let lib_name = filename.trim_end_matches(".kicad_sym");

                    // Skip if already exists
                    if is_lib_in_table(&existing_content, lib_name) {
                        continue;
                    }

                    new_entries.push(format!(
                        r#"  (lib (name "{}")(type "KiCad")(uri "{}/kicad/symbols/{}")(options "")(descr "KiVerse Library"))"#,
                        lib_name, kiverse_base, filename
                    ));
                }
            }
        }
    }

    // Also add Atlantix resistor libraries if they exist
    let atlantix_dir = if let Some(path) = &kiverse_path {
        path.join("kicad/symbols/atlantix-eda")
    } else {
        let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
        PathBuf::from(format!("{}/.kicad_libs/kiverse/kicad/symbols/atlantix-eda", home))
    };

    if atlantix_dir.exists() {
        let packages = vec!["0402", "0603", "0805", "1206", "1210", "2512"];
        for pkg in packages {
            let lib_name = format!("Atlantix_R_{}", pkg);

            // Skip if already exists
            if is_lib_in_table(&existing_content, &lib_name) {
                continue;
            }

            new_entries.push(format!(
                r#"  (lib (name "Atlantix_R_{}")(type "KiCad")(uri "{}/kicad/symbols/atlantix-eda/Atlantix_R_{}.kicad_sym")(options "")(descr "Atlantix Resistor Library {}"))"#,
                pkg, kiverse_base, pkg, pkg
            ));
        }
    }

    // If no new entries, we're done
    if new_entries.is_empty() {
        return Ok(());
    }

    // Insert new entries before the closing parenthesis
    let mut output = String::new();
    let lines: Vec<&str> = existing_content.lines().collect();

    for (i, line) in lines.iter().enumerate() {
        output.push_str(line);
        output.push('\n');

        // Insert new entries before the last line (closing parenthesis)
        if i == lines.len() - 1 && line.trim() == ")" {
            // Remove the closing parenthesis we just added
            output.truncate(output.len() - 2);

            // Add new entries
            for entry in &new_entries {
                output.push_str(entry);
                output.push('\n');
            }

            // Add closing parenthesis back
            output.push_str(")\n");
        }
    }

    // Write back to file
    fs::write(&sym_lib_table_path, output)?;

    Ok(())
}

/// Add KiVerse libraries to KiCad's global footprint library table
pub fn add_kiverse_to_global_fp_libs(kiverse_path: Option<PathBuf>) -> Result<(), GlobalLibError> {
    let config_dir = get_kicad_config_dir()
        .ok_or_else(|| GlobalLibError::ConfigNotFound("KiCad config directory not found".to_string()))?;

    let fp_lib_table_path = config_dir.join("fp-lib-table");

    // Determine KiVerse base path
    let kiverse_base = kiverse_path.as_ref()
        .map(|p| p.display().to_string())
        .unwrap_or_else(|| "${HOME}/.kicad_libs/kiverse".to_string());

    // Find all KiVerse footprint directories
    let kiverse_footprints_dir = if let Some(path_str) = &kiverse_path {
        path_str.join("kicad/footprints")
    } else {
        let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
        PathBuf::from(format!("{}/.kicad_libs/kiverse/kicad/footprints", home))
    };

    if !kiverse_footprints_dir.exists() {
        return Err(GlobalLibError::ConfigNotFound(
            format!("KiVerse footprints directory not found: {}", kiverse_footprints_dir.display())
        ));
    }

    // Read existing library table
    let existing_content = if fp_lib_table_path.exists() {
        fs::read_to_string(&fp_lib_table_path)?
    } else {
        // Create minimal library table if it doesn't exist
        "(fp_lib_table\n  (version 7)\n)\n".to_string()
    };

    // Collect new library entries
    let mut new_entries = Vec::new();

    if let Ok(entries) = std::fs::read_dir(&kiverse_footprints_dir) {
        for entry in entries.flatten() {
            if let Some(dirname) = entry.file_name().to_str() {
                if dirname.ends_with(".pretty") && entry.path().is_dir() {
                    let lib_name = dirname.trim_end_matches(".pretty");

                    // Skip if already exists
                    if is_lib_in_table(&existing_content, lib_name) {
                        continue;
                    }

                    new_entries.push(format!(
                        r#"  (lib (name "{}")(type "KiCad")(uri "{}/kicad/footprints/{}")(options "")(descr "KiVerse Footprint Library"))"#,
                        lib_name, kiverse_base, dirname
                    ));
                }
            }
        }
    }

    // Also add Atlantix resistor footprints if they exist
    let atlantix_dir = if let Some(path) = &kiverse_path {
        path.join("kicad/footprints/atlantix-eda/Atlantix_Resistors.pretty")
    } else {
        let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
        PathBuf::from(format!("{}/.kicad_libs/kiverse/kicad/footprints/atlantix-eda/Atlantix_Resistors.pretty", home))
    };

    if atlantix_dir.exists() {
        let lib_name = "Atlantix_Resistors";

        // Skip if already exists
        if !is_lib_in_table(&existing_content, lib_name) {
            new_entries.push(format!(
                r#"  (lib (name "Atlantix_Resistors")(type "KiCad")(uri "{}/kicad/footprints/atlantix-eda/Atlantix_Resistors.pretty")(options "")(descr "Atlantix Resistor Footprints"))"#,
                kiverse_base
            ));
        }
    }

    // If no new entries, we're done
    if new_entries.is_empty() {
        return Ok(());
    }

    // Insert new entries before the closing parenthesis
    let mut output = String::new();
    let lines: Vec<&str> = existing_content.lines().collect();

    for (i, line) in lines.iter().enumerate() {
        output.push_str(line);
        output.push('\n');

        // Insert new entries before the last line (closing parenthesis)
        if i == lines.len() - 1 && line.trim() == ")" {
            // Remove the closing parenthesis we just added
            output.truncate(output.len() - 2);

            // Add new entries
            for entry in &new_entries {
                output.push_str(entry);
                output.push('\n');
            }

            // Add closing parenthesis back
            output.push_str(")\n");
        }
    }

    // Write back to file
    fs::write(&fp_lib_table_path, output)?;

    Ok(())
}

/// Setup KiVerse in KiCad's global configuration (both symbols and footprints)
pub fn setup_kiverse_globally(kiverse_path: Option<PathBuf>) -> Result<(), GlobalLibError> {
    add_kiverse_to_global_sym_libs(kiverse_path.clone())?;
    add_kiverse_to_global_fp_libs(kiverse_path)?;
    Ok(())
}
