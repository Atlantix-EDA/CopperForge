//! Project-Manager modal rendering + intent handlers.
//!
//! These are the modal/handler functions that drive the Projects panel's
//! pop-up windows (create release, edit project, import, delete-release
//! confirmation, etc.). They were lifted out of `impl CopperForgeApp` so the
//! whole PM surface depends ONLY on its own `ProjectsPanelState` plus the
//! shared `SharedServices` — never on the `CopperForgeApp` god-struct. That
//! lets the Projects citizen walk out into its own crate later.
//!
//! Each function takes the disjoint `(&mut ProjectsPanelState, &mut
//! SharedServices, &egui::Context)` instead of `&mut self`.

use crate::app::{
    DeleteReleaseConfirmation, ProjectEditModalState, ProjectImportModalState, ProjectsPanelState,
    ReleaseModalState,
};
use crate::event_logger::ReactiveEventLogger;
use crate::project_manager;
use crate::services::SharedServices;

/// Run all PM modals + intent handlers for this frame. Called from
/// `show_projects_panel` after the panel body is rendered, so the whole PM
/// surface (panel + modals) flows through one entry point.
pub fn show_projects_modals(
    panel_state: &mut ProjectsPanelState,
    services: &mut SharedServices,
    ctx: &egui::Context,
) {
    show_release_modal(panel_state, services, ctx);
    handle_release_info_intent(panel_state, services, ctx);
    show_release_info_modal(panel_state, services, ctx);
    handle_delete_release_intent(panel_state, services, ctx);
    show_delete_release_confirmation(panel_state, services, ctx);
    handle_project_edit_open(panel_state, services, ctx);
    show_project_edit_modal(panel_state, services, ctx);
    handle_release_open_intent(panel_state, services, ctx);
    handle_project_import_open(panel_state, services, ctx);
    show_project_import_modal(panel_state, services, ctx);
}

/// Build a kicad-cli Command for the configured method, if any.
fn kicad_cli_command(services: &SharedServices) -> Option<std::process::Command> {
    services
        .kicad_cli_method
        .as_deref()
        .map(crate::app::CopperForgeApp::build_kicad_cli_command)
}

/// Render the release modal and handle Create/Cancel actions.
fn show_release_modal(
    panel_state: &mut ProjectsPanelState,
    services: &mut SharedServices,
    ctx: &egui::Context,
) {
    if panel_state.release_modal.is_none() {
        return;
    }

    let mut close = false;
    let mut trigger_create = false;

    let window_title = if panel_state.release_modal.as_ref().map(|m| m.overwrite_existing).unwrap_or(false) {
        "🔄 Regenerate Release"
    } else {
        "🚀 Create Release"
    };

    egui::Window::new(window_title)
        .collapsible(false)
        .resizable(true)
        .default_size(egui::vec2(520.0, 440.0))
        .default_pos(egui::pos2(
            ctx.content_rect().center().x - 260.0,
            ctx.content_rect().center().y - 220.0,
        ))
        .show(ctx, |ui| {
            let modal = panel_state.release_modal.as_mut().unwrap();

            if modal.overwrite_existing {
                ui.colored_label(
                    egui::Color32::from_rgb(230, 200, 120),
                    format!("⚠ Regenerating '{}' — existing zip + notes will be overwritten.", modal.rev_tag),
                );
                ui.add_space(6.0);
            }

            ui.label("Archive gerbers + drill files as a tagged release under");
            ui.monospace("<project>/outputs/<rev_tag>/");
            ui.add_space(8.0);

            egui::Grid::new("release_modal_grid")
                .num_columns(2)
                .spacing([12.0, 6.0])
                .show(ui, |ui| {
                    ui.label("Rev tag:");
                    ui.text_edit_singleline(&mut modal.rev_tag);
                    ui.end_row();

                    ui.label("Include date in filename:");
                    ui.checkbox(&mut modal.include_date_in_name, "e.g. _18Apr2026");
                    ui.end_row();

                    ui.label("Include RELEASE_NOTES.md in zip:");
                    ui.checkbox(&mut modal.include_notes_in_zip, "(off = client-only notes)");
                    ui.end_row();
                });

            ui.add_space(8.0);
            ui.label("Description (what this board is about):");
            ui.add(egui::TextEdit::multiline(&mut modal.description)
                .desired_width(f32::INFINITY)
                .desired_rows(3));

            ui.add_space(8.0);
            ui.label("Changes from previous version:");
            ui.add(egui::TextEdit::multiline(&mut modal.changes)
                .desired_width(f32::INFINITY)
                .desired_rows(6)
                .hint_text("- changed footprint on U2\n- routed power plane stitching\n- ..."));

            if let Some(ref err) = modal.error {
                ui.add_space(6.0);
                ui.colored_label(egui::Color32::from_rgb(245, 120, 140), err);
            }

            ui.add_space(12.0);
            ui.horizontal(|ui| {
                let go_label = if modal.overwrite_existing { "Regenerate" } else { "Create Release" };
                if ui.button(go_label).clicked() {
                    trigger_create = true;
                }
                if ui.button("Cancel").clicked() {
                    close = true;
                }
            });
        });

    if trigger_create {
        execute_release_from_modal(panel_state, services);
    }
    if close {
        panel_state.release_modal = None;
    }
}

