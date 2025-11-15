use crate::DemoLensApp;
use crate::project_manager::ProjectManagerState;
use crate::project_manager::database::ProjectMetadata;
use egui_lens::{ReactiveEventLogger, ReactiveEventLoggerState, LogColors};
use egui_mobius_reactive::Dynamic;
use std::collections::HashMap;
use egui_ltreeview::{TreeView, Action};

/// Show the projects database panel with tree view layout
pub fn show_projects_panel<'a>(
    ui: &mut egui::Ui,
    app: &'a mut DemoLensApp,
    logger_state: &'a Dynamic<ReactiveEventLoggerState>,
    log_colors: &'a Dynamic<LogColors>,
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

    if let Some(ref mut manager_state) = app.project_manager_state {
        // Handle any errors
        if let Some(error) = manager_state.last_error.take() {
            logger.log_error(&error);
        }

        ui.heading("📁 Project Database");
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

            // Current project info
            let current_project_name = manager_state.current_project
                .as_ref()
                .map(|p| p.metadata.name.clone());

            if let Some(ref project_name) = current_project_name {
                ui.label(format!("📋 Current: {}", project_name));

                // Save BOM to current project
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
                ui.label("📋 No project loaded");
            }
        });

        ui.separator();

        // Project tree view
        if manager_state.project_list.is_empty() {
            ui.vertical_centered(|ui| {
                ui.add_space(50.0);
                ui.label("No projects found. Create your first project in the Project tab!");
                ui.add_space(10.0);
                ui.label("💡 Use the Project tab to create new KiCad projects");
            });
        } else {
            // Build tree structure
            let tree_structure = build_tree_structure(&manager_state.project_list);
            let projects_by_id: HashMap<String, &ProjectMetadata> = manager_state.project_list
                .iter()
                .map(|p| (p.id.clone(), p))
                .collect();

            let current_project_id = manager_state.current_project
                .as_ref()
                .map(|p| p.metadata.id.clone());

            // Two-column layout: tree view on left, details on right
            ui.columns(2, |columns| {
                // Left column: Tree view
                columns[0].vertical(|ui| {
                    ui.heading("Projects");
                    ui.separator();

                    egui::ScrollArea::vertical()
                        .auto_shrink([false, false])
                        .show(ui, |ui| {
                            let (_response, actions) = TreeView::new(ui.make_persistent_id("projects_tree"))
                                .show(ui, |builder| {
                                    // Get root projects
                                    let root_projects: Vec<_> = manager_state.project_list
                                        .iter()
                                        .filter(|p| p.parent_id.is_none())
                                        .collect();

                                    for project in root_projects {
                                        show_tree_node_builder(
                                            builder,
                                            project,
                                            &tree_structure,
                                            &projects_by_id,
                                            &current_project_id,
                                        );
                                    }
                                });

                            // Handle tree view actions
                            for action in actions {
                                match action {
                                    Action::SetSelected(selected_ids) => {
                                        if let Some(first_id) = selected_ids.first() {
                                            // Set selected project (don't load yet)
                                            manager_state.selected_project_id = Some(first_id.clone());
                                        }
                                    }
                                    _ => {}
                                }
                            }
                        });
                });

                // Right column: Project details
                columns[1].vertical(|ui| {
                    ui.heading("Project Details");
                    ui.separator();

                    if let Some(ref selected_id) = manager_state.selected_project_id {
                        if let Some(project) = manager_state.project_list.iter().find(|p| &p.id == selected_id) {
                            // Try to read metadata from .kicad_pro file
                            let kicad_metadata = crate::project_manager::kicad_metadata::get_kicad_pro_path(&project.pcb_file_path)
                                .and_then(|pro_path| {
                                    if pro_path.exists() {
                                        crate::project_manager::kicad_metadata::read_kicad_metadata(&pro_path).ok()
                                    } else {
                                        None
                                    }
                                });

                            egui::ScrollArea::vertical()
                                .auto_shrink([false, false])
                                .show(ui, |ui| {
                                    // Project metadata
                                    // Editable fields stored in memory
                                    let name_id = egui::Id::new(format!("edit_name_{}", selected_id));
                                    let tags_id = egui::Id::new(format!("edit_tags_{}", selected_id));

                                    let mut temp_name = ui.ctx().memory(|mem| {
                                        mem.data.get_temp::<String>(name_id)
                                            .unwrap_or_else(|| project.name.clone())
                                    });

                                    let mut temp_tags = ui.ctx().memory(|mem| {
                                        mem.data.get_temp::<String>(tags_id)
                                            .unwrap_or_else(|| project.tags.join(", "))
                                    });

                                    egui::Grid::new("project_details_grid")
                                        .num_columns(2)
                                        .spacing([10.0, 5.0])
                                        .striped(true)
                                        .show(ui, |ui| {
                                            ui.label(egui::RichText::new("Name:").strong());
                                            ui.text_edit_singleline(&mut temp_name);
                                            ui.end_row();

                                            // Show Author from .kicad_pro if available
                                            if let Some(ref metadata) = kicad_metadata {
                                                if let Some(ref author) = metadata.author {
                                                    ui.label(egui::RichText::new("Author:").strong());
                                                    ui.label(author);
                                                    ui.end_row();
                                                }

                                                if let Some(ref company) = metadata.company {
                                                    ui.label(egui::RichText::new("Company:").strong());
                                                    ui.label(company);
                                                    ui.end_row();
                                                }
                                            }

                                            ui.label(egui::RichText::new("Created:").strong());
                                            ui.label(project.created_at.format("%Y-%m-%d %H:%M").to_string());
                                            ui.end_row();

                                            ui.label(egui::RichText::new("Last Modified:").strong());
                                            ui.label(project.last_modified.format("%Y-%m-%d %H:%M").to_string());
                                            ui.end_row();

                                            // Show date from .kicad_pro if available
                                            if let Some(ref metadata) = kicad_metadata {
                                                if let Some(ref date) = metadata.date {
                                                    ui.label(egui::RichText::new("Project Date:").strong());
                                                    ui.label(date);
                                                    ui.end_row();
                                                }
                                            }

                                            ui.label(egui::RichText::new("Tags:").strong());
                                            ui.text_edit_singleline(&mut temp_tags);
                                            ui.end_row();
                                        });

                                    // Store edited values back to memory
                                    ui.ctx().memory_mut(|mem| {
                                        mem.data.insert_temp(name_id, temp_name.clone());
                                        mem.data.insert_temp(tags_id, temp_tags.clone());
                                    });

                                    ui.add_space(15.0);

                                    // Description section
                                    ui.separator();
                                    ui.label(egui::RichText::new("Description").strong().size(14.0));
                                    ui.add_space(5.0);

                                    // Get available height for description area (fill remaining space)
                                    let available_height = ui.available_height() - 80.0; // Leave room for buttons

                                    egui::Frame::NONE
                                        .fill(ui.visuals().extreme_bg_color)
                                        .stroke(ui.visuals().widgets.noninteractive.bg_stroke)
                                        .inner_margin(egui::Margin::same(10))
                                        .show(ui, |ui| {
                                            ui.set_height(available_height.max(200.0));

                                            // Use description from .kicad_pro if available, otherwise from database
                                            let source_description = kicad_metadata.as_ref()
                                                .and_then(|m| m.description.clone())
                                                .unwrap_or_else(|| project.description.clone());

                                            // Get or initialize the description buffer for this project
                                            let description_id = egui::Id::new(format!("description_buffer_{}", selected_id));
                                            let init_id = egui::Id::new(format!("description_initialized_{}", selected_id));

                                            let mut temp_description = ui.ctx().memory_mut(|mem| {
                                                // Check if we've initialized this project's description
                                                let initialized = mem.data.get_temp::<bool>(init_id).unwrap_or(false);

                                                if !initialized {
                                                    // First time - initialize from source
                                                    mem.data.insert_temp(init_id, true);
                                                    mem.data.insert_temp(description_id, source_description.clone());
                                                    source_description.clone()
                                                } else {
                                                    // Already initialized - use stored value
                                                    mem.data.get_temp::<String>(description_id)
                                                        .unwrap_or_else(|| source_description.clone())
                                                }
                                            });

                                            // Create terminal-style text area (like Vescript FPGA Manager)
                                            let text_edit_id = egui::Id::new(format!("description_edit_{}", selected_id));

                                            let output = egui::TextEdit::multiline(&mut temp_description)
                                                .id(text_edit_id)
                                                .text_color(egui::Color32::GREEN)
                                                .font(egui::TextStyle::Monospace)
                                                .interactive(true)
                                                .desired_rows(15)
                                                .desired_width(f32::INFINITY)
                                                .hint_text("Enter project description...")
                                                .frame(false)  // Remove the frame/border
                                                .show(ui);

                                            // Store the current text in memory
                                            ui.ctx().memory_mut(|mem| {
                                                mem.data.insert_temp(description_id, temp_description.clone());
                                            });

                                            let response = output.response;

                                            // Only save on focus lost (not on every keystroke)
                                            if response.lost_focus() && temp_description != source_description {
                                                ui.ctx().memory_mut(|mem| {
                                                    mem.data.insert_temp(
                                                        egui::Id::new("edit_description"),
                                                        (selected_id.clone(), temp_description.clone())
                                                    );
                                                });
                                            }
                                        });

                                    ui.add_space(15.0);

                                    // Action buttons
                                    ui.horizontal(|ui| {
                                        if ui.button("📂 Load Project").clicked() {
                                            ui.ctx().memory_mut(|mem| {
                                                mem.data.insert_temp(egui::Id::new("load_project"), selected_id.clone());
                                            });
                                        }

                                        if ui.button("💾 Save Project").clicked() {
                                            ui.ctx().memory_mut(|mem| {
                                                mem.data.insert_temp(egui::Id::new("save_project"), (selected_id.clone(), temp_name.clone(), temp_tags.clone()));
                                            });
                                        }

                                        if ui.button("🗑️ Delete Project").clicked() {
                                            manager_state.show_delete_confirmation = Some(selected_id.clone());
                                        }
                                    });
                                });
                        }
                    } else {
                        ui.vertical_centered(|ui| {
                            ui.add_space(50.0);
                            ui.label("Select a project to view details");
                        });
                    }
                });
            });
        }

        // Handle save project action (name, tags, description)
        let save_info = ui.ctx().memory(|mem| {
            mem.data.get_temp::<(String, String, String)>(egui::Id::new("save_project"))
        });

        if let Some((project_id, new_name, new_tags_str)) = save_info {
            ui.ctx().memory_mut(|mem| {
                mem.data.remove::<(String, String, String)>(egui::Id::new("save_project"));
            });

            // Parse tags
            let new_tags: Vec<String> = new_tags_str
                .split(',')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect();

            // Get the current description from memory
            let description_id = egui::Id::new(format!("description_{}", project_id));
            let new_description = ui.ctx().memory(|mem| {
                mem.data.get_temp::<String>(description_id)
                    .unwrap_or_else(|| {
                        manager_state.project_list.iter()
                            .find(|p| p.id == project_id)
                            .map(|p| p.description.clone())
                            .unwrap_or_default()
                    })
            });

            if let Err(e) = manager_state.update_project(&project_id, new_name.clone(), new_description.clone(), new_tags.clone()) {
                manager_state.last_error = Some(format!("Failed to save project: {}", e));
            } else {
                logger.log_info(&format!("Saved project: {}", new_name));

                // Try to update the .kicad_pro file if it exists
                if let Some(project) = manager_state.project_list.iter().find(|p| p.id == project_id) {
                    let kicad_pro_path = crate::project_manager::kicad_metadata::get_kicad_pro_path(&project.pcb_file_path);
                    if let Some(pro_path) = kicad_pro_path {
                        if pro_path.exists() {
                            // Try to update the description in the .kicad_pro file
                            if let Err(e) = update_kicad_description(&pro_path, &new_description) {
                                logger.log_warning(&format!("Could not update .kicad_pro description: {}", e));
                            } else {
                                logger.log_info("Updated description in .kicad_pro file");
                            }
                        }
                    }
                }
            }
        }

        // Handle description edit action (legacy - now handled by save_project)
        let edit_info = ui.ctx().memory(|mem| {
            mem.data.get_temp::<(String, String)>(egui::Id::new("edit_description"))
        });

        if let Some((project_id, new_description)) = edit_info {
            ui.ctx().memory_mut(|mem| {
                mem.data.remove::<(String, String)>(egui::Id::new("edit_description"));
            });

            // Update the project description in the database
            if let Some(project) = manager_state.project_list.iter().find(|p| p.id == project_id) {
                let tags = project.tags.clone();
                let name = project.name.clone();
                let pcb_file_path = project.pcb_file_path.clone();

                if let Err(e) = manager_state.update_project(&project_id, name.clone(), new_description.clone(), tags) {
                    manager_state.last_error = Some(format!("Failed to update description: {}", e));
                } else {
                    logger.log_info(&format!("Updated description for project: {}", name));

                    // Also try to update the .kicad_pro file if it exists
                    let kicad_pro_path = crate::project_manager::kicad_metadata::get_kicad_pro_path(&pcb_file_path);
                    if let Some(pro_path) = kicad_pro_path {
                        if pro_path.exists() {
                            // Try to update the description in the .kicad_pro file
                            if let Err(e) = update_kicad_description(&pro_path, &new_description) {
                                logger.log_warning(&format!("Could not update .kicad_pro description: {}", e));
                            } else {
                                logger.log_info("Updated description in .kicad_pro file");
                            }
                        }
                    }
                }
            }
        }

        // Handle delete confirmation
        if let Some(ref project_id) = manager_state.show_delete_confirmation {
            let project_id_clone = project_id.clone();

            // Get project name before deletion
            let project_name = manager_state.project_list
                .iter()
                .find(|p| p.id == project_id_clone)
                .map(|p| p.name.clone())
                .unwrap_or_else(|| "Unknown".to_string());

            egui::Window::new("Delete Project")
                .collapsible(false)
                .resizable(false)
                .show(ui.ctx(), |ui| {
                    ui.label("Are you sure you want to delete this project?");
                    ui.label(egui::RichText::new("This action cannot be undone!").color(egui::Color32::RED));

                    ui.horizontal(|ui| {
                        if ui.button("✅ Yes, Delete").clicked() {
                            if let Err(e) = manager_state.delete_project(&project_id_clone) {
                                manager_state.last_error = Some(format!("Failed to delete project: {}", e));
                            } else {
                                logger.log_info(&format!("Project '{}' deleted successfully", project_name));
                            }
                            manager_state.show_delete_confirmation = None;
                        }

                        if ui.button("❌ Cancel").clicked() {
                            manager_state.show_delete_confirmation = None;
                        }
                    });
                });
        }

        // Handle project loading action
        let load_project_id = ui.ctx().memory(|mem| {
            mem.data.get_temp::<String>(egui::Id::new("load_project"))
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
                // Successfully loaded project data, now restore the project state
                if let Some(ref project) = manager_state.current_project {
                    // 1. Set the PCB file path in the project manager
                    app.project_manager.state = crate::project::ProjectState::PcbSelected {
                        pcb_path: project.metadata.pcb_file_path.clone()
                    };

                    // 2. Restore BOM components if available
                    if !project.bom_components.is_empty() {
                        if let Some(ref mut bom_state) = app.bom_state {
                            let mut components = bom_state.components.lock().unwrap();
                            *components = project.bom_components.clone();
                            logger.log_info(&format!("Restored {} BOM components", project.bom_components.len()));
                        } else {
                            app.pending_bom_components = Some(project.bom_components.clone());
                            logger.log_info(&format!("BOM state not initialized yet. {} components stored and will be loaded when BOM tab is opened.", project.bom_components.len()));
                        }
                    }

                    // 3. Log PCB file status
                    if project.metadata.pcb_file_path.exists() {
                        logger.log_info(&format!("PCB file found at: {}. Click 'Generate Gerbers' in PCB File tab to load gerbers.", project.metadata.pcb_file_path.display()));
                    } else {
                        logger.log_warning(&format!("PCB file not found at: {}", project.metadata.pcb_file_path.display()));
                    }

                    logger.log_info(&format!("✅ Loaded project: {}", project_name));
                }
            }
        }
    }
}

