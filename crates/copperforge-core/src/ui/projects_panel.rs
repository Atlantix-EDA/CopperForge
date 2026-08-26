use crate::project_manager::ProjectManagerState;
use crate::project_manager::database::ProjectMetadata;
use crate::event_logger::{ReactiveEventLogger, ReactiveEventLoggerState, LogColors};
use egui_mobius_reactive::Dynamic;
use std::collections::HashMap;
use egui_ltreeview::{TreeView, Action, NodeBuilder};

/// Right-click intents on a project row, deposited into egui memory by the
/// context-menu closure and dispatched after `TreeView::show` returns.
const PROJECT_CONTEXT_INTENT: &str = "project_context_intent";

/// (action, project_id). `action` is one of "open", "update", "delete", "new", "new_child".
/// For "new" the project_id is empty; for "new_child" it is the parent id.
type ProjectIntent = (String, String);

fn set_project_intent(ctx: &egui::Context, action: &str, project_id: &str) {
    ctx.memory_mut(|mem| {
        mem.data.insert_temp(
            egui::Id::new(PROJECT_CONTEXT_INTENT),
            (action.to_string(), project_id.to_string()),
        );
    });
}

/// Show the projects database panel with tree view layout
pub fn show_projects_panel(
    ui: &mut egui::Ui,
    projects: &mut crate::app::ProjectsPanelState,
    services: &mut crate::services::SharedServices,
    logger_state: &Dynamic<ReactiveEventLoggerState>,
    log_colors: &Dynamic<LogColors>,
) {
    let logger = ReactiveEventLogger::with_colors(logger_state, log_colors);

    // Initialize project manager state if not already done
    if projects.project_manager_state.is_none() {
        let mut state = ProjectManagerState::with_config(&services.config);
        if let Err(e) = state.initialize_database(&services.project_db) {
            logger.log_error(&format!("Failed to initialize project database: {}", e));
        }
        projects.project_manager_state = Some(state);
    }

    if let Some(ref mut manager_state) = projects.project_manager_state {
        // Handle any errors
        if let Some(error) = manager_state.last_error.take() {
            logger.log_error(&error);
        }

        ui.heading("📁 Project Database");
        ui.separator();

        // Top controls
        ui.horizontal(|ui| {
            // Search
            if ui.button("📥 Import KiCad Project…").clicked() {
                ui.ctx().memory_mut(|mem| {
                    mem.data.insert_temp(egui::Id::new("open_project_import_modal"), true);
                });
            }

            ui.separator();

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
                    let bom_cell = services.bom_state.clone();
                    if let Some(ref bom_state) = *bom_cell.lock() {
                        let components: Vec<crate::project_manager::bom::BomComponent> = bom_state.entries.iter().cloned().map(Into::into).collect();
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
                ui.add_space(30.0);
                ui.label("No projects imported yet.");
                ui.label(egui::RichText::new("Click 📥 Import KiCad Project… above, or use the Shell's `new-project <name>` command.").small().italics());
            });
        } else {
            // Load hierarchies for all projects (if not already loaded)
            for project in &manager_state.project_list {
                if !manager_state.project_hierarchies.contains_key(&project.id) {
                    use crate::project_manager::kicad_metadata::get_kicad_pro_path;
                    use crate::project_manager::kicad_hierarchy::ProjectHierarchy;

                    if let Some(kicad_pro_path) = get_kicad_pro_path(&project.pcb_file_path) {
                        if kicad_pro_path.exists() {
                            match ProjectHierarchy::from_kicad_pro(&kicad_pro_path) {
                                Ok(hierarchy) => {
                                    manager_state.project_hierarchies.insert(project.id.clone(), hierarchy);
                                }
                                Err(_) => {
                                    // Silently ignore errors - project may not have schematics
                                }
                            }
                        }
                    }
                }
            }

            // Build tree structure
            let tree_structure = build_tree_structure(&manager_state.project_list);
            let projects_by_id: HashMap<String, &ProjectMetadata> = manager_state.project_list
                .iter()
                .map(|p| (p.id.clone(), p))
                .collect();

            let current_project_id = manager_state.current_project
                .as_ref()
                .map(|p| p.metadata.id.clone());

            let selected_project_id = manager_state.selected_project_id.clone();

            // Full-width tree (right-pane details moved to a modal opened via
            // right-click → Update, or double-click on a project to load it).
            ui.heading("Projects");
            ui.add_space(6.0);

            egui::ScrollArea::vertical()
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    let (_response, actions) = TreeView::new(ui.make_persistent_id("projects_tree"))
                        .fallback_context_menu(|ui, selected| {
                            // Derive project id from selection. Rev-node ids look
                            // like "proj_X:rev:rev_01" — we want "proj_X".
                            let sel_id: Option<String> = selected.first().cloned();
                            let project_id_opt: Option<String> = sel_id.as_deref().map(|s| {
                                s.split_once(':').map(|(head, _)| head.to_string()).unwrap_or_else(|| s.to_string())
                            });
                            let is_release_node = sel_id.as_deref().map(|s| s.contains(":rev:")).unwrap_or(false);

                            if is_release_node {
                                if let Some(sel) = sel_id {
                                    ui.label(egui::RichText::new("Selected release").small().weak());
                                    if ui.button("📂 Open release folder").clicked() {
                                        // Intent carries the full "proj_X:rev:rev_01" id so the
                                        // registry can look up the release by tag.
                                        set_project_intent(ui.ctx(), "open_release", &sel);
                                        ui.close();
                                    }
                                    if ui.button("📥 Load Release Gerbers").clicked() {
                                        // Same composite id; pickup extracts the release ZIP
                                        // (cached after first time) and loads the gerbers into
                                        // the viewer without invoking kicad-cli.
                                        set_project_intent(ui.ctx(), "load_release", &sel);
                                        ui.close();
                                    }
                                    if ui.button("ℹ View Release Info").clicked() {
                                        // Opens a read-only modal showing the Release's
                                        // pedigree (tag, created_at, version, git hash,
                                        // description, changes, archive/notes paths).
                                        set_project_intent(ui.ctx(), "release_info", &sel);
                                        ui.close();
                                    }
                                    if ui.button("🔄 Regenerate Release").clicked() {
                                        set_project_intent(ui.ctx(), "regen_release", &sel);
                                        ui.close();
                                    }
                                    ui.separator();
                                    if ui.button(
                                        egui::RichText::new("🗑 Delete Release")
                                            .color(egui::Color32::from_rgb(220, 100, 100)),
                                    ).clicked() {
                                        // Opens a confirmation modal — actual delete (DB +
                                        // disk + cache) happens only when the user confirms.
                                        set_project_intent(ui.ctx(), "delete_release", &sel);
                                        ui.close();
                                    }
                                }
                            } else if let Some(project_id) = project_id_opt {
                                ui.label(egui::RichText::new("Selected project").small().weak());
                                if ui.button("📂 Open Project").clicked() {
                                    set_project_intent(ui.ctx(), "open", &project_id);
                                    ui.close();
                                }
                                if ui.button("✎ Update Project…").clicked() {
                                    set_project_intent(ui.ctx(), "update", &project_id);
                                    ui.close();
                                }
                                if ui.button("🗑 Delete Project").clicked() {
                                    set_project_intent(ui.ctx(), "delete", &project_id);
                                    ui.close();
                                }
                                ui.separator();
                                if ui.button("➕ New Child Project").clicked() {
                                    set_project_intent(ui.ctx(), "new_child", &project_id);
                                    ui.close();
                                }
                            }
                        })
                        .show(ui, |builder| {
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
                                    &selected_project_id,
                                    &manager_state.project_hierarchies,
                                    &manager_state.project_releases,
                                );
                            }
                        });

                    for action in actions {
                        match action {
                            Action::SetSelected(selected_ids) => {
                                if let Some(first_id) = selected_ids.first() {
                                    let project_id = first_id.split_once(':')
                                        .map(|(head, _)| head.to_string())
                                        .unwrap_or_else(|| first_id.clone());
                                    manager_state.selected_project_id = Some(project_id);
                                }
                            }
                            Action::Activate(activate) => {
                                // Double-click on a project node → load.
                                // Activate.selected is Vec<NodeId>; NodeId is String here.
                                if let Some(first_id) = activate.selected.first() {
                                    let project_id = first_id.split_once(':')
                                        .map(|(head, _)| head.to_string())
                                        .unwrap_or_else(|| first_id.clone());
                                    // Only trigger "open" if this is actually a known project id
                                    // (ignore activation attempts on file/rev nodes).
                                    if manager_state.project_list.iter().any(|p| p.id == project_id) {
                                        set_project_intent(ui.ctx(), "open", &project_id);
                                    }
                                }
                            }
                            _ => {}
                        }
                    }
                });

        }

        // Dispatch right-click context menu intent
        let context_intent = ui.ctx().memory(|mem| {
            mem.data.get_temp::<ProjectIntent>(egui::Id::new(PROJECT_CONTEXT_INTENT))
        });

        if let Some((action, project_id)) = context_intent {
            ui.ctx().memory_mut(|mem| {
                mem.data.remove::<ProjectIntent>(egui::Id::new(PROJECT_CONTEXT_INTENT));
            });

            logger.log_info(&format!("Context menu: {} ({})", action, project_id));

            match action.as_str() {
                "open" => {
                    // Route through the existing load_project memory-key so the
                    // full restore flow (PCB path + BOM) runs unchanged below.
                    ui.ctx().memory_mut(|mem| {
                        mem.data.insert_temp(egui::Id::new("load_project"), project_id.clone());
                    });
                }
                "update" => {
                    // Open the project-edit modal (rendered in app.rs).
                    // The registry up in app.rs/update() picks this up via
                    // memory key "open_project_edit_modal".
                    manager_state.selected_project_id = Some(project_id.clone());
                    ui.ctx().memory_mut(|mem| {
                        mem.data.insert_temp(
                            egui::Id::new("open_project_edit_modal"),
                            project_id.clone(),
                        );
                    });
                }
                "open_release" => {
                    // project_id here is actually "proj_X:rev:rev_01".
                    ui.ctx().memory_mut(|mem| {
                        mem.data.insert_temp(
                            egui::Id::new("open_release_intent"),
                            project_id.clone(),
                        );
                    });
                }
                "regen_release" => {
                    // Handed off to the Gerber Viewer ribbon's open_regenerate_release_modal.
                    ui.ctx().memory_mut(|mem| {
                        mem.data.insert_temp(
                            egui::Id::new("regen_release_intent"),
                            project_id.clone(),
                        );
                    });
                }
                "load_release" => {
                    // Handed off to the Gerber Viewer's load_release_gerbers,
                    // which extracts the release ZIP (cached) and loads those
                    // gerbers into the viewer with no kicad-cli invocation.
                    ui.ctx().memory_mut(|mem| {
                        mem.data.insert_temp(
                            egui::Id::new("load_release_intent"),
                            project_id.clone(),
                        );
                    });
                }
                "release_info" => {
                    // Picked up by app.rs::handle_release_info_intent.
                    ui.ctx().memory_mut(|mem| {
                        mem.data.insert_temp(
                            egui::Id::new("release_info_intent"),
                            project_id.clone(),
                        );
                    });
                }
                "delete_release" => {
                    // Picked up by app.rs::handle_delete_release_intent.
                    ui.ctx().memory_mut(|mem| {
                        mem.data.insert_temp(
                            egui::Id::new("delete_release_intent"),
                            project_id.clone(),
                        );
                    });
                }
                "delete" => {
                    manager_state.show_delete_confirmation = Some(project_id.clone());
                }
                "new" => {
                    // Route to the Project Import modal at app level.
                    ui.ctx().memory_mut(|mem| {
                        mem.data.insert_temp(egui::Id::new("open_project_import_modal"), true);
                    });
                }
                "new_child" => {
                    manager_state.new_project_parent_id = Some(project_id.clone());
                    ui.ctx().memory_mut(|mem| {
                        mem.data.insert_temp(egui::Id::new("open_project_import_modal"), true);
                    });
                }
                _ => {}
            }
        }

        // (save_project and edit_description memory-key handlers removed —
        // the Project Edit modal in app.rs now owns the save path and calls
        // manager_state.update_project() + update_kicad_description() directly.)

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
                    services.project_state.set(crate::project::ProjectState::PcbSelected {
                        pcb_path: project.metadata.pcb_file_path.clone(),
                    });

                    // 1b. Clear gerber-derived state so the previous project's
                    // geometry doesn't linger in 2D or 3D until the new project's
                    // gerbers are loaded (via Generate, Load, or right-click a
                    // release → Load Release Gerbers).
                    services.layer_store.clear_all();
                    services.geometry.board_outline = None;
                    services.geometry.top_copper = None;
                    services.geometry.bottom_copper = None;
                    services.geometry.top_mask = None;
                    services.geometry.bottom_mask = None;
                    services.gerber_view.needs_initial_view = true;

                    // 2. Restore BOM components if available
                    if !project.bom_components.is_empty() {
                        let bom_cell = services.bom_state.clone();
                        let mut bom_guard = bom_cell.lock();
                        if let Some(ref mut bom_state) = *bom_guard {
                            bom_state.entries = project.bom_components.iter().map(|c| {
                                crate::bom::BomEntry {
                                    item: c.item_number.parse().unwrap_or(0),
                                    reference: c.reference.clone(),
                                    value: c.value.clone(),
                                    description: c.description.clone(),
                                    footprint: c.footprint.clone(),
                                    x: c.x_location,
                                    y: c.y_location,
                                    rotation: c.orientation,
                                    layer: String::new(),
                                }
                            }).collect();
                            logger.log_info(&format!("Restored {} BOM components", project.bom_components.len()));
                        } else {
                            logger.log_warning("BOM state not initialized yet. Components will be loaded when BOM tab is opened.");
                        }
                    }

                    // 3. Log PCB file status
                    if project.metadata.pcb_file_path.exists() {
                        logger.log_info(&format!("PCB file: {}", project.metadata.pcb_file_path.display()));
                    } else {
                        logger.log_warning(&format!("PCB file not found at: {}", project.metadata.pcb_file_path.display()));
                    }

                    // 4. Auto-load the most recent release's gerbers (if any)
                    // so the viewer immediately shows the new project's
                    // geometry. Routes through the existing load_release_intent
                    // — pickup in the Gerber Viewer tab fires the load next
                    // frame, so this sidesteps borrow conflicts with
                    // manager_state. If there are no releases yet, the user
                    // Generates / Loads manually from the PCB File tab.
                    if let Some(latest) = project.releases.iter()
                        .max_by_key(|r| r.created_at)
                    {
                        let composite = format!("{}:rev:{}", project_id, latest.tag);
                        ui.ctx().memory_mut(|mem| {
                            mem.data.insert_temp(
                                egui::Id::new("load_release_intent"),
                                composite,
                            );
                        });
                        logger.log_info(&format!("Auto-loading latest release: {}", latest.tag));
                    }

                    logger.log_info(&format!("✅ Loaded project: {}", project_name));
                }
            }
        }
    }

    // PM modals + intent handlers run after the panel body so ALL Project-
    // Manager rendering (panel + modals) flows through this one function.
    // They depend only on the panel_state + services — no CopperForgeApp.
    crate::ui::show_projects_modals(projects, services, ui.ctx());
}