/// Validate + run the create-release flow using the modal's current state.
fn execute_release_from_modal(panel_state: &mut ProjectsPanelState, services: &mut SharedServices) {
    // Clone modal data out so we can mutably borrow panel_state for the release call.
    let (modal, overwrite) = match panel_state.release_modal.as_ref() {
        Some(m) => (m.clone_for_exec(), m.overwrite_existing),
        None => return,
    };

    // Validate
    if modal.rev_tag.trim().is_empty() {
        if let Some(ref mut m) = panel_state.release_modal {
            m.error = Some("Rev tag cannot be empty".into());
        }
        return;
    }

    // Require project record + Ready state
    use crate::project::ProjectState;
    let (pcb_path, gerber_dir) = match services.project_state.get() {
        ProjectState::Ready { pcb_path, gerber_dir, .. } => (pcb_path, gerber_dir),
        _ => {
            if let Some(ref mut m) = panel_state.release_modal {
                m.error = Some("Gerbers must be loaded (state: Ready) before releasing.".into());
            }
            return;
        }
    };

    // Collision check against existing releases — skipped in regenerate mode.
    if !overwrite {
        let current_pm_state = panel_state.project_manager_state.as_ref();
        let has_collision = current_pm_state
            .and_then(|s| s.current_project.as_ref())
            .map(|p| p.releases.iter().any(|r| r.tag == modal.rev_tag))
            .unwrap_or(false);
        if has_collision {
            if let Some(ref mut m) = panel_state.release_modal {
                m.error = Some(format!(
                    "Release '{}' already exists. Right-click the rev in the Projects tree → Regenerate.",
                    modal.rev_tag
                ));
            }
            return;
        }
    }

    // Build kicad-cli Command for drill export
    let Some(kicad_cli) = kicad_cli_command(services) else {
        if let Some(ref mut m) = panel_state.release_modal {
            m.error = Some("kicad-cli not discovered at startup — cannot export drill files.".into());
        }
        return;
    };

    let os_description = build_os_description();
    let kicad_version = services.kicad_version.clone();
    let logger_state = services.logger_state.clone();
    let log_colors = services.log_colors.clone();
    let logger = ReactiveEventLogger::with_colors(&logger_state, &log_colors);

    let req = crate::release::ReleaseRequest {
        rev_tag: modal.rev_tag.clone(),
        description: modal.description.clone(),
        changes: modal.changes.clone(),
        include_date_in_name: modal.include_date_in_name,
        include_notes_in_zip: modal.include_notes_in_zip,
        target: modal.target,
    };
    let sources = crate::release::ReleaseSources {
        pcb_path: &pcb_path,
        gerber_dir: &gerber_dir,
        kicad_cli,
        kicad_version,
        os_description,
    };

    logger.log_info(&format!("Creating release '{}'...", modal.rev_tag));
    match crate::release::create_release(&req, sources, &logger) {
        Ok(outcome) => {
            // Persist: either append (new release) or replace-in-place
            // (regenerate). Also update the tree-rendering cache so the
            // new/updated rev shows up immediately under outputs/.
            if let Some(ref mut pm) = panel_state.project_manager_state {
                let project_id_opt = pm.current_project.as_ref().map(|p| p.metadata.id.clone());
                if let Some(ref mut current) = pm.current_project {
                    if overwrite {
                        if let Some(slot) = current.releases.iter_mut().find(|r| r.tag == outcome.release.tag) {
                            *slot = outcome.release.clone();
                        } else {
                            // Cache/DB had drifted; just append.
                            current.releases.push(outcome.release.clone());
                        }
                    } else {
                        current.releases.push(outcome.release.clone());
                    }
                    current.metadata.last_modified = chrono::Utc::now();
                    if let Err(e) = services.project_db.save_project(current) {
                        logger.log_error(&format!("Release written to disk but DB save failed: {}", e));
                    }
                }
                if let Some(id) = project_id_opt {
                    if overwrite {
                        let tag = outcome.release.tag.clone();
                        let entry = pm.project_releases.entry(id).or_default();
                        if let Some(slot) = entry.iter_mut().find(|r| r.tag == tag) {
                            *slot = outcome.release.clone();
                        } else {
                            entry.push(outcome.release.clone());
                        }
                    } else {
                        pm.record_release(&id, outcome.release.clone());
                    }
                }
            }
            logger.log_info(&format!("Release '{}' complete: {}", outcome.release.tag, outcome.release.archive_path.display()));
            panel_state.release_modal = None;
        }
        Err(e) => {
            logger.log_error(&format!("Release failed: {}", e));
            if let Some(ref mut m) = panel_state.release_modal {
                m.error = Some(e);
            }
        }
    }
}

