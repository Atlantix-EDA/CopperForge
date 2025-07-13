use std::env;
use std::fs;
use std::path::Path;

fn main() {
    // Read Cargo.toml to extract dependency versions
    let manifest_dir = env::var("CARGO_MANIFEST_DIR").unwrap();
    let cargo_toml_path = Path::new(&manifest_dir).join("Cargo.toml");
    let cargo_toml = fs::read_to_string(cargo_toml_path).unwrap();
    
    // Parse versions from Cargo.toml
    let mut gerber_viewer_version = "unknown";
    let mut gerber_types_version = "unknown";
    let mut gerber_parser_version = "unknown";
    
    for line in cargo_toml.lines() {
        if line.starts_with("gerber_viewer = ") {
            gerber_viewer_version = line.split('"').nth(1).unwrap_or("unknown");
        } else if line.contains("gerber_types = ") && line.contains("version = ") {
            // Handle the complex format: gerber_types = { package = "gerber-types", version = "0.4.0" }
            if let Some(version_part) = line.split("version = ").nth(1) {
                gerber_types_version = version_part.split('"').nth(1).unwrap_or("unknown");
            }
        } else if line.starts_with("gerber_parser = ") {
            gerber_parser_version = line.split('"').nth(1).unwrap_or("unknown");
        }
    }
    
    // These will be available as env!() variables at compile time
    println!("cargo:rustc-env=GERBER_VIEWER_VERSION={}", gerber_viewer_version);
    println!("cargo:rustc-env=GERBER_TYPES_VERSION={}", gerber_types_version);
    println!("cargo:rustc-env=GERBER_PARSER_VERSION={}", gerber_parser_version);
}