/// Helper function to update description in .kicad_pro file.
/// Called by the Project Edit modal (app.rs) on Save.
pub fn update_kicad_description(kicad_pro_path: &std::path::Path, new_description: &str) -> Result<(), Box<dyn std::error::Error>> {
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

/// Recursively show tree nodes using builder pattern.
/// Project nodes are `activatable(true)` so double-click → Load.
/// Under each project: schematic → sheets, pcb, and `outputs/` with rev leaves.
fn show_tree_node_builder(
    builder: &mut egui_ltreeview::TreeViewBuilder<String>,
    project: &ProjectMetadata,
    tree_structure: &HashMap<String, TreeNode>,
    projects_by_id: &HashMap<String, &ProjectMetadata>,
    current_project_id: &Option<String>,
    selected_project_id: &Option<String>,
    project_hierarchies: &HashMap<String, crate::project_manager::kicad_hierarchy::ProjectHierarchy>,
    project_releases: &HashMap<String, Vec<crate::release::Release>>,
) {
    let is_current = current_project_id
        .as_ref()
        .map(|id| id == &project.id)
        .unwrap_or(false);

    let is_selected = selected_project_id
        .as_ref()
        .map(|id| id == &project.id)
        .unwrap_or(false);

    let dir_path = project.pcb_file_path
        .parent()
        .and_then(|p| p.to_str())
        .unwrap_or("");

    let label_text = if is_selected && !dir_path.is_empty() {
        format!("{} ({})", project.name, dir_path)
    } else {
        project.name.clone()
    };

    let label = if is_current {
        egui::RichText::new(&label_text).strong().color(egui::Color32::LIGHT_BLUE)
    } else {
        egui::RichText::new(&label_text)
    };

    let has_child_projects = tree_structure
        .get(&project.id)
        .map(|node| !node.children.is_empty())
        .unwrap_or(false);

    let has_kicad_hierarchy = project_hierarchies
        .get(&project.id)
        .map(|h| h.root_schematic.is_some() || h.pcb_file.is_some() || !h.sheets.is_empty())
        .unwrap_or(false);

    let has_releases = project_releases
        .get(&project.id)
        .map(|r| !r.is_empty())
        .unwrap_or(false);

    // Project row is always rendered as a directory if it has any children to
    // expand, otherwise as an activatable leaf.
    let has_any_children = has_child_projects || has_kicad_hierarchy || has_releases;

    if has_any_children {
        builder.node(
            NodeBuilder::dir(project.id.clone())
                .label(label)
                .activatable(true),
        );

        // Child projects first (by parent_id hierarchy).
        if let Some(node) = tree_structure.get(&project.id) {
            for child_id in &node.children {
                if let Some(child_project) = projects_by_id.get(child_id) {
                    show_tree_node_builder(
                        builder,
                        child_project,
                        tree_structure,
                        projects_by_id,
                        current_project_id,
                        selected_project_id,
                        project_hierarchies,
                        project_releases,
                    );
                }
            }
        }

        // KiCad files: schematic (+ sheets) and pcb.
        if let Some(hierarchy) = project_hierarchies.get(&project.id) {
            if let Some(ref root_sch) = hierarchy.root_schematic {
                let file_name = root_sch.file_name().and_then(|f| f.to_str()).unwrap_or("Unknown");
                if !hierarchy.sheets.is_empty() {
                    builder.dir(format!("{}:sch_root", project.id), egui::RichText::new(format!("📄 {}", file_name)));
                    for (idx, sheet) in hierarchy.sheets.iter().enumerate() {
                        show_hierarchical_sheet_node(builder, sheet, &format!("{}:sheet_{}", project.id, idx));
                    }
                    builder.close_dir();
                } else {
                    builder.leaf(format!("{}:sch_root", project.id), egui::RichText::new(format!("📄 {}", file_name)));
                }
            }
            if let Some(ref pcb) = hierarchy.pcb_file {
                let file_name = pcb.file_name().and_then(|f| f.to_str()).unwrap_or("Unknown");
                builder.leaf(format!("{}:pcb", project.id), egui::RichText::new(format!("🔧 {}", file_name)));
            }
        }

        // outputs/ subtree — one leaf per release.
        if has_releases {
            builder.dir(format!("{}:outputs", project.id), egui::RichText::new("📁 outputs"));
            if let Some(releases) = project_releases.get(&project.id) {
                for rel in releases {
                    builder.leaf(
                        format!("{}:rev:{}", project.id, rel.tag),
                        egui::RichText::new(format!("📦 {}", rel.tag)),
                    );
                }
            }
            builder.close_dir();
        }

        builder.close_dir();
    } else {
        builder.node(
            NodeBuilder::leaf(project.id.clone())
                .label(label)
                .activatable(true),
        );
    }
}

/// Recursively show hierarchical sheet nodes
fn show_hierarchical_sheet_node(
    builder: &mut egui_ltreeview::TreeViewBuilder<String>,
    sheet: &crate::project_manager::kicad_hierarchy::HierarchicalSheet,
    node_id: &str,
) {
    if sheet.sub_sheets.is_empty() {
        // Leaf sheet
        builder.leaf(node_id.to_string(), egui::RichText::new(format!("📋 {}", sheet.name)));
    } else {
        // Parent sheet with sub-sheets
        builder.dir(node_id.to_string(), egui::RichText::new(format!("📋 {}", sheet.name)));

        for (idx, sub_sheet) in sheet.sub_sheets.iter().enumerate() {
            show_hierarchical_sheet_node(builder, sub_sheet, &format!("{}:sub_{}", node_id, idx));
        }

        builder.close_dir();
    }
}