fn build_os_description() -> String {
    let mut d = crate::platform::details::Details::new();
    d.get_os();
    if d.name.is_empty() {
        "(unknown OS)".into()
    } else {
        format!("{} (kernel {})", d.name, d.kernel)
    }
}

// ─── Project Edit modal ────────────────────────────────────────

/// Called from the update() pass. Picks up the "open_project_edit_modal"
/// memory key set by the Projects tab's right-click → Update handler and
/// seeds `project_edit_modal` from the DB's full ProjectData record.
fn handle_project_edit_open(
    panel_state: &mut ProjectsPanelState,
    services: &mut SharedServices,
    ctx: &egui::Context,
) {
    let pid = ctx.memory(|mem| {
        mem.data.get_temp::<String>(egui::Id::new("open_project_edit_modal"))
    });
    let Some(pid) = pid else { return; };
    ctx.memory_mut(|mem| {
        mem.data.remove::<String>(egui::Id::new("open_project_edit_modal"));
    });

    // Load the full ProjectData so the modal can show releases + kicad_pro
    // metadata (author, company).
    let data = match services.project_db.load_project(&pid) {
        Ok(Some(d)) => d,
        _ => return,
    };
    let meta = crate::project_manager::kicad_metadata::get_kicad_pro_path(&data.metadata.pcb_file_path)
        .and_then(|pro_path| {
            if pro_path.exists() {
                crate::project_manager::kicad_metadata::read_kicad_metadata(&pro_path).ok()
            } else {
                None
            }
        });
    let (author, company) = match meta {
        Some(m) => (m.author, m.company),
        None => (None, None),
    };

    panel_state.project_edit_modal = Some(ProjectEditModalState {
        project_id: data.metadata.id.clone(),
        name: data.metadata.name.clone(),
        description: data.metadata.description.clone(),
        tags: data.metadata.tags.join(", "),
        author,
        company,
        created_at: data.metadata.created_at,
        last_modified: data.metadata.last_modified,
        pcb_file_path: data.metadata.pcb_file_path.clone(),
        releases: data.releases.clone(),
        error: None,
    });
}

// ─── Release Info modal (read-only pedigree) ────────────────────

/// Pick up "release_info_intent" (value: "proj_X:rev:rev_01") and
/// seed the read-only Release Details window.
fn handle_release_info_intent(
    panel_state: &mut ProjectsPanelState,
    _services: &mut SharedServices,
    ctx: &egui::Context,
) {
    let intent = ctx.memory(|mem| {
        mem.data.get_temp::<String>(egui::Id::new("release_info_intent"))
    });
    let Some(intent) = intent else { return; };
    ctx.memory_mut(|mem| {
        mem.data.remove::<String>(egui::Id::new("release_info_intent"));
    });

    let mut parts = intent.splitn(3, ':');
    let project_id = match parts.next() { Some(s) => s, None => return };
    let _marker = parts.next();
    let rev_tag = match parts.next() { Some(s) => s, None => return };

    let release = panel_state.project_manager_state
        .as_ref()
        .and_then(|pm| pm.project_releases.get(project_id))
        .and_then(|releases| releases.iter().find(|r| r.tag == rev_tag))
        .cloned();
    if let Some(r) = release {
        panel_state.release_info_modal = Some(r);
    }
}

