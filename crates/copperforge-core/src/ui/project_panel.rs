//! Project tab — Import an existing KiCad project into the CopperForge DB.
//!
//! This tab is import-only. Creating brand-new KiCad projects from templates
//! is available via the Shell panel's `new-project` command.

use crate::CopperForgeApp;
use crate::project_manager::ProjectManagerState;
use crate::event_logger::{ReactiveEventLogger, ReactiveEventLoggerState, LogColors};
use egui_mobius_reactive::Dynamic;
use egui_file_dialog::FileDialog;

pub fn show_project_panel<'a>(
    ui: &mut egui::Ui,
    app: &'a mut CopperForgeApp,
    logger_state: &'a Dynamic<ReactiveEventLoggerState>,
    log_colors: &'a Dynamic<LogColors>,
) {
    let logger = ReactiveEventLogger::with_colors(logger_state, log_colors);

    ui.heading("📥 Import KiCad Project");
    ui.separator();

    egui::ScrollArea::vertical()
        .auto_shrink([false, false])
        .show(ui, |ui| {
            show_import_form(ui, app, &logger);
        });
}

fn show_import_form(
    ui: &mut egui::Ui,
    app: &mut CopperForgeApp,
    logger: &ReactiveEventLogger,
) {
    // Initialize project manager state (clones the shared DB handle).
    if app.project_manager_state.is_none() {
        let mut state = ProjectManagerState::with_config(&app.services.config);
        if let Err(e) = state.initialize_database(&app.services.project_db) {
            logger.log_error(&format!("Failed to initialize project database: {}", e));
        }
        app.project_manager_state = Some(state);
    }

    let manager_state = match app.project_manager_state.as_mut() {
        Some(s) => s,
        None => return,
    };

    if let Some(error) = manager_state.last_error.take() {
        logger.log_error(&error);
    }

    ui.vertical(|ui| {
        // ── File pick flow ────────────────────────────────────
        // Handle the dialog pick *first* so UI below reflects new values this frame.
        if let Some(pro_path) = manager_state.pcb_file_dialog.update(ui.ctx()).picked() {
            let pro_path = pro_path.to_path_buf();
            let should_process = manager_state.last_picked_pro_path.as_ref() != Some(&pro_path);

            if should_process {
                manager_state.last_picked_pro_path = Some(pro_path.clone());
                manager_state.new_project_pcb_path = Some(pro_path.with_extension("kicad_pcb"));

                // Auto-fill from .kicad_pro metadata.
                match crate::project_manager::kicad_metadata::read_kicad_metadata(&pro_path) {
                    Ok(metadata) => {
                        let mut missing = Vec::new();
                        if metadata.author.is_none() { missing.push("Author"); }
                        if metadata.company.is_none() { missing.push("Company"); }

                        if let Some(desc) = metadata.description {
                            if manager_state.new_project_description.is_empty() {
                                manager_state.new_project_description = desc;
                            }
                        }

                        if manager_state.new_project_name.is_empty() {
                            if let Some(stem) = pro_path.file_stem() {
                                manager_state.new_project_name = stem.to_string_lossy().into_owned();
                            }
                        }

                        logger.log_info(&format!("Loaded pedigree from: {}", pro_path.display()));
                        if !missing.is_empty() {
                            logger.log_warning(&format!(
                                "Missing pedigree fields in .kicad_pro: {} — set in KiCad → Project Properties",
                                missing.join(", ")
                            ));
                        }
                    }
                    Err(e) => logger.log_warning(&format!("Could not read pedigree: {}", e)),
                }
            }
        }

        // ── File picker ───────────────────────────────────────
        ui.horizontal(|ui| {
            ui.label("KiCad Project File (.kicad_pro):");
            if ui.button("Browse...").clicked() {
                use std::sync::Arc;
                use std::mem;

                let dialog = mem::replace(&mut manager_state.pcb_file_dialog, FileDialog::new());
                let mut dialog = dialog
                    .add_file_filter("KiCad Project", Arc::new(|path: &std::path::Path| {
                        path.extension()
                            .and_then(|e| e.to_str())
                            .map(|e| e == "kicad_pro")
                            .unwrap_or(false)
                    }))
                    .default_file_filter("KiCad Project");
                if let Some(ref dir) = app.services.config.preferred_projects_directory {
                    dialog = dialog.initial_directory(dir.clone());
                }
                manager_state.pcb_file_dialog = dialog;
                manager_state.pcb_file_dialog.pick_file();
            }
        });

        let picked_label = manager_state.new_project_pcb_path.as_ref()
            .map(|p| p.with_extension("kicad_pro").file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_else(|| "Unknown file".into()))
            .unwrap_or_else(|| "No KiCad project file selected".into());
        ui.label(egui::RichText::new(&picked_label).small().monospace());

        ui.add_space(8.0);

        // ── Metadata fields (editable) ────────────────────────
        ui.label("Project Name:");
        ui.text_edit_singleline(&mut manager_state.new_project_name);
        ui.add_space(3.0);

        ui.label("Description:");
        ui.text_edit_multiline(&mut manager_state.new_project_description);
        ui.add_space(3.0);

        ui.label("Tags (comma-separated):");
        ui.text_edit_singleline(&mut manager_state.new_project_tags);
        ui.add_space(10.0);

        // ── Import button ─────────────────────────────────────
        if ui.button("📥 Import Project").clicked() {
            if manager_state.new_project_name.trim().is_empty() {
                manager_state.last_error = Some("Project name cannot be empty".into());
                return;
            }
            let pcb_path = match manager_state.new_project_pcb_path.clone() {
                Some(p) => p,
                None => {
                    manager_state.last_error = Some("Please select a .kicad_pro file first".into());
                    return;
                }
            };

            let tags: Vec<String> = manager_state.new_project_tags
                .split(',')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect();

            let bom_components: Vec<crate::project_manager::bom::BomComponent> =
                if let Some(ref bom_state) = app.bom_panel.state {
                    bom_state.entries.iter().cloned().map(Into::into).collect()
                } else {
                    Vec::new()
                };

            let result = manager_state.create_project(
                manager_state.new_project_name.clone(),
                manager_state.new_project_description.clone(),
                pcb_path,
                tags,
                bom_components,
            );

            match result {
                Ok(id) => {
                    logger.log_info(&format!(
                        "Imported project: {} (ID: {})",
                        manager_state.new_project_name, id
                    ));
                    manager_state.reset_create_dialog();
                }
                Err(e) => {
                    manager_state.last_error = Some(format!("Failed to import project: {}", e));
                }
            }
        }
    });
}
