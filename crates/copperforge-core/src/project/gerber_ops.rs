//! Gerber generation + load operations driven from the Gerber Viewer ribbon.
//!
//! These are the two long-lived workflows that used to live on the PCB File tab.
//! They're explicit user actions — no auto-generate, no auto-reload.

use std::path::{Path, PathBuf};

use crate::event_logger::ReactiveEventLogger;
use crate::CopperForgeApp;

/// Invoke `kicad-cli pcb export gerbers` (main copper/silk/mask/paste/edge-cuts),
/// then a second pass `kicad-cli pcb export drill --format gerber` so the drill
/// holes show up as a regular gerber layer in the 2D viewer. Output lands in a
/// single `gerber_output/` directory next to the PCB file.
///
/// Returns the output directory on success. The caller supplies the discovered
/// kicad-cli `method` string ("path" / "flatpak" / "snap") so we can build two
/// Commands without re-probing the sandbox.
pub fn generate_gerbers_from_pcb(
    pcb_path: &Path,
    kicad_cli_method: &str,
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

    // ── Pass 1: main gerbers ───────────────────────────────────────
    let mut cmd = CopperForgeApp::build_kicad_cli_command(kicad_cli_method);
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
            logger.log_info("Gerbers generated.");
            if let Ok(entries) = std::fs::read_dir(&output_dir) {
                for entry in entries.flatten() {
                    if entry.path().extension().is_some_and(|e| e == "gbr") {
                        logger.log_info(&format!("  Generated: {}", entry.file_name().to_string_lossy()));
                    }
                }
            }
        }
        Ok(result) => {
            logger.log_error("Failed to generate gerbers");
            if let Ok(stderr) = String::from_utf8(result.stderr) {
                logger.log_error(&format!("Error: {}", stderr.trim()));
            }
            return None;
        }
        Err(e) => {
            logger.log_error(&format!("Failed to run kicad-cli: {}", e));
            return None;
        }
    }

    // ── Pass 2: drill → gerber ─────────────────────────────────────
    let mut drill_cmd = CopperForgeApp::build_kicad_cli_command(kicad_cli_method);
    let drill_out = drill_cmd
        .arg("pcb")
        .arg("export")
        .arg("drill")
        .arg("--format")
        .arg("gerber")
        .arg("--output")
        .arg(&output_dir)
        .arg(pcb_path)
        .output();

    match drill_out {
        Ok(r) if r.status.success() => {
            logger.log_info("Drill holes exported as gerber.");
        }
        Ok(r) => {
            // Non-fatal — the main gerbers still shipped.
            let stderr = String::from_utf8_lossy(&r.stderr);
            logger.log_warning(&format!("Drill-gerber export failed (viewer won't show holes): {}", stderr.trim()));
        }
        Err(e) => {
            logger.log_warning(&format!("Drill-gerber export failed to spawn: {}", e));
        }
    }

    Some(output_dir)
}

/// Load gerber files from `gerber_dir` into the app's layer store.
pub fn load_gerbers_into_viewer(app: &mut CopperForgeApp, gerber_dir: &Path, logger: &ReactiveEventLogger) {
    logger.log_info("Clearing existing gerber layers...");
    app.services.layer_store.clear_all();

    match app.services.layer_store.load_from_directory(gerber_dir) {
        Ok((loaded_count, unassigned_count)) => {
            if loaded_count > 0 {
                logger.log_info(&format!("Successfully loaded {} gerber layers", loaded_count));
                app.services.needs_initial_view = true;
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
