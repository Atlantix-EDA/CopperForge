//! Gerber generation + load operations driven from the Gerber Viewer ribbon.
//!
//! These are the two long-lived workflows that used to live on the PCB File tab.
//! They're explicit user actions — no auto-generate, no auto-reload.

use std::path::{Path, PathBuf};

use crate::event_logger::ReactiveEventLogger;
use crate::CopperForgeApp;

/// Invoke `kicad-cli pcb export gerbers` on `pcb_path`, writing output to a
/// `gerber_output/` directory next to the PCB file. Returns the output
/// directory on success. The caller supplies a pre-built `kicad-cli` Command
/// (see `CopperForgeApp::kicad_cli_command`) so we don't re-probe the sandbox.
pub fn generate_gerbers_from_pcb(
    pcb_path: &Path,
    mut cmd: std::process::Command,
    logger: &ReactiveEventLogger,
) -> Option<PathBuf> {
    let output_dir = pcb_path.parent()
        .unwrap_or(Path::new("."))
        .join("gerber_output");

    if let Err(e) = std::fs::create_dir_all(&output_dir) {
        logger.log_error(&format!("Failed to create output directory: {}", e));
        return None;
    }

    logger.log_info(&format!("Output directory: {}", output_dir.display()));

    let output = cmd
        .arg("pcb")
        .arg("export")
        .arg("gerbers")
        .arg("--output")
        .arg(&output_dir)
        .arg("--layers")
        .arg("F.Cu,B.Cu,F.SilkS,B.SilkS,F.Mask,B.Mask,Edge.Cuts,F.Paste,B.Paste")
        .arg("--no-protel-ext")
        .arg(pcb_path)
        .output();

    match output {
        Ok(result) if result.status.success() => {
            logger.log_info("Gerbers generated successfully!");
            if let Ok(entries) = std::fs::read_dir(&output_dir) {
                for entry in entries.flatten() {
                    if entry.path().extension().is_some_and(|e| e == "gbr") {
                        logger.log_info(&format!("  Generated: {}", entry.file_name().to_string_lossy()));
                    }
                }
            }
            Some(output_dir)
        }
        Ok(result) => {
            logger.log_error("Failed to generate gerbers");
            if let Ok(stderr) = String::from_utf8(result.stderr) {
                logger.log_error(&format!("Error: {}", stderr));
            }
            None
        }
        Err(e) => {
            logger.log_error(&format!("Failed to run kicad-cli: {}", e));
            None
        }
    }
}

/// Load gerber files from `gerber_dir` into the app's layer store.
pub fn load_gerbers_into_viewer(app: &mut CopperForgeApp, gerber_dir: &Path, logger: &ReactiveEventLogger) {
    logger.log_info("Clearing existing gerber layers...");
    app.layer_store.clear_all();

    match app.layer_store.load_from_directory(gerber_dir) {
        Ok((loaded_count, unassigned_count)) => {
            if loaded_count > 0 {
                logger.log_info(&format!("Successfully loaded {} gerber layers", loaded_count));
                app.needs_initial_view = true;
            } else if unassigned_count > 0 {
                logger.log_warning(&format!("{} gerber files could not be automatically assigned", unassigned_count));
            } else {
                logger.log_error("No gerber files were found");
            }
        }
        Err(e) => {
            logger.log_error(&format!("Failed to load gerbers: {}", e));
        }
    }
}
