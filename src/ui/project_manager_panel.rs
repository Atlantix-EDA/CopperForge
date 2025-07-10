#![allow(dead_code)]
use crate::DemoLensApp;
use crate::project_manager::ProjectManagerState;
use egui_lens::{ReactiveEventLogger, ReactiveEventLoggerState, LogColors};
use egui_mobius_reactive::Dynamic;

/// Show the project manager panel
pub fn show_project_manager_panel(
    ui: &mut egui::Ui,
    app: &mut DemoLensApp,
    logger_state: &Dynamic<ReactiveEventLoggerState>,
    log_colors: &Dynamic<LogColors>,
) {
    let logger = ReactiveEventLogger::with_colors(logger_state, log_colors);
    
    // Initialize project manager state if not already done
    if app.project_manager_state.is_none() {
        let mut state = ProjectManagerState::default();
        
        // Initialize database
        let db_path = app.config_path.join("projects.db");
        if let Err(e) = state.initialize_database(&db_path) {
            logger.log_error(&format!("Failed to initialize project database: {}", e));
        }
        
        app.project_manager_state = Some(state);
    }
    
    // Split app borrow to avoid conflicts
    let bom_components = if let Some(ref bom_state) = app.bom_state {
        Some(bom_state.components.lock().unwrap().clone())
    } else {
        None
    };
    
    let project_state = &app.project_manager.state;
    
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
                            body.row(18.0, |mut row| {
                                // Project name
                                row.col(|ui| {
                                    let is_current = current_project_id
                                        .as_ref()
                                        .map(|id| id == &project.id)
                                        .unwrap_or(false);
                                    
                                    let text = if is_current {
                                        egui::RichText::new(&project.name).strong().color(egui::Color32::LIGHT_BLUE)
                                    } else {
                                        egui::RichText::new(&project.name)
                                    };
                                    
                                    ui.label(text);
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
    project_state: &crate::project::ProjectState,
    bom_components: Vec<crate::ui::bom_panel_v2::BomComponent>,
    logger: &ReactiveEventLogger,
) {
    egui::Window::new("Create New Project")
        .id(egui::Id::new("create_project_dialog"))
        .collapsible(false)
        .resizable(false)
        .movable(true)
        .default_pos(egui::Pos2::new(400.0, 200.0))
        .max_size(egui::Vec2::new(500.0, 400.0))
        .show(ctx, |ui| {
            ui.vertical(|ui| {
                ui.label("Project Name:");
                ui.text_edit_singleline(&mut manager_state.new_project_name);
                
                ui.add_space(5.0);
                
                ui.label("Description:");
                ui.text_edit_multiline(&mut manager_state.new_project_description);
                
                ui.add_space(5.0);
                
                ui.label("Tags (comma-separated):");
                ui.text_edit_singleline(&mut manager_state.new_project_tags);
                
                ui.add_space(10.0);
                
                // PCB file selection
                ui.horizontal(|ui| {
                    ui.label("PCB File:");
                    let pcb_file_text = match project_state {
                        crate::project::ProjectState::Ready { pcb_path, .. } |
                        crate::project::ProjectState::PcbSelected { pcb_path } |
                        crate::project::ProjectState::GeneratingGerbers { pcb_path } |
                        crate::project::ProjectState::GerbersGenerated { pcb_path, .. } |
                        crate::project::ProjectState::LoadingGerbers { pcb_path, .. } => {
                            pcb_path.file_name()
                                .map(|n| n.to_string_lossy().to_string())
                                .unwrap_or_else(|| "Unknown file".to_string())
                        },
                        _ => "No PCB file selected".to_string(),
                    };
                    ui.label(pcb_file_text);
                });
                
                ui.add_space(15.0);
                
                // Buttons
                ui.horizontal(|ui| {
                    if ui.button("Create").clicked() {
                        // Validate input
                        if manager_state.new_project_name.trim().is_empty() {
                            manager_state.last_error = Some("Project name cannot be empty".to_string());
                            return;
                        }
                        
                        // Get PCB file path
                        let pcb_path = match project_state {
                            crate::project::ProjectState::Ready { pcb_path, .. } |
                            crate::project::ProjectState::PcbSelected { pcb_path } |
                            crate::project::ProjectState::GeneratingGerbers { pcb_path } |
                            crate::project::ProjectState::GerbersGenerated { pcb_path, .. } |
                            crate::project::ProjectState::LoadingGerbers { pcb_path, .. } => {
                                pcb_path.clone()
                            },
                            _ => {
                                manager_state.last_error = Some("Please select a PCB file first".to_string());
                                return;
                            }
                        };
                        
                        // Parse tags
                        let tags: Vec<String> = manager_state.new_project_tags
                            .split(',')
                            .map(|s| s.trim().to_string())
                            .filter(|s| !s.is_empty())
                            .collect();
                        
                        // Create project
                        match manager_state.create_project(
                            manager_state.new_project_name.clone(),
                            manager_state.new_project_description.clone(),
                            pcb_path,
                            tags,
                            bom_components.clone(),
                        ) {
                            Ok(project_id) => {
                                logger.log_info(&format!("Created project: {} (ID: {})", manager_state.new_project_name, project_id));
                                manager_state.reset_create_dialog();
                            }
                            Err(e) => {
                                manager_state.last_error = Some(format!("Failed to create project: {}", e));
                            }
                        }
                    }
                    
                    if ui.button("Cancel").clicked() {
                        manager_state.reset_create_dialog();
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