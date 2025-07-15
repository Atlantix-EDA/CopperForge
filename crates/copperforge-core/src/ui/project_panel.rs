use crate::DemoLensApp;
use crate::project::ProjectState;
use crate::project_manager::ProjectManagerState;
use egui_lens::{ReactiveEventLogger, ReactiveEventLoggerState, LogColors};
use egui_mobius_reactive::Dynamic;
use std::path::{Path, PathBuf};
use std::process::Command;

pub fn show_project_panel<'a>(
    ui: &mut egui::Ui,
    app: &'a mut DemoLensApp,
    logger_state: &'a Dynamic<ReactiveEventLoggerState>,
    log_colors: &'a Dynamic<LogColors>,
) {
    let logger = ReactiveEventLogger::with_colors(logger_state, log_colors);

    ui.heading("Project");
    ui.separator();

    // Show current state
    ui.group(|ui| {
        ui.label("Current State:");
        let state_text = match &app.project_manager.state {
            ProjectState::NoProject => "No project loaded",
            ProjectState::PcbSelected { .. } => "PCB file selected",
            ProjectState::GeneratingGerbers { .. } => "Generating gerbers...",
            ProjectState::GerbersGenerated { .. } => "Gerbers generated",
            ProjectState::LoadingGerbers { .. } => "Loading gerbers...",
            ProjectState::Ready { .. } => "Project ready",
        };
        ui.monospace(state_text);
    });

    ui.add_space(10.0);

    // Project Database Section - only if visible
    let show_database = ui.ctx().memory(|mem| 
        mem.data.get_temp::<bool>(egui::Id::new("show_project_database")).unwrap_or(true)
    );
    
    if ui.button(if show_database { "▼ Project Database" } else { "▶ Project Database" }).clicked() {
        ui.ctx().memory_mut(|mem| {
            mem.data.insert_temp(egui::Id::new("show_project_database"), !show_database);
        });
    }
    
    if show_database {
        show_project_database_section(ui, app, &logger);
    }

    ui.add_space(10.0);

    // Auto-generation settings
    ui.horizontal(|ui| {
        ui.checkbox(&mut app.project_manager.auto_generate_on_startup, "Auto-generate on startup");
    });
    ui.horizontal(|ui| {
        ui.checkbox(&mut app.project_manager.auto_reload_on_change, "Auto-reload on file change");
    });

    ui.add_space(10.0);

    ui.horizontal(|ui| {
        ui.label("KiCad PCB File:");
        
        // Add clear button to reset state
        if ui.small_button("Clear").clicked() {
            app.project_manager.state = ProjectState::NoProject;
            // Also clear current project in database state
            if let Some(ref mut manager_state) = app.project_manager_state {
                manager_state.current_project = None;
                manager_state.selected_project_id = None;
            }
            logger.log_info("Cleared PCB file selection and current project");
        }
    });
    ui.add_space(5.0);

    // Get current PCB path from state
    let current_pcb_path = match &app.project_manager.state {
        ProjectState::NoProject => None,
        ProjectState::PcbSelected { pcb_path } |
        ProjectState::GeneratingGerbers { pcb_path } |
        ProjectState::GerbersGenerated { pcb_path, .. } |
        ProjectState::LoadingGerbers { pcb_path, .. } |
        ProjectState::Ready { pcb_path, .. } => Some(pcb_path.clone()),
    };

    // Text input field for PCB file path
    ui.horizontal(|ui| {
        let mut path_str = current_pcb_path
            .as_ref()
            .map(|p| p.display().to_string())
            .unwrap_or_default();
        
        let response = ui.add(
            egui::TextEdit::singleline(&mut path_str)
                .desired_width(ui.available_width() - 80.0)
                .hint_text("Enter path to .kicad_pcb file...")
        );
        
        // Update path if user edited the text
        if response.changed() {
            if path_str.is_empty() {
                app.project_manager.state = ProjectState::NoProject;
            } else {
                let path = PathBuf::from(&path_str);
                if path.extension().and_then(|s| s.to_str()) == Some("kicad_pcb") {
                    app.project_manager.state = ProjectState::PcbSelected { pcb_path: path.clone() };
                }
            }
        }

        if ui.button("Browse...").clicked() {
            app.project_manager.open_file_dialog();
        }
    });

    // Update file dialog and handle selection
    if let Some(path_buf) = app.project_manager.update_file_dialog(ui.ctx()) {
        app.project_manager.state = ProjectState::PcbSelected { pcb_path: path_buf.clone() };
        logger.log_info(&format!("Selected PCB file: {}", path_buf.display()));
    }

    ui.add_space(10.0);

    // Show appropriate controls based on current state
    match &app.project_manager.state.clone() {
        ProjectState::NoProject => {
            ui.label("No PCB file selected");
        },
        ProjectState::PcbSelected { pcb_path } => {
            show_pcb_info(ui, pcb_path);
            ui.add_space(10.0);
            
            if ui.button("Generate Gerbers").clicked() {
                app.project_manager.state = ProjectState::GeneratingGerbers { pcb_path: pcb_path.clone() };
                logger.log_info("Generating gerbers from PCB file...");
            }
        },
        ProjectState::GeneratingGerbers { pcb_path } => {
            show_pcb_info(ui, pcb_path);
            ui.add_space(10.0);
            
            ui.add_enabled(false, egui::Button::new("Generating..."));
            
            // Handle generation
            if matches!(app.project_manager.state, ProjectState::GeneratingGerbers { .. }) {
                if let Some(output_dir) = generate_gerbers_from_pcb(pcb_path, &logger) {
                    app.project_manager.state = ProjectState::GerbersGenerated {
                        pcb_path: pcb_path.clone(),
                        gerber_dir: output_dir.clone(),
                    };
                } else {
                    // Generation failed, go back to selected state
                    app.project_manager.state = ProjectState::PcbSelected { pcb_path: pcb_path.clone() };
                }
            }
        },
        ProjectState::GerbersGenerated { pcb_path, gerber_dir } => {
            show_pcb_info(ui, pcb_path);
            ui.add_space(10.0);
            
            ui.label(format!("Gerbers in: {}", gerber_dir.display()));
            ui.add_space(5.0);
            
            if ui.button("Load Gerbers into Viewer").clicked() {
                app.project_manager.state = ProjectState::LoadingGerbers {
                    pcb_path: pcb_path.clone(),
                    gerber_dir: gerber_dir.clone(),
                };
                logger.log_info("Loading gerbers into viewer...");
            }
            
            if ui.button("Regenerate Gerbers").clicked() {
                app.project_manager.state = ProjectState::GeneratingGerbers { pcb_path: pcb_path.clone() };
                            }
        },
        ProjectState::LoadingGerbers { pcb_path, gerber_dir } => {
            show_pcb_info(ui, pcb_path);
            ui.add_space(10.0);
            
            ui.add_enabled(false, egui::Button::new("Loading..."));
            
            // Handle loading
            if matches!(app.project_manager.state, ProjectState::LoadingGerbers { .. }) {
                load_gerbers_into_viewer(app, gerber_dir, &logger);
                let last_modified = std::fs::metadata(pcb_path)
                    .and_then(|m| m.modified())
                    .unwrap_or(std::time::SystemTime::now());
                    
                app.project_manager.state = ProjectState::Ready {
                    pcb_path: pcb_path.clone(),
                    gerber_dir: gerber_dir.clone(),
                    last_modified,
                };
            }
        },
        ProjectState::Ready { pcb_path, gerber_dir, last_modified } => {
            show_pcb_info(ui, pcb_path);
            ui.add_space(10.0);
            
            ui.label("✓ Gerbers loaded and displayed");
            
            // Check if file has been modified
            if let Ok(metadata) = std::fs::metadata(pcb_path) {
                if let Ok(modified) = metadata.modified() {
                    if &modified != last_modified {
                        ui.colored_label(egui::Color32::YELLOW, "⚠ PCB file has been modified");
                    }
                }
            }
            
            ui.add_space(5.0);
            
            if ui.button("Reload Gerbers").clicked() {
                app.project_manager.state = ProjectState::LoadingGerbers {
                    pcb_path: pcb_path.clone(),
                    gerber_dir: gerber_dir.clone(),
                };
                            }
            
            if ui.button("Regenerate Gerbers").clicked() {
                app.project_manager.state = ProjectState::GeneratingGerbers { pcb_path: pcb_path.clone() };
                            }
        },
    }
}

