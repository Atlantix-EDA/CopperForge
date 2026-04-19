#![allow(dead_code)]
use crate::CopperForgeApp;
use crate::project_manager::ProjectManagerState;
use crate::event_logger::{ReactiveEventLogger, ReactiveEventLoggerState, LogColors};
use egui_mobius_reactive::Dynamic;

/// Show the project manager panel
pub fn show_project_manager_panel(
    ui: &mut egui::Ui,
    app: &mut CopperForgeApp,
    logger_state: &Dynamic<ReactiveEventLoggerState>,
    log_colors: &Dynamic<LogColors>,
) {
    let logger = ReactiveEventLogger::with_colors(logger_state, log_colors);
    
    // Split app borrow to avoid conflicts
    let bom_components: Option<Vec<crate::project_manager::bom::BomComponent>> = if let Some(ref bom_state) = app.bom_panel.state {
        Some(bom_state.entries.iter().cloned().map(Into::into).collect())
    } else {
        None
    };
    
    let project_state = &app.services.project_state.get();
    
    if let Some(ref mut manager_state) = app.project_manager_state {
        // Handle any errors
        if let Some(error) = manager_state.last_error.take() {
            logger.log_error(&error);
        }
        
        ui.heading("📁 Project Manager");
        ui.separator();
        
        // Top controls
        ui.horizontal(|ui| {
            // Search
            ui.label("🔍 Search:");
            let search_changed = ui.text_edit_singleline(&mut manager_state.search_query).changed();
            
            if search_changed {
                if let Err(e) = manager_state.search_projects(&manager_state.search_query.clone()) {
                    manager_state.last_error = Some(format!("Search failed: {}", e));
                }
            }
            
            ui.separator();
            
            // Create new project button
            if ui.button("➕ New Project").clicked() {
                manager_state.show_create_dialog = true;
            }
            
            ui.separator();
            
            // Current project info
            let current_project_name = manager_state.current_project
                .as_ref()
                .map(|p| p.metadata.name.clone());
            
            if let Some(ref project_name) = current_project_name {
                ui.label(format!("📋 Current: {}", project_name));
                
                // Save BOM to current project
                if ui.button("💾 Save BOM").clicked() {
                    if let Some(ref components) = bom_components {
                        if let Err(e) = manager_state.update_project_bom(components.clone()) {
                            manager_state.last_error = Some(format!("Failed to save BOM: {}", e));
                        } else {
                            logger.log_info(&format!("Saved BOM to project: {}", project_name));
                        }
                    }
                }
            } else {
                ui.label("📋 No project loaded");
            }
        });
        
        ui.separator();
        
        // Project list
        ui.vertical(|ui| {
            if manager_state.project_list.is_empty() {
                ui.centered_and_justified(|ui| {
                    ui.label("No projects found. Create your first project!");
                });
            } else {
                // Clone project list and current project id to avoid borrowing issues
                let project_list = manager_state.project_list.clone();
                let current_project_id = manager_state.current_project
                    .as_ref()
                    .map(|p| p.metadata.id.clone());
                let expanded_project_id = manager_state.expanded_project_id.clone();
                let project_hierarchies = manager_state.project_hierarchies.clone();
                
                // Project table
                egui_extras::TableBuilder::new(ui)
                    .striped(true)
                    .cell_layout(egui::Layout::left_to_right(egui::Align::Center))
                    .column(egui_extras::Column::exact(200.0))  // Name
                    .column(egui_extras::Column::remainder())   // Description
                    .column(egui_extras::Column::exact(120.0))  // Last Modified
                    .column(egui_extras::Column::exact(120.0))  // Actions
                    .header(20.0, |mut header| {
                        header.col(|ui| { ui.strong("Project Name"); });
                        header.col(|ui| { ui.strong("Description"); });
                        header.col(|ui| { ui.strong("Last Modified"); });
                        header.col(|ui| { ui.strong("Actions"); });
                    })
                    .body(|mut body| {
                        for project in &project_list {
                            // Check if this project is expanded
                            let is_expanded = expanded_project_id.as_ref() == Some(&project.id);

                            body.row(18.0, |mut row| {
                                // Project name (with double-click to expand)
                                row.col(|ui| {
                                    let is_current = current_project_id
                                        .as_ref()
                                        .map(|id| id == &project.id)
                                        .unwrap_or(false);

                                    let expand_icon = if is_expanded { "▼" } else { "▶" };
                                    let text = if is_current {
                                        egui::RichText::new(format!("{} {}", expand_icon, &project.name)).strong().color(egui::Color32::LIGHT_BLUE)
                                    } else {
                                        egui::RichText::new(format!("{} {}", expand_icon, &project.name))
                                    };

                                    // Use selectable label which is interactive
                                    let _response = ui.selectable_label(false, text);
                                });
                                
                                // Description
                                row.col(|ui| {
                                    ui.label(&project.description);
                                });
                                
                                // Last modified
                                row.col(|ui| {
                                    let date_str = project.last_modified.format("%m/%d/%Y").to_string();
                                    ui.label(date_str);
                                });
                                
                                // Actions
                                row.col(|ui| {
                                    ui.horizontal(|ui| {
                                        // Load project button
                                        if ui.small_button("📂 Load").clicked() {
                                            ui.ctx().memory_mut(|mem| {
                                                mem.data.insert_temp(egui::Id::new("load_project"), project.id.clone());
                                            });
                                        }
                                        
                                        // Delete project button
                                        if ui.small_button("🗑️").clicked() {
                                            ui.ctx().memory_mut(|mem| {
                                                mem.data.insert_temp(egui::Id::new("delete_project"), project.id.clone());
                                            });
                                        }
                                    });
                                });
                            });

                            // Show tree view if this project is expanded
                            if is_expanded {
                                // Display hierarchy tree
                                if let Some(hierarchy) = project_hierarchies.get(&project.id) {
                                    body.row(18.0, |mut row| {
                                        row.col(|ui| {
                                            ui.indent("tree_indent", |ui| {
                                                show_hierarchy_tree(ui, hierarchy);
                                            });
                                        });
                                        // Empty columns for alignment
                                        row.col(|_ui| {});
                                        row.col(|_ui| {});
                                        row.col(|_ui| {});
                                    });
                                }
                            }
                        }
                    });
            }
        });

        // Handle actions stored in memory
        let load_project_id = ui.ctx().memory(|mem| {
            mem.data.get_temp::<String>(egui::Id::new("load_project"))
        });
        let delete_project_id = ui.ctx().memory(|mem| {
            mem.data.get_temp::<String>(egui::Id::new("delete_project"))
        });
        let _toggle_expand_id = ui.ctx().memory(|mem| {
            mem.data.get_temp::<String>(egui::Id::new("toggle_expand"))
        });
        
        if let Some(project_id) = load_project_id {
            ui.ctx().memory_mut(|mem| {
                mem.data.remove::<String>(egui::Id::new("load_project"));
            });
            
            let project_name = manager_state.project_list
                .iter()
                .find(|p| p.id == project_id)
                .map(|p| p.name.clone())
                .unwrap_or_else(|| "Unknown".to_string());
                
            if let Err(e) = manager_state.load_project(&project_id) {
                manager_state.last_error = Some(format!("Failed to load project: {}", e));
            } else {
                logger.log_info(&format!("Loaded project: {}", project_name));
            }
        }
        
        if let Some(project_id) = delete_project_id {
            ui.ctx().memory_mut(|mem| {
                mem.data.remove::<String>(egui::Id::new("delete_project"));
            });
            manager_state.show_delete_confirmation = Some(project_id);
        }

        // Note: This code is no longer used - tree view is in projects_panel.rs

        let show_create = manager_state.show_create_dialog;
        let show_delete_id = manager_state.show_delete_confirmation.clone();
        
        // Create project dialog
        if show_create {
            show_create_project_dialog(ui.ctx(), manager_state, project_state, bom_components.unwrap_or_default(), &logger);
        }
        
        // Delete confirmation dialog
        if let Some(ref project_id) = show_delete_id {
            show_delete_confirmation_dialog(ui.ctx(), manager_state, project_id, &logger);
        }
    }
}