fn show_release_info_modal(
    panel_state: &mut ProjectsPanelState,
    _services: &mut SharedServices,
    ctx: &egui::Context,
) {
    let Some(release) = panel_state.release_info_modal.clone() else { return; };
    let mut close = false;
    egui::Window::new(format!("Release: {}", release.tag))
        .collapsible(false)
        .resizable(true)
        .default_width(540.0)
        .default_pos(egui::pos2(
            ctx.content_rect().center().x - 270.0,
            ctx.content_rect().center().y - 220.0,
        ))
        .show(ctx, |ui| {
            ui.add_space(6.0);
            egui::Grid::new("release_info_grid")
                .num_columns(2)
                .spacing([14.0, 6.0])
                .show(ui, |ui| {
                    ui.label(egui::RichText::new("Tag").strong());
                    ui.label(egui::RichText::new(&release.tag).monospace());
                    ui.end_row();

                    ui.label(egui::RichText::new("Created").strong());
                    ui.label(release.created_at.format("%Y-%m-%d %H:%M:%S UTC").to_string());
                    ui.end_row();

                    ui.label(egui::RichText::new("KiCad version").strong());
                    ui.label(release.kicad_version.clone().unwrap_or_else(|| "(unknown)".into()));
                    ui.end_row();

                    ui.label(egui::RichText::new("Git commit").strong());
                    ui.label(
                        egui::RichText::new(
                            release.git_hash.clone().unwrap_or_else(|| "(not in a git repo)".into())
                        )
                        .monospace()
                    );
                    ui.end_row();

                    ui.label(egui::RichText::new("Date in name").strong());
                    ui.label(if release.include_date_in_name { "yes" } else { "no" });
                    ui.end_row();

                    ui.label(egui::RichText::new("Notes in zip").strong());
                    ui.label(if release.include_notes_in_zip { "yes" } else { "no" });
                    ui.end_row();

                    ui.label(egui::RichText::new("Archive").strong());
                    ui.label(
                        egui::RichText::new(release.archive_path.display().to_string())
                            .monospace()
                            .small(),
                    );
                    ui.end_row();

                    ui.label(egui::RichText::new("Notes").strong());
                    ui.label(
                        egui::RichText::new(release.notes_path.display().to_string())
                            .monospace()
                            .small(),
                    );
                    ui.end_row();
                });

            ui.add_space(8.0);
            ui.label(egui::RichText::new("Description").strong());
            ui.add_space(2.0);
            ui.label(if release.description.is_empty() { "(none)" } else { release.description.as_str() });

            ui.add_space(8.0);
            ui.label(egui::RichText::new("Changes").strong());
            ui.add_space(2.0);
            egui::ScrollArea::vertical()
                .max_height(160.0)
                .show(ui, |ui| {
                    ui.label(if release.changes.is_empty() { "(none)" } else { release.changes.as_str() });
                });

            ui.add_space(10.0);
            ui.separator();
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.button("Close").clicked() {
                        close = true;
                    }
                });
            });
        });
    if close {
        panel_state.release_info_modal = None;
    }
}

// ─── Delete Release confirmation ───────────────────────────────

/// Pick up "delete_release_intent" and seed the confirmation modal.
fn handle_delete_release_intent(
    panel_state: &mut ProjectsPanelState,
    _services: &mut SharedServices,
    ctx: &egui::Context,
) {
    let intent = ctx.memory(|mem| {
        mem.data.get_temp::<String>(egui::Id::new("delete_release_intent"))
    });
    let Some(intent) = intent else { return; };
    ctx.memory_mut(|mem| {
        mem.data.remove::<String>(egui::Id::new("delete_release_intent"));
    });

    let mut parts = intent.splitn(3, ':');
    let project_id = match parts.next() { Some(s) => s.to_string(), None => return };
    let _marker = parts.next();
    let rev_tag = match parts.next() { Some(s) => s.to_string(), None => return };

    let archive_path = panel_state.project_manager_state
        .as_ref()
        .and_then(|pm| pm.project_releases.get(&project_id))
        .and_then(|releases| releases.iter().find(|r| r.tag == rev_tag))
        .map(|r| r.archive_path.clone());
    let Some(archive_path) = archive_path else { return; };

    panel_state.delete_release_confirmation = Some(DeleteReleaseConfirmation {
        project_id,
        rev_tag,
        archive_path,
        error: None,
    });
}

