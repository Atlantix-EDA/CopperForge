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
    // Include all 45 KiCad user-layer slots (User.1..User.45). kicad-cli
    // silently skips slots that aren't defined in the board — boards that
    // use M1/M2/M10/M11/M12 etc. get those exported; others aren't affected.
    let user_layers: String = (1..=45)
        .map(|n| format!("User.{}", n))
        .collect::<Vec<_>>()
        .join(",");
    let layers_arg = format!(
        "F.Cu,B.Cu,F.SilkS,B.SilkS,F.Mask,B.Mask,Edge.Cuts,F.Paste,B.Paste,{}",
        user_layers
    );

    let mut cmd = CopperForgeApp::build_kicad_cli_command(kicad_cli_method);
    let output = cmd
        .arg("pcb")
        .arg("export")
        .arg("gerbers")
        .arg("--output")
        .arg(&output_dir)
        .arg("--layers")
        .arg(&layers_arg)
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

/// Load gerber files from `gerber_dir` into the app's layer store. The
/// `pcb_path` is read to pull the KiCad 10 canonical names for User.N
/// slots out of the `.kicad_pcb` (e.g. "M1 Board Outline", "Top 3D Body")
/// so the View Settings panel shows KiCad's own labels.
pub fn load_gerbers_into_viewer(
    app: &mut CopperForgeApp,
    pcb_path: &Path,
    gerber_dir: &Path,
    logger: &ReactiveEventLogger,
) {
    logger.log_info("Clearing existing gerber layers...");
    app.services.layer_store.clear_all();

    match crate::project_manager::kicad_metadata::read_user_layer_names(pcb_path) {
        Ok(names) => {
            if !names.is_empty() {
                logger.log_info(&format!("Parsed {} formal user-layer name(s) from .kicad_pcb", names.len()));
            }
            app.services.layer_store.user_layer_names = names;
        }
        Err(e) => {
            logger.log_warning(&format!("Could not parse user-layer names from PCB: {}", e));
        }
    }

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

    // 3D pipeline geometry: extract the board outline from the mechanical-
    // outline gerber. Reads the file a second time via `gerber_parser` — the
    // legacy `gerber_viewer` path has already read it once for the 2D canvas.
    // This duplication is documented in the FDD's "Legacy 2D Rendering Path"
    // section and goes away when Phase 7 retires gerber_viewer.
    app.services.board_outline = extract_outline_from_layer_store(&app.services.layer_store, logger);

    // Copper layers (Phase 4a). Require an outline bbox so the copper mesh
    // lines up with the board mesh — both share the same world transform
    // (Stage 6 of the FDD pipeline, centered at outline bbox).
    if let Some(outline) = app.services.board_outline.as_ref() {
        let outline_bbox = outline.bbox.clone();
        app.services.top_copper = extract_copper_side(
            &app.services.layer_store,
            crate::layer_store::LayerType::Copper(1),
            "F.Cu",
            &outline_bbox,
            logger,
        );
        app.services.bottom_copper = extract_copper_side(
            &app.services.layer_store,
            crate::layer_store::LayerType::Copper(2),
            "B.Cu",
            &outline_bbox,
            logger,
        );
    } else {
        app.services.top_copper = None;
        app.services.bottom_copper = None;
    }
}

/// Look up a copper layer in the store and extract its polygon IR, aligned
/// to the board outline's bbox. `label` is used in log output so the reader
/// can tell F.Cu and B.Cu lines apart.
fn extract_copper_side(
    store: &crate::layer_store::LayerStore,
    layer_type: crate::layer_store::LayerType,
    label: &str,
    outline_bbox: &gerber_viewer::BoundingBox,
    logger: &ReactiveEventLogger,
) -> Option<crate::gerber_geom::CopperData> {
    let layer = store.find(layer_type)?;
    let path = layer.file_path.as_ref()?;
    match crate::gerber_geom::extract_copper(path, outline_bbox) {
        Some((data, counts)) => {
            logger.log_info(&format!(
                "{} copper: {} circle + {} rect + {} obround + {} polygon flash(es); {} linear stroke(s); {} region polygon(s); {} macros / {} arc-strokes / {} non-circle-strokes skipped",
                label,
                counts.flashed_circles,
                counts.flashed_rectangles,
                counts.flashed_obrounds,
                counts.flashed_polygons,
                counts.linear_strokes,
                counts.region_polygons,
                counts.flashed_macros_skipped,
                counts.arc_strokes_skipped,
                counts.non_circle_strokes_skipped,
            ));
            logger.log_info(&format!(
                "{} copper: {} triangle(s) tessellated",
                label,
                data.mesh_indices.len() / 3,
            ));
            Some(data)
        }
        None => {
            logger.log_warning(&format!("{} copper: no geometry extracted", label));
            None
        }
    }
}

/// Look up the mechanical-outline layer in the store, grab its source file
/// path, and run `gerber_geom::extract_outline` on it. Returns `None` if the
/// layer isn't present, the file path wasn't recorded at load time, or the
/// extractor couldn't recover any closed contours.
fn extract_outline_from_layer_store(
    store: &crate::layer_store::LayerStore,
    logger: &ReactiveEventLogger,
) -> Option<crate::gerber_geom::OutlineData> {
    let layer = store.find(crate::layer_store::LayerType::MechanicalOutline)?;
    let path = layer.file_path.as_ref()?;
    match crate::gerber_geom::extract_outline(path) {
        Some((data, counts)) => {
            logger.log_info(&format!(
                "Board outline: {} linear + {} arc stroke(s), {} region polygon(s) -> {} stitched contour(s), {} triangle(s)",
                counts.linear_strokes,
                counts.arc_strokes,
                counts.region_polygons,
                counts.stitched_contours,
                data.mesh_indices.len() / 3,
            ));
            logger.log_info(&format!(
                "Board outline bbox (gerber coords, mm): [{:.3}, {:.3}] -> [{:.3}, {:.3}]",
                data.bbox.min.x, data.bbox.min.y, data.bbox.max.x, data.bbox.max.y,
            ));
            Some(data)
        }
        None => {
            logger.log_warning("Board outline: gerber_geom produced no closed contours");
            None
        }
    }
}
