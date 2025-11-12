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
                            egui::ScrollArea::vertical()
                                .auto_shrink([false, false])
                                .show(ui, |ui| {
                                    // Project metadata
                                    egui::Grid::new("project_details_grid")
                                        .num_columns(2)
                                        .spacing([10.0, 5.0])
                                        .striped(true)
                                        .show(ui, |ui| {
                                            ui.label(egui::RichText::new("Name:").strong());
                                            ui.label(&project.name);
                                            ui.end_row();

                                            ui.label(egui::RichText::new("Description:").strong());
                                            ui.label(&project.description);
                                            ui.end_row();

                                            ui.label(egui::RichText::new("Created:").strong());
                                            ui.label(project.created_at.format("%Y-%m-%d %H:%M").to_string());
                                            ui.end_row();

                                            ui.label(egui::RichText::new("Last Modified:").strong());
                                            ui.label(project.last_modified.format("%Y-%m-%d %H:%M").to_string());
                                            ui.end_row();

                                            ui.label(egui::RichText::new("Version:").strong());
                                            ui.label(&project.version);
                                            ui.end_row();

                                            ui.label(egui::RichText::new("Tags:").strong());
                                            ui.label(project.tags.join(", "));
                                            ui.end_row();
                                        });

                                    ui.add_space(10.0);

                                    // Action buttons
                                    ui.horizontal(|ui| {
                                        if ui.button("📂 Load Project").clicked() {
                                            ui.ctx().memory_mut(|mem| {
                                                mem.data.insert_temp(egui::Id::new("load_project"), selected_id.clone());
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