fn show_delete_release_confirmation(
    panel_state: &mut ProjectsPanelState,
    services: &mut SharedServices,
    ctx: &egui::Context,
) {
    if panel_state.delete_release_confirmation.is_none() {
        return;
    }
    let (project_id, rev_tag, archive_path, error) = {
        let c = panel_state.delete_release_confirmation.as_ref().unwrap();
        (c.project_id.clone(), c.rev_tag.clone(), c.archive_path.clone(), c.error.clone())
    };

    let mut cancel = false;
    let mut confirm = false;

    egui::Window::new(format!("Delete release '{}'?", rev_tag))
        .collapsible(false)
        .resizable(false)
        .default_width(480.0)
        .default_pos(egui::pos2(
            ctx.content_rect().center().x - 240.0,
            ctx.content_rect().center().y - 140.0,
        ))
        .show(ctx, |ui| {
            ui.add_space(8.0);
            ui.label("This removes:");
            ui.label(egui::RichText::new("  • The release entry from the project database").small());
            ui.label(egui::RichText::new("  • The outputs/<rev>/ folder on disk (zip, BOM, notes)").small());
            ui.label(egui::RichText::new("  • The cached extracted gerbers (if any)").small());
            ui.add_space(6.0);
            if let Some(parent) = archive_path.parent() {
                ui.label(egui::RichText::new("Path:").strong().small());
                ui.label(egui::RichText::new(parent.display().to_string()).monospace().small());
            }
            ui.add_space(8.0);
            ui.label(
                egui::RichText::new("This cannot be undone.")
                    .italics()
                    .color(egui::Color32::from_rgb(220, 180, 80)),
            );

            if let Some(err) = error.as_ref() {
                ui.add_space(6.0);
                ui.label(egui::RichText::new(err).color(egui::Color32::from_rgb(220, 100, 100)));
            }

            ui.add_space(10.0);
            ui.separator();
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.button(
                        egui::RichText::new("🗑 Delete")
                            .color(egui::Color32::from_rgb(220, 100, 100))
                            .strong(),
                    ).clicked() {
                        confirm = true;
                    }
                    if ui.button("Cancel").clicked() {
                        cancel = true;
                    }
                });
            });
        });

    if cancel {
        panel_state.delete_release_confirmation = None;
        return;
    }

    if confirm {
        let logger_state = services.logger_state.clone();
        let log_colors = services.log_colors.clone();
        let logger = ReactiveEventLogger::with_colors(&logger_state, &log_colors);

        let result = crate::app::delete_release_artifacts(
            &services.project_db,
            panel_state.project_manager_state.as_mut(),
            &project_id,
            &rev_tag,
            &archive_path,
            &logger,
        );
        match result {
            Ok(()) => {
                logger.log_info(&format!("Deleted release '{}'", rev_tag));
                panel_state.delete_release_confirmation = None;
            }
            Err(e) => {
                if let Some(c) = panel_state.delete_release_confirmation.as_mut() {
                    c.error = Some(format!("Delete failed: {e}"));
                }
                logger.log_error(&format!("Delete release '{}' failed: {}", rev_tag, e));
            }
        }
    }
}

fn show_project_edit_modal(
    panel_state: &mut ProjectsPanelState,
    services: &mut SharedServices,
    ctx: &egui::Context,
) {
    if panel_state.project_edit_modal.is_none() { return; }
    let mut close = false;
    let mut save = false;

    egui::Window::new("✎ Project Details")
        .collapsible(false)
        .resizable(true)
        .default_size(egui::vec2(560.0, 520.0))
        .default_pos(egui::pos2(
            ctx.content_rect().center().x - 280.0,
            ctx.content_rect().center().y - 260.0,
        ))
        .show(ctx, |ui| {
            let modal = panel_state.project_edit_modal.as_mut().unwrap();

            egui::Grid::new("project_edit_grid_meta")
                .num_columns(2)
                .spacing([12.0, 6.0])
                .show(ui, |ui| {
                    ui.label(egui::RichText::new("PCB file:").strong());
                    ui.monospace(modal.pcb_file_path.display().to_string());
                    ui.end_row();

                    ui.label(egui::RichText::new("Author:").strong());
                    ui.label(modal.author.as_deref().unwrap_or("(not set in .kicad_pro)"));
                    ui.end_row();

                    ui.label(egui::RichText::new("Company:").strong());
                    ui.label(modal.company.as_deref().unwrap_or("(not set in .kicad_pro)"));
                    ui.end_row();

                    ui.label(egui::RichText::new("Created:").strong());
                    ui.label(modal.created_at.format("%Y-%m-%d %H:%M UTC").to_string());
                    ui.end_row();

                    ui.label(egui::RichText::new("Last Modified:").strong());
                    ui.label(modal.last_modified.format("%Y-%m-%d %H:%M UTC").to_string());
                    ui.end_row();
                });

            ui.add_space(10.0);
            ui.label(egui::RichText::new("Editable fields").strong());

            egui::Grid::new("project_edit_grid_edit")
                .num_columns(2)
                .spacing([12.0, 6.0])
                .show(ui, |ui| {
                    ui.label("Name:");
                    ui.text_edit_singleline(&mut modal.name);
                    ui.end_row();

                    ui.label("Tags:");
                    ui.text_edit_singleline(&mut modal.tags);
                    ui.end_row();
                });

            ui.add_space(6.0);
            ui.label("Description:");
            ui.add(egui::TextEdit::multiline(&mut modal.description)
                .desired_width(f32::INFINITY)
                .desired_rows(5));

            ui.add_space(10.0);
            ui.collapsing(format!("Releases ({})", modal.releases.len()), |ui| {
                if modal.releases.is_empty() {
                    ui.label(egui::RichText::new("No releases yet. Use 🚀 Release on the Gerber Viewer ribbon.").italics());
                } else {
                    for rel in &modal.releases {
                        ui.horizontal(|ui| {
                            ui.monospace(&rel.tag);
                            ui.label(rel.created_at.format("%Y-%m-%d").to_string());
                            ui.label(egui::RichText::new(rel.archive_path.display().to_string()).small().weak());
                        });
                    }
                }
            });

            if let Some(ref err) = modal.error {
                ui.add_space(6.0);
                ui.colored_label(egui::Color32::from_rgb(245, 120, 140), err);
            }

            ui.add_space(12.0);
            ui.horizontal(|ui| {
                if ui.button("Save").clicked() { save = true; }
                if ui.button("Cancel").clicked() { close = true; }
            });
        });

    if save {
        save_project_edit_modal(panel_state, services);
    }
    if close {
        panel_state.project_edit_modal = None;
    }
}