/// Show create project dialog
pub fn show_create_project_dialog(
    ctx: &egui::Context,
    manager_state: &mut ProjectManagerState,
    _project_state: &crate::project::ProjectState,
    bom_components: Vec<crate::project_manager::bom::BomComponent>,
    logger: &ReactiveEventLogger,
) {
    egui::Window::new("Import KiCad Project")
        .id(egui::Id::new("create_project_dialog"))
        .collapsible(false)
        .resizable(false)
        .movable(true)
        .default_pos(egui::Pos2::new(300.0, 150.0))
        .min_size(egui::Vec2::new(550.0, 400.0))
        .show(ctx, |ui| {
            ui.vertical(|ui| {
                // Handle the file dialog FIRST so auto-population runs this frame.
                if let Some(pro_path) = manager_state.pcb_file_dialog.update(ui.ctx()).picked() {
                    let pro_path = pro_path.to_path_buf();
                    let should_process = manager_state.last_picked_pro_path.as_ref() != Some(&pro_path);
                    if should_process {
                        manager_state.last_picked_pro_path = Some(pro_path.clone());
                        manager_state.new_project_pcb_path = Some(pro_path.with_extension("kicad_pcb"));
                        if let Ok(meta) = crate::project_manager::kicad_metadata::read_kicad_metadata(&pro_path) {
                            if let Some(desc) = meta.description {
                                if manager_state.new_project_description.is_empty() {
                                    manager_state.new_project_description = desc;
                                }
                            }
                            if manager_state.new_project_name.is_empty() {
                                if let Some(stem) = pro_path.file_stem() {
                                    manager_state.new_project_name = stem.to_string_lossy().into_owned();
                                }
                            }
                        }
                    }
                }

                ui.horizontal(|ui| {
                    ui.label("KiCad Project File (.kicad_pro):");
                    let pro_file_text = manager_state.new_project_pcb_path.as_ref()
                        .map(|p| p.with_extension("kicad_pro").file_name()
                            .map(|n| n.to_string_lossy().into_owned())
                            .unwrap_or_else(|| "Unknown file".into()))
                        .unwrap_or_else(|| "No KiCad project file selected".into());
                    ui.label(&pro_file_text);

                    if ui.button("Browse...").clicked() {
                        use std::sync::Arc;
                        use std::mem;
                        use egui_file_dialog::FileDialog;
                        let dialog = mem::replace(&mut manager_state.pcb_file_dialog, FileDialog::new());
                        manager_state.pcb_file_dialog = dialog
                            .add_file_filter("KiCad Project", Arc::new(|path: &std::path::Path| {
                                path.extension()
                                    .and_then(|e| e.to_str())
                                    .map(|e| e == "kicad_pro")
                                    .unwrap_or(false)
                            }))
                            .default_file_filter("KiCad Project");
                        manager_state.pcb_file_dialog.pick_file();
                    }
                });

                ui.add_space(10.0);

                ui.label("Project Name:");
                ui.horizontal(|ui| {
                    ui.text_edit_singleline(&mut manager_state.new_project_name);
                    egui::ComboBox::from_id_salt("project_name_combo_dialog")
                        .selected_text("📋 Recent")
                        .show_ui(ui, |ui| {
                            if manager_state.recent_project_names.is_empty() {
                                ui.label(egui::RichText::new("No recent projects").small().italics());
                            } else {
                                let recent = manager_state.recent_project_names.clone();
                                for n in &recent {
                                    if ui.selectable_label(false, n).clicked() {
                                        manager_state.load_project_metadata_into_form(n);
                                    }
                                }
                            }
                        });
                });

                ui.add_space(5.0);
                ui.label("Description:");
                ui.text_edit_multiline(&mut manager_state.new_project_description);

                ui.add_space(5.0);
                ui.label("Tags (comma-separated):");
                ui.text_edit_singleline(&mut manager_state.new_project_tags);

                ui.add_space(15.0);

                ui.horizontal(|ui| {
                    if ui.button("📥 Import").clicked() {
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
                        match manager_state.create_project(
                            manager_state.new_project_name.clone(),
                            manager_state.new_project_description.clone(),
                            pcb_path,
                            tags,
                            bom_components.clone(),
                        ) {
                            Ok(id) => {
                                logger.log_info(&format!(
                                    "Imported project: {} (ID: {})",
                                    manager_state.new_project_name, id
                                ));
                                manager_state.reset_create_dialog();
                            }
                            Err(e) => {
                                manager_state.last_error = Some(format!("Failed to import: {}", e));
                            }
                        }
                    }

                    if ui.button("Cancel").clicked() {
                        manager_state.show_create_dialog = false;
                    }
                });
            });
        });
}

