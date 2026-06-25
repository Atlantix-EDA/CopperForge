//! Gerber generation + load operations driven from the Gerber Viewer ribbon.
//!
//! These are the two long-lived workflows that used to live on the PCB File tab.
//! They're explicit user actions — no auto-generate, no auto-reload.

use std::path::{Path, PathBuf};

use crate::event_logger::ReactiveEventLogger;
use crate::services::SharedServices;
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

    // Clean stale gerber/job files from prior generations. kicad-cli only
    // writes the layers in the current stackup — it does NOT delete files
    // for layers that have since been removed. Without this, dropping a
    // 4-layer board to 2 leaves In1_Cu.gbr / In2_Cu.gbr behind and the
    // viewer faithfully shows 4 layers. Drill .gbr files are also wiped
    // (regenerated in Pass 2 below). Non-gerber sidecars left alone.
    let stale = std::fs::read_dir(&output_dir)
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.path())
        .filter(|p| {
            p.is_file()
                && p.extension()
                    .and_then(|e| e.to_str())
                    .is_some_and(|ext| matches!(ext, "gbr" | "gbrjob"))
        })
        .collect::<Vec<_>>();
    if !stale.is_empty() {
        logger.log_info(&format!(
            "Removing {} stale gerber file(s) from prior generation",
            stale.len()
        ));
        for path in &stale {
            if let Err(e) = std::fs::remove_file(path) {
                logger.log_warning(&format!(
                    "Could not remove {}: {}",
                    path.display(),
                    e
                ));
            }
        }
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
    let copper_layers = copper_layers_from_pcb(pcb_path);
    logger.log_info(&format!("Copper stack from .kicad_pcb: {}", copper_layers));
    let layers_arg = format!(
        "{},F.SilkS,B.SilkS,F.Mask,B.Mask,Edge.Cuts,F.Paste,B.Paste,{}",
        copper_layers, user_layers
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

/// Read the copper-layer stack from the `.kicad_pcb` in physical stack order:
/// F.Cu first, then In1.Cu, In2.Cu, ..., B.Cu last. KiCad assigns even layer
/// IDs to copper (F.Cu=0, B.Cu=2, In1.Cu=4, In2.Cu=6, ...), so sorting by ID
/// with B.Cu pinned last yields the correct order. Empty vec on parse failure
/// — callers fall back to plain F.Cu/B.Cu.
pub fn copper_layers_from_pcb_vec(pcb_path: &Path) -> Vec<String> {
    let Ok(content) = std::fs::read_to_string(pcb_path) else { return vec![] };
    let Ok(pcb) = kiparse::pcb::parse_layers_only(&content) else { return vec![] };

    let mut copper: Vec<(i32, String)> = pcb.layers.iter()
        .filter(|(_, layer)| layer.name.ends_with(".Cu"))
        .map(|(id, layer)| (*id, layer.name.clone()))
        .collect();

    copper.sort_by(|a, b| match (a.1 == "B.Cu", b.1 == "B.Cu") {
        (true, true) => std::cmp::Ordering::Equal,
        (true, false) => std::cmp::Ordering::Greater,
        (false, true) => std::cmp::Ordering::Less,
        _ => a.0.cmp(&b.0),
    });
    copper.into_iter().map(|(_, name)| name).collect()
}

/// Comma-joined copper stack for the kicad-cli `--layers` argument.
fn copper_layers_from_pcb(pcb_path: &Path) -> String {
    let stack = copper_layers_from_pcb_vec(pcb_path);
    if stack.is_empty() { "F.Cu,B.Cu".into() } else { stack.join(",") }
}

/// Load gerber files from `gerber_dir` into the app's layer store. The
/// `pcb_path` is read to pull the KiCad 10 canonical names for User.N
/// slots out of the `.kicad_pcb` (e.g. "M1 Board Outline", "Top 3D Body")
/// so the View Settings panel shows KiCad's own labels.
pub fn load_gerbers(
    services: &mut SharedServices,
    pcb_path: &Path,
    gerber_dir: &Path,
    logger: &ReactiveEventLogger,
) {
    logger.log_info("Clearing existing gerber layers...");
    services.layer_store.clear_all();

    let copper_stack = copper_layers_from_pcb_vec(pcb_path);
    if !copper_stack.is_empty() {
        let cc = copper_stack.len() as u8;
        logger.log_info(&format!(
            "Board has {} copper layers: {}", cc, copper_stack.join(", ")));
        services.layer_store.set_copper_count(cc);
    }

    match crate::project_manager::kicad_metadata::read_user_layer_names(pcb_path) {
        Ok(names) => {
            if !names.is_empty() {
                logger.log_info(&format!("Parsed {} formal user-layer name(s) from .kicad_pcb", names.len()));
            }
            services.layer_store.user_layer_names = names;
        }
        Err(e) => {
            logger.log_warning(&format!("Could not parse user-layer names from PCB: {}", e));
        }
    }

    match services.layer_store.load_from_directory(gerber_dir) {
        Ok((loaded_count, unassigned_count)) => {
            if loaded_count > 0 {
                logger.log_info(&format!("Successfully loaded {} gerber layers", loaded_count));
                services.gerber_view.needs_initial_view = true;
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
    services.geometry.board_outline = extract_outline_from_layer_store(&services.layer_store, logger);

    // Copper layers (Phase 4a). Require an outline bbox so the copper mesh
    // lines up with the board mesh — both share the same world transform
    // (Stage 6 of the FDD pipeline, centered at outline bbox).
    if let Some(outline) = services.geometry.board_outline.as_ref() {
        let outline_bbox = outline.bbox.clone();
        let outline_contours = outline.contours.clone();
        services.geometry.top_copper = extract_copper_side(
            &services.layer_store,
            crate::layer_store::LayerType::Copper(1),
            "F.Cu",
            &outline_bbox,
            logger,
        );
        services.geometry.bottom_copper = extract_copper_side(
            &services.layer_store,
            services.layer_store.bottom_copper_type(),
            "B.Cu",
            &outline_bbox,
            logger,
        );
        // Inner copper layers: stack positions Copper(2)..Copper(N-1), between
        // F.Cu (Copper(1)) and B.Cu (Copper(N)). Empty on 2-layer boards.
        let copper_count = services.layer_store.copper_count;
        let mut inner_copper = Vec::new();
        for n in 2..copper_count {
            let name = format!("In{}.Cu", n - 1);
            if let Some(cu) = extract_copper_side(
                &services.layer_store,
                crate::layer_store::LayerType::Copper(n),
                &name,
                &outline_bbox,
                logger,
            ) {
                inner_copper.push((n, cu));
            }
        }
        services.geometry.inner_copper = inner_copper;
        // Silkscreen reuses the copper extractor — the silk gerber draws the
        // same primitive types, so its mesh IR is a `CopperData`.
        services.geometry.top_silk = extract_copper_side(
            &services.layer_store,
            crate::layer_store::LayerType::Silkscreen(crate::layer_store::Side::Top),
            "F.SilkS",
            &outline_bbox,
            logger,
        );
        services.geometry.bottom_silk = extract_copper_side(
            &services.layer_store,
            crate::layer_store::LayerType::Silkscreen(crate::layer_store::Side::Bottom),
            "B.SilkS",
            &outline_bbox,
            logger,
        );
        services.geometry.top_mask = extract_mask_side(
            &services.layer_store,
            crate::layer_store::LayerType::Soldermask(crate::layer_store::Side::Top),
            "F.Mask",
            &outline_contours,
            &outline_bbox,
            logger,
        );
        services.geometry.bottom_mask = extract_mask_side(
            &services.layer_store,
            crate::layer_store::LayerType::Soldermask(crate::layer_store::Side::Bottom),
            "B.Mask",
            &outline_contours,
            &outline_bbox,
            logger,
        );
        services.geometry.drill = extract_drill_side(
            &services.layer_store,
            &outline_bbox,
            logger,
        );
    } else {
        services.geometry.top_copper = None;
        services.geometry.bottom_copper = None;
        services.geometry.inner_copper.clear();
        services.geometry.top_silk = None;
        services.geometry.bottom_silk = None;
        services.geometry.top_mask = None;
        services.geometry.bottom_mask = None;
        services.geometry.drill = None;
    }

    // Signal the new geometry so the 3D panel rebuilds — works no matter who
    // called us (app ribbon or a dock panel through services).
    services.board_geometry_gen = services.board_geometry_gen.wrapping_add(1);
}

/// App-level load: drive [`load_gerbers`] over the shared services, then refresh
/// the 3D panel — which lives on the app, not services. Its per-layer change
/// detection treats Some→Some as "no change", so loading a different board
/// would otherwise keep showing the old one's GPU meshes.
pub fn load_gerbers_into_viewer(
    app: &mut CopperForgeApp,
    pcb_path: &Path,
    gerber_dir: &Path,
    logger: &ReactiveEventLogger,
) {
    load_gerbers(&mut app.services, pcb_path, gerber_dir, logger);
    app.gerber_view_3d_panel.mark_dirty();
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
    // Log the filename each slot landed on so bottom/top swaps caused by
    // upstream filename detection are visible without a code trace.
    logger.log_info(&format!(
        "{} copper: reading {}",
        label,
        path.file_name().map(|f| f.to_string_lossy().to_string()).unwrap_or_default(),
    ));
    match crate::gerber_geom::extract_copper(path, outline_bbox) {
        Some((data, counts)) => {
            logger.log_info(&format!(
                "{} copper: {} circle + {} rect + {} roundrect + {} obround + {} polygon flash(es); {} linear stroke(s); {} region polygon(s); {} macros / {} arc-strokes / {} non-circle-strokes skipped",
                label,
                counts.flashed_circles,
                counts.flashed_rectangles,
                counts.flashed_roundrects,
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

/// Look up the drill layer (exported as gerber by kicad-cli) and recover its
/// hole centres + radii, aligned to the board outline's bbox so the 3D hole
/// disks land on their pads.
fn extract_drill_side(
    store: &crate::layer_store::LayerStore,
    outline_bbox: &gerber_viewer::BoundingBox,
    logger: &ReactiveEventLogger,
) -> Option<crate::gerber_geom::DrillData> {
    let layer = store.find(crate::layer_store::LayerType::Drill)?;
    let path = layer.file_path.as_ref()?;
    logger.log_info(&format!(
        "drill: reading {}",
        path.file_name().map(|f| f.to_string_lossy().to_string()).unwrap_or_default(),
    ));
    match crate::gerber_geom::extract_drill_gerber(path, outline_bbox) {
        Some(d) => {
            logger.log_info(&format!("drill: {} hole(s)", d.holes.len()));
            Some(d)
        }
        None => {
            logger.log_warning("drill: no holes extracted");
            None
        }
    }
}

/// Look up a soldermask layer in the store and extract its polygon IR as
/// a green-sheet-with-holes mesh. The openings in the gerber become holes
/// in the mask; the board outline contours provide the outer sheet
/// boundary (plus any cutouts / slots as additional holes).
fn extract_mask_side(
    store: &crate::layer_store::LayerStore,
    layer_type: crate::layer_store::LayerType,
    label: &str,
    outline_contours: &[Vec<nalgebra::Point2<f32>>],
    outline_bbox: &gerber_viewer::BoundingBox,
    logger: &ReactiveEventLogger,
) -> Option<crate::gerber_geom::MaskData> {
    let layer = store.find(layer_type)?;
    let path = layer.file_path.as_ref()?;
    logger.log_info(&format!(
        "{} mask: reading {}",
        label,
        path.file_name().map(|f| f.to_string_lossy().to_string()).unwrap_or_default(),
    ));
    match crate::gerber_geom::extract_mask(path, outline_contours, outline_bbox) {
        Some((data, counts)) => {
            logger.log_info(&format!(
                "{} mask: {} circle + {} rect + {} roundrect + {} obround + {} polygon opening(s); {} linear stroke(s); {} region polygon(s); {} macros / {} arc-strokes / {} non-circle-strokes skipped",
                label,
                counts.flashed_circles,
                counts.flashed_rectangles,
                counts.flashed_roundrects,
                counts.flashed_obrounds,
                counts.flashed_polygons,
                counts.linear_strokes,
                counts.region_polygons,
                counts.flashed_macros_skipped,
                counts.arc_strokes_skipped,
                counts.non_circle_strokes_skipped,
            ));
            logger.log_info(&format!(
                "{} mask: {} triangle(s) tessellated",
                label,
                data.mesh_indices.len() / 3,
            ));
            Some(data)
        }
        None => {
            logger.log_warning(&format!("{} mask: no geometry extracted", label));
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