fn save_project_edit_modal(panel_state: &mut ProjectsPanelState, services: &mut SharedServices) {
    let modal = match panel_state.project_edit_modal.as_ref() {
        Some(m) => (m.project_id.clone(), m.name.clone(), m.description.clone(), m.tags.clone(), m.pcb_file_path.clone()),
        None => return,
    };
    let (pid, name, description, tags_str, pcb_path) = modal;

    if name.trim().is_empty() {
        if let Some(ref mut m) = panel_state.project_edit_modal {
            m.error = Some("Name cannot be empty".into());
        }
        return;
    }

    let tags: Vec<String> = tags_str
        .split(',')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();

    let logger_state = services.logger_state.clone();
    let log_colors = services.log_colors.clone();
    let logger = ReactiveEventLogger::with_colors(&logger_state, &log_colors);

    // Update DB record via ProjectManagerState (keeps its project_list in sync).
    let result = if let Some(ref mut pm) = panel_state.project_manager_state {
        pm.update_project(&pid, name.clone(), description.clone(), tags)
    } else {
        return;
    };

    match result {
        Ok(()) => {
            logger.log_info(&format!("Saved project: {}", name));
            if let Some(pro_path) = crate::project_manager::kicad_metadata::get_kicad_pro_path(&pcb_path) {
                if pro_path.exists() {
                    if let Err(e) = crate::ui::projects_panel::update_kicad_description(&pro_path, &description) {
                        logger.log_warning(&format!("Could not update .kicad_pro description: {}", e));
                    } else {
                        logger.log_info("Updated description in .kicad_pro");
                    }
                }
            }
            panel_state.project_edit_modal = None;
        }
        Err(e) => {
            if let Some(ref mut m) = panel_state.project_edit_modal {
                m.error = Some(format!("Save failed: {}", e));
            }
        }
    }
}

// ─── Release right-click → open folder ─────────────────────────

/// Pick up "open_release_intent" (value: "proj_X:rev:rev_01") and open the
/// containing release dir via xdg-open / open / explorer.
#[cfg(not(target_arch = "wasm32"))]
fn handle_release_open_intent(
    panel_state: &ProjectsPanelState,
    services: &SharedServices,
    ctx: &egui::Context,
) {
    let intent = ctx.memory(|mem| {
        mem.data.get_temp::<String>(egui::Id::new("open_release_intent"))
    });
    let Some(intent) = intent else { return; };
    ctx.memory_mut(|mem| {
        mem.data.remove::<String>(egui::Id::new("open_release_intent"));
    });

    // Parse "proj_X:rev:rev_01" → (project_id, rev_tag).
    let mut parts = intent.splitn(3, ':');
    let project_id = match parts.next() { Some(s) => s, None => return };
    let _marker = parts.next(); // "rev"
    let rev_tag = match parts.next() { Some(s) => s, None => return };

    let logger_state = services.logger_state.clone();
    let log_colors = services.log_colors.clone();
    let logger = ReactiveEventLogger::with_colors(&logger_state, &log_colors);

    // Find the release in the cache to get its archive path.
    let releases = panel_state.project_manager_state
        .as_ref()
        .and_then(|pm| pm.project_releases.get(project_id))
        .cloned()
        .unwrap_or_default();
    let release = releases.iter().find(|r| r.tag == rev_tag);
    let Some(release) = release else {
        logger.log_error(&format!("Release {} not found in cache", rev_tag));
        return;
    };
    let rev_dir = match release.archive_path.parent() {
        Some(d) => d,
        None => {
            logger.log_error("Release archive path has no parent dir");
            return;
        }
    };

    #[cfg(target_os = "linux")]
    let opener = "xdg-open";
    #[cfg(target_os = "macos")]
    let opener = "open";
    #[cfg(target_os = "windows")]
    let opener = "explorer";

    match std::process::Command::new(opener).arg(rev_dir).spawn() {
        Ok(_) => logger.log_info(&format!("Opened release folder: {}", rev_dir.display())),
        Err(e) => logger.log_error(&format!("Failed to open {}: {}", rev_dir.display(), e)),
    }
}