/// Helper function to update description in .kicad_pro file
fn update_kicad_description(kicad_pro_path: &std::path::Path, new_description: &str) -> Result<(), Box<dyn std::error::Error>> {
    use serde_json::Value;

    // Read the existing .kicad_pro file
    let content = std::fs::read_to_string(kicad_pro_path)?;
    let mut project: Value = serde_json::from_str(&content)?;

    // Update the DESCRIPTION field in text_variables
    if let Some(text_vars) = project.get_mut("text_variables") {
        if let Some(obj) = text_vars.as_object_mut() {
            obj.insert("DESCRIPTION".to_string(), Value::String(new_description.to_string()));
        }
    } else {
        // Create text_variables if it doesn't exist
        let mut text_vars = serde_json::Map::new();
        text_vars.insert("DESCRIPTION".to_string(), Value::String(new_description.to_string()));
        project["text_variables"] = Value::Object(text_vars);
    }

    // Write back to file with pretty formatting
    let updated_content = serde_json::to_string_pretty(&project)?;
    std::fs::write(kicad_pro_path, updated_content)?;

    Ok(())
}

/// Tree node structure for organizing projects hierarchically
struct TreeNode {
    children: Vec<String>,
}

/// Build tree structure from flat project list
fn build_tree_structure(projects: &[ProjectMetadata]) -> HashMap<String, TreeNode> {
    let mut tree: HashMap<String, TreeNode> = HashMap::new();

    // Initialize all nodes
    for project in projects {
        tree.insert(project.id.clone(), TreeNode {
            children: Vec::new(),
        });
    }

    // Build parent-child relationships
    for project in projects {
        if let Some(ref parent_id) = project.parent_id {
            if let Some(parent_node) = tree.get_mut(parent_id) {
                parent_node.children.push(project.id.clone());
            }
        }
    }

    tree
}

