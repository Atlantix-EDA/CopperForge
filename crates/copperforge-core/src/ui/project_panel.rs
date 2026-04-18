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

    ui.heading("➕ Create New Project");
    ui.separator();

    // Use a scroll area to ensure all content is visible
    egui::ScrollArea::vertical()
        .auto_shrink([false, false])
        .show(ui, |ui| {
            show_create_project_form(ui, app, &logger);
        });
}

/// Show create project form (static, not modal)
fn show_create_project_form(
    ui: &mut egui::Ui,
    app: &mut CopperForgeApp,
    logger: &ReactiveEventLogger,
) {
    // Initialize project manager state if not already done
    if app.project_manager_state.is_none() {
        let mut state = ProjectManagerState::with_config(&app.services.config);

        // Initialize database
        let db_path = app.services.config_path.join("projects.db");
        if let Err(e) = state.initialize_database(&db_path) {
            logger.log_error(&format!("Failed to initialize project database: {}", e));
        }

        app.project_manager_state = Some(state);
    }

    if let Some(ref mut manager_state) = app.project_manager_state {
        // Handle any errors
        if let Some(error) = manager_state.last_error.take() {
            logger.log_error(&error);
        }

        ui.vertical(|ui| {
                    // Toggle between creating new or importing existing
                    ui.horizontal(|ui| {
                        ui.label("Project Type:");
                    });
                    ui.radio_value(&mut manager_state.create_new_kicad_project, true, "🆕 Create New KiCad Project");
                    ui.radio_value(&mut manager_state.create_new_kicad_project, false, "📂 Import Existing PCB");

                    ui.add_space(5.0);

                    // Common fields
                    ui.label("Project Name:");

                    ui.horizontal(|ui| {
                        // Text entry field always visible for editing
                        ui.text_edit_singleline(&mut manager_state.new_project_name);

                        // Show ComboBox with recent project names
                        egui::ComboBox::from_id_salt("project_name_combo")
                            .selected_text("📋 Recent")
                            .show_ui(ui, |ui| {
                                if !manager_state.recent_project_names.is_empty() {
                                    // Clone the list to avoid borrow issues
                                    let recent_names = manager_state.recent_project_names.clone();
                                    for recent_name in &recent_names {
                                        if ui.selectable_label(false, recent_name).clicked() {
                                            // Load full project metadata (name, description, tags)
                                            manager_state.load_project_metadata_into_form(recent_name);
                                        }
                                    }
                                } else {
                                    ui.label(egui::RichText::new("No recent projects").small().italics());
                                }
                            });
                    });

                    ui.add_space(3.0);

                    ui.label("Description:");
                    ui.text_edit_multiline(&mut manager_state.new_project_description);

                    ui.add_space(3.0);

                    ui.label("Tags (comma-separated):");
                    ui.text_edit_singleline(&mut manager_state.new_project_tags);

                    ui.add_space(5.0);

                    // Show different fields based on project type
                    if manager_state.create_new_kicad_project {
                        ui.label(egui::RichText::new("New KiCad Project Settings").strong());
                        ui.separator();

                        egui::Grid::new("kicad_project_settings_grid")
                            .num_columns(2)
                            .spacing([10.0, 5.0])
                            .show(ui, |ui| {
                                // Location
                                ui.label("Location:");
                                ui.horizontal(|ui| {
                                    let location_text = manager_state.new_kicad_project_location
                                        .to_string_lossy()
                                        .to_string();
                                    ui.label(egui::RichText::new(&location_text).small().monospace());
                                    if ui.button("Browse...").clicked() {
                                        manager_state.location_dialog.pick_directory();
                                    }
                                });
                                ui.end_row();

                                // Author
                                ui.label("Author:");
                                ui.text_edit_singleline(&mut manager_state.new_kicad_project_author);
                                ui.end_row();

                                // Company
                                ui.label("Company:");
                                ui.text_edit_singleline(&mut manager_state.new_kicad_project_company);
                                ui.end_row();
                            });

                        // Handle location dialog
                        if let Some(path) = manager_state.location_dialog.update(ui.ctx()).picked() {
                            manager_state.new_kicad_project_location = path.to_path_buf();
                        }

                    } else {
                        // Import existing KiCad project
                        ui.label(egui::RichText::new("Import Existing KiCad Project").strong());
                        ui.separator();

                        // Handle KiCad project file dialog FIRST (before UI rendering)
                        if let Some(pro_path) = manager_state.pcb_file_dialog.update(ui.ctx()).picked() {
                            let pro_path = pro_path.to_path_buf();

                            // Only process if this is a NEW file selection (not already processed)
                            let should_process = manager_state.last_picked_pro_path.as_ref() != Some(&pro_path);

                            if should_process {
                                // Store the path to prevent re-processing
                                manager_state.last_picked_pro_path = Some(pro_path.clone());

                                // Convert .kicad_pro path to .kicad_pcb path
                                let pcb_path = pro_path.with_extension("kicad_pcb");
                                manager_state.new_project_pcb_path = Some(pcb_path);

                                // Read pedigree information from .kicad_pro file
                                match crate::project_manager::kicad_metadata::read_kicad_metadata(&pro_path) {
                                    Ok(metadata) => {
                                        let mut missing_fields = Vec::new();

                                        // Auto-populate form fields with pedigree data
                                        if let Some(author) = metadata.author {
                                            manager_state.new_kicad_project_author = author;
                                        } else {
                                            missing_fields.push("Author");
                                        }

                                        if let Some(company) = metadata.company {
                                            manager_state.new_kicad_project_company = company;
                                        } else {
                                            missing_fields.push("Company");
                                        }

                                        if let Some(description) = metadata.description {
                                            // Only populate if empty to avoid overwriting user input
                                            if manager_state.new_project_description.is_empty() {
                                                manager_state.new_project_description = description;
                                            }
                                        }

                                        // Use filename as project name if not already set
                                        if manager_state.new_project_name.is_empty() {
                                            if let Some(file_stem) = pro_path.file_stem() {
                                                manager_state.new_project_name = file_stem.to_string_lossy().to_string();
                                            }
                                        }

                                        logger.log_info(&format!("Loaded pedigree from: {}", pro_path.display()));

                                        // Warn about missing pedigree fields
                                        if !missing_fields.is_empty() {
                                            logger.log_warning(&format!("Missing pedigree information in .kicad_pro: {}", missing_fields.join(", ")));
                                        }
                                    }
                                    Err(e) => {
                                        logger.log_warning(&format!("Could not read pedigree from .kicad_pro: {}", e));
                                    }
                                }
                            }
                        }

                        ui.horizontal(|ui| {
                            ui.label("KiCad Project File (.kicad_pro):");

                            if ui.button("Browse...").clicked() {
                                use std::sync::Arc;
                                use std::mem;

                                // Take the dialog, add filter, and put it back
                                let dialog = mem::replace(&mut manager_state.pcb_file_dialog, FileDialog::new());
                                let mut dialog = dialog
                                    .add_file_filter("KiCad Project", Arc::new(|path: &std::path::Path| {
                                        path.extension()
                                            .and_then(|ext| ext.to_str())
                                            .map(|ext| ext == "kicad_pro")
                                            .unwrap_or(false)
                                    }));

                                // Set initial directory from preferences if available
                                if let Some(ref preferred_dir) = app.services.config.preferred_projects_directory {
                                    dialog = dialog.initial_directory(preferred_dir.clone());
                                }

                                manager_state.pcb_file_dialog = dialog;
                                manager_state.pcb_file_dialog.pick_file();
                            }
                        });

                        let pcb_file_text = if let Some(ref path) = manager_state.new_project_pcb_path {
                            path.file_name()
                                .map(|n| n.to_string_lossy().to_string())
                                .unwrap_or_else(|| "Unknown file".to_string())
                        } else {
                            "No KiCad project file selected".to_string()
                        };
                        ui.label(egui::RichText::new(&pcb_file_text).small().monospace());

                        ui.add_space(5.0);

                        // Show pedigree fields (auto-populated from .kicad_pro)
                        ui.label(egui::RichText::new("Pedigree Information").strong());
                        ui.label(egui::RichText::new("💡 These fields are auto-populated from the .kicad_pro file").small().italics());
                        ui.separator();

                        egui::Grid::new("import_pedigree_grid")
                            .num_columns(2)
                            .spacing([10.0, 5.0])
                            .show(ui, |ui| {
                                // Author
                                ui.label("Author:");
                                ui.text_edit_singleline(&mut manager_state.new_kicad_project_author);
                                ui.end_row();

                                // Company
                                ui.label("Company:");
                                ui.text_edit_singleline(&mut manager_state.new_kicad_project_company);
                                ui.end_row();
                            });
                    }

                    ui.add_space(10.0);

                    // Button text changes based on mode
                    let button_text = if manager_state.create_new_kicad_project {
                        "✅ Create Project"
                    } else {
                        "📥 Import Project"
                    };

                    if ui.button(button_text).clicked() {
                        // Validate input
                        if manager_state.new_project_name.trim().is_empty() {
                            manager_state.last_error = Some("Project name cannot be empty".to_string());
                            return;
                        }

                        // Parse tags
                        let tags: Vec<String> = manager_state.new_project_tags
                            .split(',')
                            .map(|s| s.trim().to_string())
                            .filter(|s| !s.is_empty())
                            .collect();

                        // Get BOM components
                        let bom_components: Vec<crate::project_manager::bom::BomComponent> = if let Some(ref bom_state) = app.bom_state {
                            bom_state.entries.iter().cloned().map(Into::into).collect()
                        } else {
                            Vec::new()
                        };

                        // Create project based on type
                        let result = if manager_state.create_new_kicad_project {
                            // Create new KiCad project from scratch
                            manager_state.create_new_kicad_project_from_scratch(
                                manager_state.new_project_name.clone(),
                                manager_state.new_project_description.clone(),
                                tags,
                            )
                        } else {
                            // Import existing PCB
                            let pcb_path = if let Some(ref path) = manager_state.new_project_pcb_path {
                                path.clone()
                            } else {
                                manager_state.last_error = Some("Please select a PCB file first".to_string());
                                return;
                            };

                            manager_state.create_project(
                                manager_state.new_project_name.clone(),
                                manager_state.new_project_description.clone(),
                                pcb_path,
                                tags,
                                bom_components,
                            )
                        };

                        match result {
                            Ok(project_id) => {
                                let action = if manager_state.create_new_kicad_project { "Created" } else { "Imported" };
                                logger.log_info(&format!("{} project: {} (ID: {})", action, manager_state.new_project_name, project_id));
                                // Only reset on success - this keeps user preferences but clears project fields
                                manager_state.reset_create_dialog();
                            }
                            Err(e) => {
                                // Don't reset on error - user can fix the issue and try again
                                let action = if manager_state.create_new_kicad_project { "create" } else { "import" };
                                manager_state.last_error = Some(format!("Failed to {} project: {}", action, e));
                            }
                        }
                    }
        });
    }
}