fn show_pcb_info(ui: &mut egui::Ui, pcb_path: &Path) {
    ui.group(|ui| {
        ui.horizontal(|ui| {
            ui.label("File:");
            ui.monospace(pcb_path.file_name()
                .unwrap_or_default()
                .to_string_lossy());
        });
        
        if pcb_path.exists() {
            ui.label("✓ File exists");
        } else {
            ui.colored_label(egui::Color32::RED, "✗ File not found");
        }
    });
}

fn generate_gerbers_from_pcb(pcb_path: &Path, logger: &ReactiveEventLogger) -> Option<PathBuf> {
    // Create output directory in the same location as the PCB file
    let output_dir = pcb_path.parent()
        .unwrap_or(Path::new("."))
        .join("gerber_output");
    
    // Create directory if it doesn't exist
    if let Err(e) = std::fs::create_dir_all(&output_dir) {
        logger.log_error(&format!("Failed to create output directory: {}", e));
        return None;
    }

    logger.log_info(&format!("Output directory: {}", output_dir.display()));

    // Run KiCad CLI to generate gerbers
    // Try to find kicad-cli in PATH first, then fall back to known locations
    let kicad_cli_path = if let Ok(output) = Command::new("which").arg("kicad-cli").output() {
        if output.status.success() {
            String::from_utf8_lossy(&output.stdout).trim().to_string()
        } else {
            // Check common installation paths
            let paths = [
                "/usr/lib/kicad-nightly/bin/kicad-cli",
                "/usr/lib/kicad/bin/kicad-cli",
                "/usr/local/bin/kicad-cli",
                "/opt/kicad/bin/kicad-cli",
            ];
            
            paths.iter()
                .find(|p| std::path::Path::new(p).exists())
                .map(|s| s.to_string())
                .unwrap_or_else(|| "kicad-cli".to_string())
        }
    } else {
        "/usr/lib/kicad-nightly/bin/kicad-cli".to_string()
    };
    
    logger.log_info(&format!("Using KiCad CLI at: {}", kicad_cli_path));
    
    // Set up environment for KiCad libraries
    let mut cmd = Command::new(&kicad_cli_path);
    
    // Add library path for KiCad nightly if needed
    if kicad_cli_path.contains("kicad-nightly") {
        let lib_path = "/usr/lib/kicad-nightly/lib/x86_64-linux-gnu";
        let current_ld_path = std::env::var("LD_LIBRARY_PATH").unwrap_or_default();
        let new_ld_path = if current_ld_path.is_empty() {
            lib_path.to_string()
        } else {
            format!("{}:{}", lib_path, current_ld_path)
        };
        cmd.env("LD_LIBRARY_PATH", new_ld_path);
        logger.log_info(&format!("Set LD_LIBRARY_PATH for KiCad nightly: {}", lib_path));
    }
    
    let output = cmd
        .arg("pcb")
        .arg("export")
        .arg("gerbers")
        .arg("--output")
        .arg(&output_dir)
        .arg("--layers")
        .arg("F.Cu,B.Cu,F.SilkS,B.SilkS,F.Mask,B.Mask,Edge.Cuts,F.Paste,B.Paste")
        .arg("--no-protel-ext")  // Use .gbr extension
        .arg(pcb_path)
        .output();

    match output {
        Ok(result) => {
            if result.status.success() {
                logger.log_info("Gerbers generated successfully!");
                logger.log_info(&format!("Check output directory: {}", output_dir.display()));
                
                // Log the generated files
                if let Ok(entries) = std::fs::read_dir(&output_dir) {
                    for entry in entries.flatten() {
                        if let Some(ext) = entry.path().extension() {
                            if ext == "gbr" {
                                logger.log_info(&format!("  Generated: {}", entry.file_name().to_string_lossy()));
                            }
                        }
                    }
                }
                return Some(output_dir);
            } else {
                logger.log_error("Failed to generate gerbers");
                if let Ok(stderr) = String::from_utf8(result.stderr) {
                    logger.log_error(&format!("Error: {}", stderr));
                }
            }
        }
        Err(e) => {
            logger.log_error(&format!("Failed to run kicad-cli: {}", e));
            logger.log_error("Make sure KiCad is installed and kicad-cli is in your PATH");
        }
    }
    None
}