/// Recursively show tree nodes using builder pattern
fn show_tree_node_builder(
    builder: &mut egui_ltreeview::TreeViewBuilder<String>,
    project: &ProjectMetadata,
    tree_structure: &HashMap<String, TreeNode>,
    projects_by_id: &HashMap<String, &ProjectMetadata>,
    current_project_id: &Option<String>,
) {
    let is_current = current_project_id
        .as_ref()
        .map(|id| id == &project.id)
        .unwrap_or(false);

    let label = if is_current {
        egui::RichText::new(&project.name).strong().color(egui::Color32::LIGHT_BLUE)
    } else {
        egui::RichText::new(&project.name)
    };

    let has_children = tree_structure
        .get(&project.id)
        .map(|node| !node.children.is_empty())
        .unwrap_or(false);

    if has_children {
        // Parent node with children
        builder.dir(project.id.clone(), label);

        // Add children
        if let Some(node) = tree_structure.get(&project.id) {
            for child_id in &node.children {
                if let Some(child_project) = projects_by_id.get(child_id) {
                    show_tree_node_builder(
                        builder,
                        child_project,
                        tree_structure,
                        projects_by_id,
                        current_project_id,
                    );
                }
            }
        }

        builder.close_dir();
    } else {
        // Leaf node
        builder.leaf(project.id.clone(), label);
    }
}