/// Wasm build: no native shell to launch a file manager, so the
/// "open release folder" intent is a no-op in the browser.
#[cfg(target_arch = "wasm32")]
fn handle_release_open_intent(
    _panel_state: &ProjectsPanelState,
    _services: &SharedServices,
    _ctx: &egui::Context,
) {
}

// ─── Project Import modal ──────────────────────────────────────

/// Pick up the "open_project_import_modal" memory key set by the
/// Projects tab's Import button click, and seed a fresh modal.
fn handle_project_import_open(
    panel_state: &mut ProjectsPanelState,
    _services: &mut SharedServices,
    ctx: &egui::Context,
) {
    let fire = ctx.memory(|mem| {
        mem.data.get_temp::<bool>(egui::Id::new("open_project_import_modal")).unwrap_or(false)
    });
    if !fire { return; }
    ctx.memory_mut(|mem| {
        mem.data.remove::<bool>(egui::Id::new("open_project_import_modal"));
    });
    panel_state.project_import_modal = Some(ProjectImportModalState {
        pcb_file_path: None,
        name: String::new(),
        description: String::new(),
        tags: String::new(),
        author: None,
        company: None,
        missing_pedigree: Vec::new(),
        error: None,
    });
    panel_state.project_import_last_picked = None;
}

fn show_project_import_modal(
    panel_state: &mut ProjectsPanelState,
    services: &mut SharedServices,
    ctx: &egui::Context,
) {
    if panel_state.project_import_modal.is_none() { return; }

    // Poll the file dialog first so auto-population runs this frame.
    if let Some(pro_path) = panel_state.project_import_dialog.update(ctx).picked() {
        let pro_path = pro_path.to_path_buf();
        if panel_state.project_import_last_picked.as_ref() != Some(&pro_path) {
            panel_state.project_import_last_picked = Some(pro_path.clone());
            if let Some(ref mut m) = panel_state.project_import_modal {
                m.pcb_file_path = Some(pro_path.with_extension("kicad_pcb"));

                // Auto-fill pedigree.
                let mut missing: Vec<&'static str> = Vec::new();
                match crate::project_manager::kicad_metadata::read_kicad_metadata(&pro_path) {
                    Ok(meta) => {
                        if meta.author.is_none() { missing.push("Author"); }
                        if meta.company.is_none() { missing.push("Company"); }
                        m.author = meta.author;
                        m.company = meta.company;
                        if let Some(desc) = meta.description {
                            if m.description.is_empty() { m.description = desc; }
                        }
                    }
                    Err(_) => {
                        missing.push("Author");
                        missing.push("Company");
                    }
                }
                if m.name.is_empty() {
                    if let Some(stem) = pro_path.file_stem() {
                        m.name = stem.to_string_lossy().into_owned();
                    }
                }
                m.missing_pedigree = missing;
                m.error = None;
            }
        }
    }

    let mut close = false;
    let mut trigger_import = false;

    egui::Window::new("📥 Import KiCad Project")
        .collapsible(false)
        .resizable(true)
        .default_size(egui::vec2(560.0, 480.0))
        .default_pos(egui::pos2(
            ctx.content_rect().center().x - 280.0,
            ctx.content_rect().center().y - 240.0,
        ))
        .show(ctx, |ui| {
            let modal = panel_state.project_import_modal.as_mut().unwrap();

            // File picker row
            ui.horizontal(|ui| {
                ui.label("KiCad Project File (.kicad_pro):");
                if ui.button("Browse...").clicked() {
                    use std::sync::Arc;
                    use std::mem;
                    use egui_file_dialog::FileDialog;
                    let dialog = mem::replace(&mut panel_state.project_import_dialog, FileDialog::new());
                    let mut dialog = dialog
                        .add_file_filter("KiCad Project", Arc::new(|path: &std::path::Path| {
                            path.extension().and_then(|e| e.to_str()).map(|e| e == "kicad_pro").unwrap_or(false)
                        }))
                        .default_file_filter("KiCad Project");
                    if let Some(ref dir) = services.config.preferred_projects_directory {
                        dialog = dialog.initial_directory(dir.clone());
                    }
                    panel_state.project_import_dialog = dialog;
                    panel_state.project_import_dialog.pick_file();
                }
            });

            let picked_label = modal.pcb_file_path.as_ref()
                .map(|p| p.with_extension("kicad_pro").file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_else(|| "Unknown file".into()))
                .unwrap_or_else(|| "No KiCad project file selected".into());
            ui.label(egui::RichText::new(&picked_label).small().monospace());

            ui.add_space(10.0);

            // Pedigree (read-only)
            egui::Grid::new("import_pedigree_grid")
                .num_columns(2)
                .spacing([12.0, 6.0])
                .show(ui, |ui| {
                    ui.label(egui::RichText::new("Author:").strong());
                    ui.label(modal.author.as_deref().unwrap_or("(not set in .kicad_pro)"));
                    ui.end_row();

                    ui.label(egui::RichText::new("Company:").strong());
                    ui.label(modal.company.as_deref().unwrap_or("(not set in .kicad_pro)"));
                    ui.end_row();
                });

            if !modal.missing_pedigree.is_empty() {
                ui.add_space(4.0);
                ui.colored_label(
                    egui::Color32::from_rgb(230, 200, 120),
                    format!("⚠ Missing: {} — set in KiCad → Project Properties.", modal.missing_pedigree.join(", "))
                );
            }

            ui.add_space(10.0);
            ui.label(egui::RichText::new("Editable fields").strong());
            egui::Grid::new("import_edit_grid")
                .num_columns(2)
                .spacing([12.0, 6.0])
                .show(ui, |ui| {
                    ui.label("Name:");
                    ui.text_edit_singleline(&mut modal.name);
                    ui.end_row();
                    ui.label("Tags:");
                    ui.text_edit_singleline(&mut modal.tags);
                    ui.end_row();
                });

            ui.add_space(6.0);
            ui.label("Description:");
            ui.add(egui::TextEdit::multiline(&mut modal.description)
                .desired_width(f32::INFINITY)
                .desired_rows(4));

            if let Some(ref err) = modal.error {
                ui.add_space(6.0);
                ui.colored_label(egui::Color32::from_rgb(245, 120, 140), err);
            }

            ui.add_space(12.0);
            ui.horizontal(|ui| {
                if ui.button("📥 Import").clicked() { trigger_import = true; }
                if ui.button("Cancel").clicked() { close = true; }
            });
        });

    if trigger_import {
        execute_project_import(panel_state, services);
    }
    if close {
        panel_state.project_import_modal = None;
        panel_state.project_import_last_picked = None;
    }
}