fn load_gerbers_into_viewer(app: &mut DemoLensApp, gerber_dir: &Path, logger: &ReactiveEventLogger) {
    // Clear all existing layers and unassigned gerbers first
    logger.log_info("Clearing existing gerber layers...");
    crate::ecs::clear_all_layers_system(&mut app.ecs_world);
    
    // Use ECS system for bulk gerber loading
    match crate::ecs::load_gerbers_from_directory_system(&mut app.ecs_world, gerber_dir) {
        Ok((loaded_count, unassigned_count)) => {
            // Log results from ECS system
            if loaded_count > 0 {
                logger.log_info(&format!("Successfully loaded {} gerber layers", loaded_count));
            }
            if unassigned_count > 0 {
                logger.log_warning(&format!("{} gerber files could not be automatically assigned", unassigned_count));
            }
            
            // Set loading status for UI
            if loaded_count > 0 {
                app.needs_initial_view = true; // Trigger view reset
            } else if unassigned_count > 0 {
                logger.log_warning(&format!("No layers were automatically detected. {} gerber files need manual assignment.", unassigned_count));
            } else {
                logger.log_error("No gerber files were found");
            }
        }
        Err(e) => {
            logger.log_error(&format!("Failed to load gerbers: {}", e));
        }
    }
}

fn show_project_database_section(ui: &mut egui::Ui, app: &mut DemoLensApp, logger: &ReactiveEventLogger) {
    ui.group(|ui| {
        ui.label("💾 Project Database");
        ui.separator();
        
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
        
        if let Some(ref mut manager_state) = app.project_manager_state {
            // Handle any errors
            if let Some(error) = manager_state.last_error.take() {
                logger.log_error(&error);
            }
            
            // Top controls
            ui.horizontal(|ui| {
                // Current project info
                let current_project_name = manager_state.current_project
                    .as_ref()
                    .map(|p| p.metadata.name.clone());
                
                if let Some(ref project_name) = current_project_name {
                    ui.vertical(|ui| {
                        ui.label(format!("Current: {}", project_name));
                        
                        // Enterprise feature: Show current project dates
                        if let Some(ref current_project) = manager_state.current_project {
                            ui.horizontal(|ui| {
                                ui.spacing_mut().item_spacing.x = 8.0;
                                
                                let created_date = current_project.metadata.created_at.format("%Y-%m-%d %H:%M UTC").to_string();
                                ui.label(egui::RichText::new(format!("📅 {}", created_date))
                                    .small()
                                    .color(egui::Color32::GRAY));
                                
                                let modified_date = current_project.metadata.last_modified.format("%Y-%m-%d %H:%M UTC").to_string();
                                ui.label(egui::RichText::new(format!("🔄 {}", modified_date))
                                    .small()
                                    .color(egui::Color32::GRAY));
                            });
                        }
                    });
                    
                    // Save BOM button
                    if ui.button("💾 Save BOM").clicked() {
                        if let Some(ref bom_state) = app.bom_state {
                            let components = bom_state.components.lock().unwrap().clone();
                            if let Err(e) = manager_state.update_project_bom(components) {
                                manager_state.last_error = Some(format!("Failed to save BOM: {}", e));
                            } else {
                                logger.log_info(&format!("Saved BOM to project: {}", project_name));
                            }
                        }
                    }
                } else {
                    ui.label("No project loaded");
                }
                
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.button("➕ New").clicked() {
                        // Toggle create mode instead of showing modal
                        manager_state.show_create_dialog = !manager_state.show_create_dialog;
                    }
                });
            });
            
            ui.add_space(5.0);
            
            // Project list (scrollable)
            if !manager_state.project_list.is_empty() {
                ui.label("Projects:");
                
                // Get current project ID without cloning entire list
                let current_project_id = manager_state.current_project
                    .as_ref()
                    .map(|p| p.metadata.id.clone());
                
                // Grouped scrollable area for projects  
                ui.group(|ui| {
                    egui::ScrollArea::vertical()
                        .max_height(120.0)
                        .min_scrolled_height(60.0)
                        .auto_shrink([false, false])
                        .show(ui, |ui| {
                        for project in &manager_state.project_list {
                    ui.horizontal(|ui| {
                        let is_current = current_project_id
                            .as_ref()
                            .map(|id| id == &project.id)
                            .unwrap_or(false);
                        
                        let text = if is_current {
                            egui::RichText::new(&project.name).strong().color(egui::Color32::LIGHT_BLUE)
                        } else {
                            egui::RichText::new(&project.name)
                        };
                        
                        ui.vertical(|ui| {
                            ui.label(text);
                            
                            // Enterprise feature: Show database dates
                            ui.horizontal(|ui| {
                                ui.spacing_mut().item_spacing.x = 8.0;
                                
                                // Created date
                                let created_date = project.created_at.format("%Y-%m-%d %H:%M UTC").to_string();
                                ui.label(egui::RichText::new(format!("📅 Created: {}", created_date))
                                    .small()
                                    .color(egui::Color32::GRAY));
                                
                                // Last modified date
                                let modified_date = project.last_modified.format("%Y-%m-%d %H:%M UTC").to_string();
                                ui.label(egui::RichText::new(format!("🔄 Modified: {}", modified_date))
                                    .small()
                                    .color(egui::Color32::GRAY));
                            });
                        });
                        
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            // Delete button
                            if ui.small_button("🗑️").on_hover_text("Delete this project").clicked() {
                                ui.ctx().memory_mut(|mem| {
                                    mem.data.insert_temp(egui::Id::new("delete_project_id"), project.id.clone());
                                    mem.data.insert_temp(egui::Id::new("delete_project_name"), project.name.clone());
                                });
                            }
                            
                            // Edit button
                            if ui.small_button("✏️").on_hover_text("Edit project details").clicked() {
                                ui.ctx().memory_mut(|mem| {
                                    mem.data.insert_temp(egui::Id::new("edit_project_id"), project.id.clone());
                                });
                            }
                            
                            // Load button
                            if ui.small_button("📂").on_hover_text("Load this project").clicked() {
                                ui.ctx().memory_mut(|mem| {
                                    mem.data.insert_temp(egui::Id::new("load_project_id"), project.id.clone());
                                    mem.data.insert_temp(egui::Id::new("load_project_name"), project.name.clone());
                                });
                            }
                        });
                    });
                        }
                    });
                });
                
                // Handle project actions
                let load_project_id = ui.ctx().memory(|mem| {
                    mem.data.get_temp::<String>(egui::Id::new("load_project_id"))
                });
                let load_project_name = ui.ctx().memory(|mem| {
                    mem.data.get_temp::<String>(egui::Id::new("load_project_name"))
                });
                let delete_project_id = ui.ctx().memory(|mem| {
                    mem.data.get_temp::<String>(egui::Id::new("delete_project_id"))
                });
                let delete_project_name = ui.ctx().memory(|mem| {
                    mem.data.get_temp::<String>(egui::Id::new("delete_project_name"))
                });
                
                // Handle load project
                if let (Some(project_id), Some(project_name)) = (load_project_id, load_project_name) {
                    ui.ctx().memory_mut(|mem| {
                        mem.data.remove::<String>(egui::Id::new("load_project_id"));
                        mem.data.remove::<String>(egui::Id::new("load_project_name"));
                    });
                    
                    if let Err(e) = manager_state.load_project(&project_id) {
                        manager_state.last_error = Some(format!("Failed to load project: {}", e));
                    } else {
                        // Successfully loaded project data, now restore the project state
                        if let Some(ref project) = manager_state.current_project {
                            // 1. Set the PCB file path in the project manager
                            app.project_manager.state = crate::project::ProjectState::PcbSelected { 
                                pcb_path: project.metadata.pcb_file_path.clone() 
                            };
                            
                            // 2. Restore BOM components if available
                            if !project.bom_components.is_empty() {
                                // Note: BOM state initialization happens in show_bom_panel when the BOM tab is first shown
                                // If BOM state exists, restore the components
                                if let Some(ref mut bom_state) = app.bom_state {
                                    let mut components = bom_state.components.lock().unwrap();
                                    *components = project.bom_components.clone();
                                    logger.log_info(&format!("Restored {} BOM components", project.bom_components.len()));
                                } else {
                                    // Store pending BOM components to be loaded when BOM tab is opened
                                    app.pending_bom_components = Some(project.bom_components.clone());
                                    logger.log_info(&format!("BOM state not initialized yet. {} components stored and will be loaded when BOM tab is opened.", project.bom_components.len()));
                                }
                            }
                            
                            // 3. Log PCB file status
                            if project.metadata.pcb_file_path.exists() {
                                logger.log_info(&format!("PCB file found at: {}. Click 'Generate Gerbers' to load gerbers.", project.metadata.pcb_file_path.display()));
                            } else {
                                logger.log_warning(&format!("PCB file not found at: {}", project.metadata.pcb_file_path.display()));
                            }
                            
                            logger.log_info(&format!("✅ Loaded project: {} with PCB file: {}", 
                                project_name, 
                                project.metadata.pcb_file_path.display()
                            ));
                        }
                    }
                }
                
                // Handle delete project
                if let (Some(project_id), Some(project_name)) = (delete_project_id, delete_project_name) {
                    ui.ctx().memory_mut(|mem| {
                        mem.data.remove::<String>(egui::Id::new("delete_project_id"));
                        mem.data.remove::<String>(egui::Id::new("delete_project_name"));
                    });
                    
                    if let Err(e) = manager_state.delete_project(&project_id) {
                        manager_state.last_error = Some(format!("Failed to delete project: {}", e));
                    } else {
                        logger.log_info(&format!("Deleted project: {}", project_name));
                    }
                }
            }
            
            // Inline create project section
            if manager_state.show_create_dialog {
                ui.separator();
                ui.group(|ui| {
                    ui.label("🆕 Create New Project");
                    ui.separator();
                    
                    ui.horizontal(|ui| {
                        ui.label("Name:");
                        ui.text_edit_singleline(&mut manager_state.new_project_name);
                    });
                    
                    ui.horizontal(|ui| {
                        ui.label("Description:");
                        ui.text_edit_singleline(&mut manager_state.new_project_description);
                    });
                    
                    ui.horizontal(|ui| {
                        ui.label("Tags:");
                        ui.text_edit_singleline(&mut manager_state.new_project_tags);
                    });
                    
                    ui.add_space(5.0);
                    
                    ui.horizontal(|ui| {
                        if ui.button("✅ Create").clicked() {
                            if !manager_state.new_project_name.trim().is_empty() {
                                // Get PCB file path
                                if let Some(pcb_path) = match &app.project_manager.state {
                                    crate::project::ProjectState::Ready { pcb_path, .. } |
                                    crate::project::ProjectState::PcbSelected { pcb_path } |
                                    crate::project::ProjectState::GeneratingGerbers { pcb_path } |
                                    crate::project::ProjectState::GerbersGenerated { pcb_path, .. } |
                                    crate::project::ProjectState::LoadingGerbers { pcb_path, .. } => {
                                        Some(pcb_path.clone())
                                    },
                                    _ => None,
                                } {
                                    // Parse tags
                                    let tags: Vec<String> = manager_state.new_project_tags
                                        .split(',')
                                        .map(|s| s.trim().to_string())
                                        .filter(|s| !s.is_empty())
                                        .collect();
                                    
                                    // Get BOM components
                                    let bom_components = if let Some(ref bom_state) = app.bom_state {
                                        bom_state.components.lock().unwrap().clone()
                                    } else {
                                        Vec::new()
                                    };
                                    
                                    // Create project
                                    match manager_state.create_project(
                                        manager_state.new_project_name.clone(),
                                        manager_state.new_project_description.clone(),
                                        pcb_path,
                                        tags,
                                        bom_components,
                                    ) {
                                        Ok(project_id) => {
                                            logger.log_info(&format!("Created project: {} (ID: {})", manager_state.new_project_name, project_id));
                                            manager_state.reset_create_dialog();
                                        }
                                        Err(e) => {
                                            manager_state.last_error = Some(format!("Failed to create project: {}", e));
                                        }
                                    }
                                } else {
                                    manager_state.last_error = Some("Please select a PCB file first".to_string());
                                }
                            } else {
                                manager_state.last_error = Some("Project name cannot be empty".to_string());
                            }
                        }
                        
                        if ui.button("❌ Cancel").clicked() {
                            manager_state.reset_create_dialog();
                        }
                    });
                });
            }
        }
    });
}