/// Show delete confirmation dialog
fn show_delete_confirmation_dialog(
    ctx: &egui::Context,
    manager_state: &mut ProjectManagerState,
    project_id: &str,
    logger: &ReactiveEventLogger,
) {
    let project_name = manager_state.project_list
        .iter()
        .find(|p| p.id == project_id)
        .map(|p| p.name.clone())
        .unwrap_or_else(|| "Unknown".to_string());
    
    egui::Window::new("Delete Project")
        .collapsible(false)
        .resizable(false)
        .movable(true)
        .default_pos(egui::Pos2::new(400.0, 300.0))
        .show(ctx, |ui| {
            ui.vertical(|ui| {
                ui.label(format!("Are you sure you want to delete project '{}'?", project_name));
                ui.label("This action cannot be undone.");
                
                ui.add_space(15.0);
                
                ui.horizontal(|ui| {
                    if ui.button("🗑️ Delete").clicked() {
                        match manager_state.delete_project(project_id) {
                            Ok(()) => {
                                logger.log_info(&format!("Deleted project: {}", project_name));
                                manager_state.show_delete_confirmation = None;
                            }
                            Err(e) => {
                                manager_state.last_error = Some(format!("Failed to delete project: {}", e));
                                manager_state.show_delete_confirmation = None;
                            }
                        }
                    }
                    
                    if ui.button("Cancel").clicked() {
                        manager_state.show_delete_confirmation = None;
                    }
                });
            });
        });
}