fn execute_project_import(panel_state: &mut ProjectsPanelState, services: &mut SharedServices) {
    let (pcb_path, name, description, tags_str) = match panel_state.project_import_modal.as_ref() {
        Some(m) => (m.pcb_file_path.clone(), m.name.clone(), m.description.clone(), m.tags.clone()),
        None => return,
    };
    if name.trim().is_empty() {
        if let Some(ref mut m) = panel_state.project_import_modal {
            m.error = Some("Name cannot be empty".into());
        }
        return;
    }
    let pcb_path = match pcb_path {
        Some(p) => p,
        None => {
            if let Some(ref mut m) = panel_state.project_import_modal {
                m.error = Some("Pick a .kicad_pro file first".into());
            }
            return;
        }
    };
    let tags: Vec<String> = tags_str
        .split(',')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();

    let logger_state = services.logger_state.clone();
    let log_colors = services.log_colors.clone();
    let logger = ReactiveEventLogger::with_colors(&logger_state, &log_colors);

    let bom_cell = services.bom_state.clone();
    let bom_components: Vec<crate::project_manager::bom::BomComponent> =
        if let Some(ref bom_state) = *bom_cell.lock() {
            bom_state.entries.iter().cloned().map(Into::into).collect()
        } else {
            Vec::new()
        };

    // Ensure ProjectManagerState is initialized so create_project can persist.
    if panel_state.project_manager_state.is_none() {
        let mut state = project_manager::ProjectManagerState::with_config(&services.config);
        if let Err(e) = state.initialize_database(&services.project_db) {
            logger.log_error(&format!("Failed to initialize project database: {}", e));
        }
        panel_state.project_manager_state = Some(state);
    }

    let result = panel_state.project_manager_state.as_mut().unwrap().create_project(
        name.clone(),
        description,
        pcb_path,
        tags,
        bom_components,
    );
    match result {
        Ok(id) => {
            logger.log_info(&format!("Imported project: {} (ID: {})", name, id));
            panel_state.project_import_modal = None;
            panel_state.project_import_last_picked = None;
        }
        Err(e) => {
            if let Some(ref mut m) = panel_state.project_import_modal {
                m.error = Some(format!("Import failed: {}", e));
            }
        }
    }
}

impl ReleaseModalState {
    /// Snapshot the values needed to execute the release, avoiding borrow
    /// conflicts with `panel_state.release_modal` during the call.
    fn clone_for_exec(&self) -> ReleaseModalSnapshot {
        ReleaseModalSnapshot {
            rev_tag: self.rev_tag.clone(),
            description: self.description.clone(),
            changes: self.changes.clone(),
            include_date_in_name: self.include_date_in_name,
            include_notes_in_zip: self.include_notes_in_zip,
            target: self.target,
        }
    }
}

struct ReleaseModalSnapshot {
    rev_tag: String,
    description: String,
    changes: String,
    include_date_in_name: bool,
    include_notes_in_zip: bool,
    target: Option<crate::vendor::VendorKind>,
}
