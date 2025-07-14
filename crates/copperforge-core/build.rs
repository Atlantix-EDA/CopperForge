use std::env;
use std::fs;
use std::path::Path;

fn main() {
    // Read both local and workspace Cargo.toml to extract dependency versions
    let manifest_dir = env::var("CARGO_MANIFEST_DIR").unwrap();
    let cargo_toml_path = Path::new(&manifest_dir).join("Cargo.toml");
    let cargo_toml = fs::read_to_string(cargo_toml_path).unwrap();
    
    // Also read workspace Cargo.toml
    let workspace_cargo_toml_path = Path::new(&manifest_dir).join("../../Cargo.toml");
    let workspace_cargo_toml = fs::read_to_string(workspace_cargo_toml_path).unwrap_or_default();
    
    // Parse versions from both local and workspace Cargo.toml
    let mut gerber_viewer_version = "unknown";
    let mut gerber_types_version = "unknown";
    let mut gerber_parser_version = "unknown";
    
    // Check local Cargo.toml first
    for line in cargo_toml.lines() {
        if line.starts_with("gerber_viewer = ") {
            gerber_viewer_version = line.split('"').nth(1).unwrap_or("unknown");
        } else if line.contains("gerber_types = ") && line.contains("version = ") {
            if let Some(version_part) = line.split("version = ").nth(1) {
                gerber_types_version = version_part.split('"').nth(1).unwrap_or("unknown");
            }
        } else if line.starts_with("gerber_parser = ") {
            gerber_parser_version = line.split('"').nth(1).unwrap_or("unknown");
        }
    }
    
    // Check workspace Cargo.toml if not found locally
    for line in workspace_cargo_toml.lines() {
        if gerber_viewer_version == "unknown" && line.starts_with("gerber_viewer = ") {
            gerber_viewer_version = line.split('"').nth(1).unwrap_or("unknown");
        } else if gerber_types_version == "unknown" && line.contains("gerber_types = ") && line.contains("version = ") {
            if let Some(version_part) = line.split("version = ").nth(1) {
                gerber_types_version = version_part.split('"').nth(1).unwrap_or("unknown");
            }
        } else if gerber_parser_version == "unknown" && line.starts_with("gerber_parser = ") {
            gerber_parser_version = line.split('"').nth(1).unwrap_or("unknown");
        }
    }
    
    // These will be available as env!() variables at compile time
    println!("cargo:rustc-env=GERBER_VIEWER_VERSION={}", gerber_viewer_version);
    println!("cargo:rustc-env=GERBER_TYPES_VERSION={}", gerber_types_version);
    println!("cargo:rustc-env=GERBER_PARSER_VERSION={}", gerber_parser_version);
}