/// Display the hierarchical tree structure of a KiCad project
fn show_hierarchy_tree(ui: &mut egui::Ui, hierarchy: &crate::project_manager::kicad_hierarchy::ProjectHierarchy) {
    ui.vertical(|ui| {
        // Show root schematic
        if let Some(ref root_sch) = hierarchy.root_schematic {
            let file_name = root_sch.file_name().and_then(|f| f.to_str()).unwrap_or("Unknown");
            ui.label(format!("📄 Top Level Schematic: {}", file_name));
        }

        // Show hierarchical sheets
        if !hierarchy.sheets.is_empty() {
            ui.indent("sheets_indent", |ui| {
                for sheet in &hierarchy.sheets {
                    show_sheet_tree(ui, sheet);
                }
            });
        }

        // Show PCB file
        if let Some(ref pcb) = hierarchy.pcb_file {
            let file_name = pcb.file_name().and_then(|f| f.to_str()).unwrap_or("Unknown");
            ui.label(format!("🔧 PCB File: {}", file_name));
        }
    });
}

/// Recursively display a hierarchical sheet and its sub-sheets
fn show_sheet_tree(ui: &mut egui::Ui, sheet: &crate::project_manager::kicad_hierarchy::HierarchicalSheet) {
    ui.label(format!("├─ 📋 {}", sheet.name));

    if !sheet.sub_sheets.is_empty() {
        ui.indent(format!("sub_sheets_{}", sheet.name), |ui| {
            for sub_sheet in &sheet.sub_sheets {
                show_sheet_tree(ui, sub_sheet);
            }
        });
    }
}