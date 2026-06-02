//! Browser-side Projects panel — talks to `cuforge-services` via
//! `cuforge_api::CuforgeApi`.
//!
//! Slice 4 + 5 of WASM-Phase-E. Projects list/create/edit/delete (slice 4)
//! plus inline expandable releases per project — upload/download/delete
//! (slice 5). Async API calls spawn via `wasm_bindgen_futures::spawn_local`
//! and deposit their results into an `Arc<Mutex<...>>` slot, mirroring the
//! upload pattern in `app.rs`.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use copperforge_core::cuforge_api::{
    ApiCallError, CuforgeApi, NewProject, NewRelease, Project, ProjectUpdate, Release,
};
use eframe::egui;
use uuid::Uuid;

use crate::app::LoadSlot;

/// Pick a sensible default backend URL based on the deployment domain:
/// running at `copperforge.dev` (or any subdomain) → assume the API is
/// at `api.copperforge.dev`; running anywhere else → assume local dev.
/// localStorage overrides this when the user has typed their own URL.
fn default_base_url() -> &'static str {
    let on_prod_domain = web_sys::window()
        .and_then(|w| w.location().hostname().ok())
        .map(|h| h.ends_with("copperforge.dev"))
        .unwrap_or(false);
    if on_prod_domain {
        "https://api.copperforge.dev"
    } else {
        "http://127.0.0.1:8421"
    }
}

/// localStorage key — survives across browser reloads / tab restarts but
/// is scoped to the origin (so dev `localhost:8080` and prod
/// `copperforge.dev` keep separate values, which is the right default).
const BASE_URL_STORAGE_KEY: &str = "copperforge.cuforge_services_url";

/// Read a string from `window.localStorage`. Returns `None` if storage
/// isn't available (private mode, sandboxed iframe) or the key is unset.
fn local_storage_get(key: &str) -> Option<String> {
    web_sys::window()
        .and_then(|w| w.local_storage().ok().flatten())
        .and_then(|s| s.get_item(key).ok().flatten())
        .filter(|s| !s.is_empty())
}

/// Best-effort write to `window.localStorage`. Failures are silent —
/// no UI surfacing because the only ways this fails are user-mode
/// (private browsing, quota) and don't break the app.
fn local_storage_set(key: &str, value: &str) {
    if let Some(storage) = web_sys::window().and_then(|w| w.local_storage().ok().flatten()) {
        let _ = storage.set_item(key, value);
    }
}

// ─── Slot shared with spawned futures ───────────────────────────────────────

#[derive(Default)]
struct Slot {
    /// `Some(_)` while a request is in flight.
    in_flight: Option<&'static str>,
    /// Result the UI loop drains on the next frame.
    pending: Option<Outcome>,
}

enum Outcome {
    Listed(Vec<Project>),
    Created(Project),
    Updated(Project),
    Deleted(Uuid),
    ReleasesListed {
        project_id: Uuid,
        releases: Vec<Release>,
    },
    /// New release created. `auto_view_bytes` is `Some((file_name, bytes))`
    /// if we should pipe the freshly-uploaded zip straight into the
    /// gerber canvas; `None` for paths that don't auto-view.
    ReleaseCreated {
        release: Release,
        auto_view_bytes: Option<(String, Vec<u8>)>,
    },
    /// File bytes streamed back from the server for a download. `intent`
    /// distinguishes "Save As" (browser download) from "View in viewer"
    /// (pipe into the gerber scene).
    ReleaseDownloaded {
        file_name: String,
        bytes: Vec<u8>,
        intent: DownloadIntent,
    },
    ReleaseDeleted {
        project_id: Uuid,
        release_id: Uuid,
    },
    Failed(String),
}

#[derive(Clone, Copy)]
enum DownloadIntent {
    SaveAs,
    ViewInViewer,
}

type SlotRef = Arc<Mutex<Slot>>;

// ─── Modal state ────────────────────────────────────────────────────────────

#[derive(Default)]
struct EditForm {
    mode: EditMode,
    name: String,
    description: String,
    author: String,
    pcb_path: String,
    /// Comma-separated tag input — split on save.
    tags_input: String,
    version: String,
    validation: Option<String>,
}

#[derive(Default, Clone, Copy)]
enum EditMode {
    #[default]
    Create,
    Update(Uuid),
}

impl EditForm {
    fn from_project(p: &Project) -> Self {
        Self {
            mode: EditMode::Update(p.id),
            name: p.name.clone(),
            description: p.description.clone(),
            author: p.author.clone(),
            pcb_path: p.pcb_path.clone(),
            tags_input: p.tags.join(", "),
            version: p.version.clone(),
            validation: None,
        }
    }

    fn parse_tags(&self) -> Vec<String> {
        self.tags_input
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect()
    }
}

/// Modal state for uploading a release zip.
struct ReleaseUploadForm {
    project_id: Uuid,
    revision: String,
    vendor: String,
    notes: String,
    /// `Some((file_name, bytes))` once the user has picked a file.
    picked: Option<(String, Vec<u8>)>,
    validation: Option<String>,
}

impl ReleaseUploadForm {
    fn new(project_id: Uuid) -> Self {
        Self {
            project_id,
            revision: String::new(),
            vendor: String::new(),
            notes: String::new(),
            picked: None,
            validation: None,
        }
    }
}

// ─── Panel ──────────────────────────────────────────────────────────────────

pub struct ProjectsPanel {
    api: CuforgeApi,
    slot: SlotRef,
    base_url_input: String,
    /// Shared with `WebApp::pending_load` — when the user uploads a
    /// release here, or clicks "View" on an existing release, we write
    /// the unzipped `LoadedRelease` into this slot and `WebApp`'s
    /// existing `drain_pending` picks it up, builds the gerber scene,
    /// updates the canvas. One pipeline, two entry points.
    load_slot: LoadSlot,
    /// Last known list of projects. Populated by `Outcome::Listed`.
    projects: Vec<Project>,
    /// Whether the panel has completed at least one successful
    /// `list_projects` for the current server URL. Used to gate the
    /// one-shot auto-fetch on first render so we DON'T re-fire every
    /// frame when the DB just happens to be empty (an empty list looks
    /// identical to "haven't loaded yet" if you only check `is_empty()`,
    /// which caused a per-frame request flood).
    initial_load_done: bool,
    /// `Some(msg)` if the last operation failed; cleared on next success.
    error: Option<String>,
    /// Highlighted row in the list.
    selected: Option<Uuid>,
    /// Currently-open edit/create modal.
    editing: Option<EditForm>,
    /// Project queued for delete confirmation.
    delete_confirm: Option<Uuid>,
    /// Result slot for an in-flight `rfd::AsyncFileDialog` pick. The
    /// future deposits the picked filename here; the modal drains it on
    /// the next frame into the open `EditForm`'s `pcb_path` field. None
    /// when nothing is in flight. Browser-side rfd only exposes the
    /// filename (no path — browser security model strips it before JS
    /// sees it), so this is filename-only by design.
    pcb_pick_slot: Arc<Mutex<Option<String>>>,
    // ── Slice 5: releases sub-view ──────────────────────────────────
    /// Which project's releases are currently expanded inline. None =
    /// all collapsed (the default — list reads as a flat catalog).
    expanded: Option<Uuid>,
    /// Per-project release cache. Populated by `Outcome::ReleasesListed`
    /// when a project is expanded; survives collapse/re-expand within
    /// the session so we don't re-hit the API on every toggle.
    releases_by_project: HashMap<Uuid, Vec<Release>>,
    /// Currently-open release upload modal.
    release_upload: Option<ReleaseUploadForm>,
    /// Release queued for delete confirmation: `(project_id, release_id, file_name)`.
    release_delete_confirm: Option<(Uuid, Uuid, String)>,
    /// Slot for an in-flight release-file pick. Future deposits
    /// `(file_name, bytes)`; the upload modal drains it next frame.
    release_pick_slot: Arc<Mutex<Option<(String, Vec<u8>)>>>,
}

impl ProjectsPanel {
    pub fn new(load_slot: LoadSlot) -> Self {
        // Persisted URL beats domain-derived default beats hardcoded
        // fallback. Power users keep whatever they typed; first-time
        // visitors get a sensible default for their environment.
        let base = local_storage_get(BASE_URL_STORAGE_KEY)
            .unwrap_or_else(|| default_base_url().to_string());
        Self::with_base_url(base, load_slot)
    }

    pub fn with_base_url(base_url: impl Into<String>, load_slot: LoadSlot) -> Self {
        let base_url = base_url.into();
        Self {
            api: CuforgeApi::new(&base_url),
            slot: SlotRef::default(),
            base_url_input: base_url,
            load_slot,
            projects: Vec::new(),
            initial_load_done: false,
            error: None,
            selected: None,
            editing: None,
            delete_confirm: None,
            pcb_pick_slot: Arc::new(Mutex::new(None)),
            expanded: None,
            releases_by_project: HashMap::new(),
            release_upload: None,
            release_delete_confirm: None,
            release_pick_slot: Arc::new(Mutex::new(None)),
        }
    }

    /// Trigger an immediate `list_projects` — called on first render so
    /// the panel populates without user interaction.
    pub fn refresh(&mut self, ctx: &egui::Context) {
        if self.start_request("list") {
            let api = self.api.clone();
            let slot = self.slot.clone();
            let ctx = ctx.clone();
            wasm_bindgen_futures::spawn_local(async move {
                let result = api.list_projects().await;
                deposit(
                    &slot,
                    match result {
                        Ok(projects) => Outcome::Listed(projects),
                        Err(e) => Outcome::Failed(format!("list: {e}")),
                    },
                );
                ctx.request_repaint();
            });
        }
    }

    /// Main render entry. Call from `WebApp::render_projects_tab`.
    pub fn show(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        self.drain_slot(&ctx);

        // Gate the one-shot initial fetch on `initial_load_done`, NOT on
        // `projects.is_empty()` — an empty server response looks
        // identical to "haven't loaded yet" by emptiness alone, which
        // would refire the request every frame forever.
        if !self.initial_load_done && !self.is_busy() && self.error.is_none() {
            self.refresh(&ctx);
        }

        self.toolbar(ui, &ctx);
        ui.separator();

        if let Some(msg) = &self.error {
            ui.colored_label(
                egui::Color32::from_rgb(220, 120, 120),
                format!("⚠ {msg}"),
            );
            ui.add_space(4.0);
        }

        // Render either the project list or the empty-state CTA — but
        // ALWAYS render the modals afterward. The previous early-return
        // skipped modal rendering whenever the list was empty, which
        // meant clicking ➕ New on an empty DB silently set
        // `editing = Some(form)` and then nothing drew the modal — the
        // user saw a dead button.
        if self.projects.is_empty() && !self.is_busy() {
            self.empty_state(ui, &ctx);
        } else {
            self.project_list(ui, &ctx);
        }

        self.edit_modal(&ctx);
        self.delete_modal(&ctx);
        self.release_upload_modal(&ctx);
        self.release_delete_modal(&ctx);
    }

    // ── Sub-renders ─────────────────────────────────────────────────────────

    fn toolbar(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new("Projects").heading());
            ui.add_space(8.0);

            if ui
                .button("🔄 Refresh")
                .on_hover_text("List projects from cuforge-services")
                .clicked()
            {
                self.refresh(ctx);
            }

            if ui.button("➕ New").clicked() {
                self.editing = Some(EditForm::default());
            }

            ui.add_space(8.0);
            let status = if self.is_busy() {
                "loading…".to_string()
            } else {
                format!("{} project(s)", self.projects.len())
            };
            ui.label(egui::RichText::new(status).weak().small());
        });

        ui.add_space(2.0);
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new("Server:").small().weak());
            let url_changed = ui
                .add(
                    egui::TextEdit::singleline(&mut self.base_url_input)
                        .desired_width(260.0)
                        .font(egui::TextStyle::Monospace),
                )
                .lost_focus()
                && ui.input(|i| i.key_pressed(egui::Key::Enter));
            if url_changed || ui.button("Apply").clicked() {
                let normalized =
                    self.base_url_input.trim().trim_end_matches('/').to_string();
                self.base_url_input = normalized.clone();
                self.api = CuforgeApi::new(&normalized);
                local_storage_set(BASE_URL_STORAGE_KEY, &normalized);
                self.projects.clear();
                self.releases_by_project.clear();
                self.expanded = None;
                // Force the auto-fetch to fire once against the new URL.
                self.initial_load_done = false;
                self.error = None;
                self.refresh(ctx);
            }
        });
    }

    fn empty_state(&mut self, ui: &mut egui::Ui, _ctx: &egui::Context) {
        ui.add_space(24.0);
        ui.vertical_centered(|ui| {
            ui.label(egui::RichText::new("No projects yet").size(18.0).strong());
            ui.add_space(6.0);
            ui.label(
                egui::RichText::new(
                    "Click ➕ New above to create your first project, or check the\n\
                     server URL if you expected projects to appear.",
                )
                .weak(),
            );
            ui.add_space(12.0);
            if ui.button("➕ Create first project").clicked() {
                self.editing = Some(EditForm::default());
            }
        });
    }

    fn project_list(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        // Defer state changes so we don't mutate `self` while the
        // closure still borrows `self.projects`. Each Option tracks one
        // possible action triggered by this frame's clicks.
        let mut toggle_expand: Option<Uuid> = None;
        let mut request_releases_for: Option<Uuid> = None;
        let mut open_upload_for: Option<Uuid> = None;
        let mut download_release: Option<(Uuid, String)> = None;
        let mut view_release: Option<(Uuid, String)> = None;
        let mut confirm_delete_release: Option<(Uuid, Uuid, String)> = None;
        let mut edit_project: Option<Project> = None;
        let mut delete_project: Option<Uuid> = None;

        egui::ScrollArea::vertical().show(ui, |ui| {
            // Take an owned clone of the list so the closure doesn't
            // alias `self`. Cheap — projects.len() is small.
            let rows: Vec<Project> = self.projects.clone();
            for p in rows {
                let selected = self.selected == Some(p.id);
                let expanded = self.expanded == Some(p.id);
                let arrow = if expanded { "▾" } else { "▸" };
                let label = egui::RichText::new(format!("{arrow}  {}", p.name)).strong();

                ui.horizontal(|ui| {
                    let resp = ui.add(egui::Button::selectable(selected, label));

                    if resp.clicked() {
                        self.selected = Some(p.id);
                        toggle_expand = Some(p.id);
                    }

                    // Visible Edit / Delete buttons — CRUD shouldn't be
                    // hidden behind a right-click context menu.
                    if ui
                        .small_button("✏")
                        .on_hover_text("Edit project metadata")
                        .clicked()
                    {
                        edit_project = Some(p.clone());
                    }
                    if ui
                        .small_button("🗑")
                        .on_hover_text("Delete project (and all its releases)")
                        .clicked()
                    {
                        delete_project = Some(p.id);
                    }

                    // Right-click on the row name still works for power
                    // users who prefer it.
                    resp.context_menu(|ui| {
                        if ui.button("✏ Edit").clicked() {
                            edit_project = Some(p.clone());
                            ui.close();
                        }
                        if ui.button("🗑 Delete…").clicked() {
                            delete_project = Some(p.id);
                            ui.close();
                        }
                    });
                });

                // Sub-line: author, version, updated_at — small + weak.
                ui.indent(p.id, |ui| {
                    ui.horizontal_wrapped(|ui| {
                        if !p.author.is_empty() {
                            ui.label(
                                egui::RichText::new(&p.author).small().weak(),
                            );
                        }
                        if !p.version.is_empty() {
                            ui.label(
                                egui::RichText::new(format!("v{}", p.version))
                                    .small()
                                    .weak(),
                            );
                        }
                        ui.label(
                            egui::RichText::new(format!(
                                "updated {}",
                                p.updated_at.format("%Y-%m-%d %H:%M UTC")
                            ))
                            .small()
                            .weak(),
                        );
                    });
                    if !p.tags.is_empty() {
                        ui.horizontal_wrapped(|ui| {
                            for tag in &p.tags {
                                ui.label(
                                    egui::RichText::new(format!("#{tag}"))
                                        .small()
                                        .color(egui::Color32::from_rgb(140, 180, 220)),
                                );
                            }
                        });
                    }
                    if !p.description.is_empty() {
                        ui.label(egui::RichText::new(&p.description).small());
                    }

                    // ── Releases section (only when expanded) ────────
                    if expanded {
                        ui.add_space(4.0);
                        ui.separator();
                        ui.horizontal(|ui| {
                            ui.label(
                                egui::RichText::new("Releases")
                                    .small()
                                    .strong()
                                    .color(egui::Color32::from_rgb(200, 200, 220)),
                            );
                            if ui
                                .small_button("📤 Upload release")
                                .on_hover_text("Upload a release zip for this project")
                                .clicked()
                            {
                                open_upload_for = Some(p.id);
                            }
                        });

                        match self.releases_by_project.get(&p.id) {
                            None => {
                                // First expansion — kick off the fetch.
                                request_releases_for = Some(p.id);
                                ui.label(
                                    egui::RichText::new("loading releases…")
                                        .small()
                                        .weak(),
                                );
                            }
                            Some(releases) if releases.is_empty() => {
                                ui.label(
                                    egui::RichText::new(
                                        "No releases yet. Click \
                                         📤 Upload release to add one.",
                                    )
                                    .small()
                                    .weak(),
                                );
                            }
                            Some(releases) => {
                                for r in releases {
                                    ui.horizontal(|ui| {
                                        // Tag-style revision label.
                                        ui.label(
                                            egui::RichText::new(format!("◆ {}", r.revision))
                                                .strong()
                                                .color(egui::Color32::from_rgb(220, 180, 100)),
                                        );
                                        if !r.vendor.is_empty() {
                                            ui.label(
                                                egui::RichText::new(format!("[{}]", r.vendor))
                                                    .small()
                                                    .color(egui::Color32::from_rgb(
                                                        180, 200, 220,
                                                    )),
                                            );
                                        }
                                        ui.label(
                                            egui::RichText::new(format_size(r.file_size))
                                                .small()
                                                .weak()
                                                .monospace(),
                                        );
                                        ui.label(
                                            egui::RichText::new(format!(
                                                "{}",
                                                r.created_at.format("%Y-%m-%d %H:%M UTC")
                                            ))
                                            .small()
                                            .weak(),
                                        );
                                        if ui
                                            .small_button("👁 View")
                                            .on_hover_text(
                                                "Load this release's gerbers \
                                                 into the Canvas",
                                            )
                                            .clicked()
                                        {
                                            view_release =
                                                Some((r.id, r.file_name.clone()));
                                        }
                                        if ui
                                            .small_button("📥")
                                            .on_hover_text("Download release zip")
                                            .clicked()
                                        {
                                            download_release =
                                                Some((r.id, r.file_name.clone()));
                                        }
                                        if ui
                                            .small_button("🗑")
                                            .on_hover_text("Delete this release")
                                            .clicked()
                                        {
                                            confirm_delete_release = Some((
                                                p.id,
                                                r.id,
                                                r.file_name.clone(),
                                            ));
                                        }
                                    });
                                    if !r.notes.is_empty() {
                                        ui.indent(r.id, |ui| {
                                            ui.label(
                                                egui::RichText::new(&r.notes).small().weak(),
                                            );
                                        });
                                    }
                                    ui.label(
                                        egui::RichText::new(format!(
                                            "sha256: {}…",
                                            &r.file_sha256[..16.min(r.file_sha256.len())]
                                        ))
                                        .small()
                                        .weak()
                                        .monospace()
                                        .color(egui::Color32::from_rgb(120, 130, 150)),
                                    );
                                    ui.add_space(2.0);
                                }
                            }
                        }
                    }
                });
                ui.add_space(6.0);
            }
        });

        // Apply deferred state changes.
        if let Some(p) = edit_project {
            self.editing = Some(EditForm::from_project(&p));
        }
        if let Some(id) = delete_project {
            self.delete_confirm = Some(id);
        }
        if let Some(id) = toggle_expand {
            self.expanded = if self.expanded == Some(id) { None } else { Some(id) };
        }
        if let Some(id) = request_releases_for {
            self.do_list_releases(ctx, id);
        }
        if let Some(id) = open_upload_for {
            self.release_upload = Some(ReleaseUploadForm::new(id));
        }
        if let Some((id, file_name)) = download_release {
            self.do_download_release(ctx, id, file_name, DownloadIntent::SaveAs);
        }
        if let Some((id, file_name)) = view_release {
            self.do_download_release(ctx, id, file_name, DownloadIntent::ViewInViewer);
        }
        if let Some(triple) = confirm_delete_release {
            self.release_delete_confirm = Some(triple);
        }
    }

    fn edit_modal(&mut self, ctx: &egui::Context) {
        // Drain any pending file-pick result into the open form before
        // rendering so the picked name shows up the same frame the
        // future completed.
        if let Ok(mut slot) = self.pcb_pick_slot.lock() {
            if let Some(picked) = slot.take() {
                if let Some(form) = self.editing.as_mut() {
                    form.pcb_path = picked;
                }
            }
        }

        let Some(form) = self.editing.as_mut() else {
            return;
        };

        let title = match form.mode {
            EditMode::Create => "New Project",
            EditMode::Update(_) => "Edit Project",
        };

        let mut open = true;
        let mut save_clicked = false;
        let mut cancel_clicked = false;
        let mut pick_clicked = false;

        egui::Window::new(title)
            .open(&mut open)
            .collapsible(false)
            .resizable(true)
            .default_width(440.0)
            .show(ctx, |ui| {
                egui::Grid::new("project_edit_grid")
                    .num_columns(2)
                    .spacing([12.0, 6.0])
                    .show(ui, |ui| {
                        ui.label("Name:");
                        ui.text_edit_singleline(&mut form.name);
                        ui.end_row();

                        ui.label("Description:");
                        ui.add(
                            egui::TextEdit::multiline(&mut form.description)
                                .desired_rows(3),
                        );
                        ui.end_row();

                        ui.label("Author:");
                        ui.text_edit_singleline(&mut form.author);
                        ui.end_row();

                        ui.label("PCB file:");
                        ui.horizontal(|ui| {
                            ui.add(
                                egui::TextEdit::singleline(&mut form.pcb_path)
                                    .hint_text("e.g. alpha_gan_sense.kicad_pcb")
                                    .desired_width(280.0),
                            );
                            if ui
                                .button("📁 Browse…")
                                .on_hover_text(
                                    "Browser security strips full paths — \
                                     this fills in the filename only.",
                                )
                                .clicked()
                            {
                                pick_clicked = true;
                            }
                        });
                        ui.end_row();

                        ui.label("Tags:");
                        ui.add(
                            egui::TextEdit::singleline(&mut form.tags_input)
                                .hint_text("comma-separated, e.g. fpga, oss"),
                        );
                        ui.end_row();

                        ui.label("Version:");
                        ui.text_edit_singleline(&mut form.version);
                        ui.end_row();
                    });

                if let Some(msg) = &form.validation {
                    ui.add_space(4.0);
                    ui.colored_label(egui::Color32::from_rgb(220, 120, 120), msg);
                }

                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    if ui.button("💾 Save").clicked() {
                        save_clicked = true;
                    }
                    if ui.button("Cancel").clicked() {
                        cancel_clicked = true;
                    }
                });
            });

        if pick_clicked {
            self.do_pick_pcb(ctx);
        }

        if cancel_clicked || !open {
            self.editing = None;
            return;
        }
        if !save_clicked {
            return;
        }

        // Validate locally before issuing the request.
        let Some(form) = self.editing.as_mut() else { return };
        if form.name.trim().is_empty() {
            form.validation = Some("Name must not be empty".to_string());
            return;
        }
        form.validation = None;
        let snapshot = std::mem::take(form);
        let mode = snapshot.mode;
        self.editing = None;

        match mode {
            EditMode::Create => self.do_create(ctx, &snapshot),
            EditMode::Update(id) => self.do_update(ctx, id, &snapshot),
        }
    }

    fn delete_modal(&mut self, ctx: &egui::Context) {
        let Some(id) = self.delete_confirm else { return };
        let target = self.projects.iter().find(|p| p.id == id).cloned();
        let Some(target) = target else {
            self.delete_confirm = None;
            return;
        };

        let mut open = true;
        let mut confirmed = false;
        let mut cancelled = false;

        egui::Window::new("Delete project?")
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .default_width(360.0)
            .show(ctx, |ui| {
                ui.label(format!("Delete '{}'?", target.name));
                ui.label(
                    egui::RichText::new(
                        "All releases and their files will also be removed.\n\
                         This can't be undone.",
                    )
                    .small()
                    .weak(),
                );
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    if ui
                        .button(
                            egui::RichText::new("🗑 Delete")
                                .color(egui::Color32::from_rgb(220, 120, 120)),
                        )
                        .clicked()
                    {
                        confirmed = true;
                    }
                    if ui.button("Cancel").clicked() {
                        cancelled = true;
                    }
                });
            });

        if cancelled || !open {
            self.delete_confirm = None;
            return;
        }
        if confirmed {
            self.delete_confirm = None;
            self.do_delete(ctx, id);
        }
    }

    // ── Async dispatch ──────────────────────────────────────────────────────

    fn do_create(&mut self, ctx: &egui::Context, form: &EditForm) {
        if !self.start_request("create") {
            return;
        }
        let req = NewProject {
            name: form.name.trim().to_string(),
            description: form.description.clone(),
            author: form.author.clone(),
            pcb_path: form.pcb_path.clone(),
            tags: form.parse_tags(),
            version: form.version.clone(),
        };
        let api = self.api.clone();
        let slot = self.slot.clone();
        let ctx = ctx.clone();
        wasm_bindgen_futures::spawn_local(async move {
            let result = api.create_project(&req).await;
            deposit(
                &slot,
                match result {
                    Ok(p) => Outcome::Created(p),
                    Err(e) => Outcome::Failed(api_error_msg("create", &e)),
                },
            );
            ctx.request_repaint();
        });
    }

    fn do_update(&mut self, ctx: &egui::Context, id: Uuid, form: &EditForm) {
        if !self.start_request("update") {
            return;
        }
        let patch = ProjectUpdate {
            name: Some(form.name.trim().to_string()),
            description: Some(form.description.clone()),
            author: Some(form.author.clone()),
            pcb_path: Some(form.pcb_path.clone()),
            tags: Some(form.parse_tags()),
            version: Some(form.version.clone()),
        };
        let api = self.api.clone();
        let slot = self.slot.clone();
        let ctx = ctx.clone();
        wasm_bindgen_futures::spawn_local(async move {
            let result = api.update_project(id, &patch).await;
            deposit(
                &slot,
                match result {
                    Ok(p) => Outcome::Updated(p),
                    Err(e) => Outcome::Failed(api_error_msg("update", &e)),
                },
            );
            ctx.request_repaint();
        });
    }

    // ── Release upload modal ────────────────────────────────────────────────

    fn release_upload_modal(&mut self, ctx: &egui::Context) {
        // Drain any in-flight file pick into the open form before
        // rendering so the picked file shows up the same frame it
        // landed.
        if let Ok(mut slot) = self.release_pick_slot.lock() {
            if let Some(picked) = slot.take() {
                if let Some(form) = self.release_upload.as_mut() {
                    form.picked = Some(picked);
                }
            }
        }

        let Some(form) = self.release_upload.as_mut() else {
            return;
        };

        let project_name = self
            .projects
            .iter()
            .find(|p| p.id == form.project_id)
            .map(|p| p.name.clone())
            .unwrap_or_else(|| "project".to_string());

        let mut open = true;
        let mut save_clicked = false;
        let mut cancel_clicked = false;
        let mut pick_clicked = false;

        egui::Window::new(format!("Upload release — {project_name}"))
            .open(&mut open)
            .collapsible(false)
            .resizable(true)
            .default_width(440.0)
            .show(ctx, |ui| {
                egui::Grid::new("release_upload_grid")
                    .num_columns(2)
                    .spacing([12.0, 6.0])
                    .show(ui, |ui| {
                        ui.label("Revision:");
                        ui.add(
                            egui::TextEdit::singleline(&mut form.revision)
                                .hint_text("e.g. v1.0 or 01June2026"),
                        );
                        ui.end_row();

                        ui.label("Vendor:");
                        ui.add(
                            egui::TextEdit::singleline(&mut form.vendor)
                                .hint_text("e.g. pcbway, jlcpcb, oshpark — free-form"),
                        );
                        ui.end_row();

                        ui.label("Notes:");
                        ui.add(
                            egui::TextEdit::multiline(&mut form.notes)
                                .desired_rows(3)
                                .hint_text("Optional release notes"),
                        );
                        ui.end_row();

                        ui.label("File:");
                        ui.horizontal(|ui| {
                            if ui
                                .button("📁 Pick .zip…")
                                .on_hover_text(
                                    "Pick a release archive (gerbers + drill bundle)",
                                )
                                .clicked()
                            {
                                pick_clicked = true;
                            }
                            match &form.picked {
                                Some((name, bytes)) => {
                                    ui.label(
                                        egui::RichText::new(format!(
                                            "{name}  ·  {}",
                                            format_size(bytes.len() as i64)
                                        ))
                                        .monospace()
                                        .small(),
                                    );
                                }
                                None => {
                                    ui.label(
                                        egui::RichText::new("(no file selected)")
                                            .small()
                                            .weak(),
                                    );
                                }
                            }
                        });
                        ui.end_row();
                    });

                if let Some(msg) = &form.validation {
                    ui.add_space(4.0);
                    ui.colored_label(egui::Color32::from_rgb(220, 120, 120), msg);
                }

                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    if ui.button("📤 Upload").clicked() {
                        save_clicked = true;
                    }
                    if ui.button("Cancel").clicked() {
                        cancel_clicked = true;
                    }
                });
            });

        if pick_clicked {
            self.do_pick_release_file(ctx);
        }

        if cancel_clicked || !open {
            self.release_upload = None;
            return;
        }
        if !save_clicked {
            return;
        }

        let Some(form) = self.release_upload.as_mut() else { return };
        if form.revision.trim().is_empty() {
            form.validation = Some("Revision must not be empty".to_string());
            return;
        }
        if form.picked.is_none() {
            form.validation = Some("Pick a file first".to_string());
            return;
        }
        form.validation = None;

        // Take ownership of the form so we can pass its picked bytes
        // into the upload future without copying.
        let form = self.release_upload.take().unwrap();
        let (file_name, bytes) = form.picked.unwrap();
        self.do_create_release(
            ctx,
            form.project_id,
            NewRelease {
                revision: form.revision,
                vendor: form.vendor,
                notes: form.notes,
            },
            file_name,
            bytes,
        );
    }

    fn release_delete_modal(&mut self, ctx: &egui::Context) {
        let Some((project_id, release_id, file_name)) =
            self.release_delete_confirm.clone()
        else {
            return;
        };

        let mut open = true;
        let mut confirmed = false;
        let mut cancelled = false;

        egui::Window::new("Delete release?")
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .default_width(360.0)
            .show(ctx, |ui| {
                ui.label(format!("Delete '{file_name}'?"));
                ui.label(
                    egui::RichText::new(
                        "The release file will be removed from the server. \
                         The project itself stays. This can't be undone.",
                    )
                    .small()
                    .weak(),
                );
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    if ui
                        .button(
                            egui::RichText::new("🗑 Delete")
                                .color(egui::Color32::from_rgb(220, 120, 120)),
                        )
                        .clicked()
                    {
                        confirmed = true;
                    }
                    if ui.button("Cancel").clicked() {
                        cancelled = true;
                    }
                });
            });

        if cancelled || !open {
            self.release_delete_confirm = None;
            return;
        }
        if confirmed {
            self.release_delete_confirm = None;
            self.do_delete_release(ctx, project_id, release_id);
        }
    }

    /// Open the browser file picker and write the selected file's name
    /// into `pcb_pick_slot`. The next `edit_modal` frame drains the
    /// slot into the open form's `pcb_path`.
    ///
    /// Filename-only by design: browsers don't expose filesystem paths
    /// to JS for privacy, so even if the user picks
    /// `/home/.../foo.kicad_pcb` we only see `foo.kicad_pcb`.
    fn do_pick_pcb(&self, ctx: &egui::Context) {
        let slot = self.pcb_pick_slot.clone();
        let ctx = ctx.clone();
        wasm_bindgen_futures::spawn_local(async move {
            let picked = rfd::AsyncFileDialog::new()
                .set_title("Select a KiCad PCB file")
                .add_filter("KiCad PCB", &["kicad_pcb"])
                .add_filter("KiCad schematic", &["kicad_sch"])
                .add_filter("All files", &["*"])
                .pick_file()
                .await;
            if let Some(handle) = picked {
                if let Ok(mut s) = slot.lock() {
                    *s = Some(handle.file_name());
                }
                ctx.request_repaint();
            }
        });
    }

    /// Open the browser file picker for a release zip and stash
    /// `(file_name, bytes)` in `release_pick_slot`. Next
    /// `release_upload_modal` frame drains it into the form.
    fn do_pick_release_file(&self, ctx: &egui::Context) {
        let slot = self.release_pick_slot.clone();
        let ctx = ctx.clone();
        wasm_bindgen_futures::spawn_local(async move {
            let picked = rfd::AsyncFileDialog::new()
                .set_title("Select a release zip")
                .add_filter("Release ZIP", &["zip"])
                .add_filter("All files", &["*"])
                .pick_file()
                .await;
            if let Some(handle) = picked {
                let bytes = handle.read().await;
                let file_name = handle.file_name();
                if let Ok(mut s) = slot.lock() {
                    *s = Some((file_name, bytes));
                }
                ctx.request_repaint();
            }
        });
    }

    fn do_list_releases(&mut self, ctx: &egui::Context, project_id: Uuid) {
        if !self.start_request("list_releases") {
            return;
        }
        let api = self.api.clone();
        let slot = self.slot.clone();
        let ctx = ctx.clone();
        wasm_bindgen_futures::spawn_local(async move {
            let result = api.list_releases(project_id).await;
            deposit(
                &slot,
                match result {
                    Ok(releases) => Outcome::ReleasesListed { project_id, releases },
                    Err(e) => Outcome::Failed(api_error_msg("list_releases", &e)),
                },
            );
            ctx.request_repaint();
        });
    }

    fn do_create_release(
        &mut self,
        ctx: &egui::Context,
        project_id: Uuid,
        metadata: NewRelease,
        file_name: String,
        bytes: Vec<u8>,
    ) {
        if !self.start_request("create_release") {
            return;
        }
        let api = self.api.clone();
        let slot = self.slot.clone();
        let ctx = ctx.clone();
        // Keep a clone of the bytes for auto-viewing in the gerber
        // canvas on successful upload. ~MB scale — fine on the heap for
        // the moments between upload and the next frame.
        let bytes_for_view = bytes.clone();
        let file_name_for_view = file_name.clone();
        wasm_bindgen_futures::spawn_local(async move {
            let result = api
                .create_release(project_id, &metadata, file_name, bytes)
                .await;
            deposit(
                &slot,
                match result {
                    Ok(r) => Outcome::ReleaseCreated {
                        release: r,
                        auto_view_bytes: Some((file_name_for_view, bytes_for_view)),
                    },
                    Err(e) => Outcome::Failed(api_error_msg("upload_release", &e)),
                },
            );
            ctx.request_repaint();
        });
    }

    /// Fetch the release file bytes, then either trigger a Save-As
    /// (`SaveAs`) or pipe them into the gerber canvas (`ViewInViewer`).
    /// The download/view trigger happens on the UI frame that drains
    /// the outcome — not from inside the future — so DOM operations
    /// (anchor click for save, repaint for view) run in the right context.
    fn do_download_release(
        &mut self,
        ctx: &egui::Context,
        release_id: Uuid,
        file_name: String,
        intent: DownloadIntent,
    ) {
        if !self.start_request("download_release") {
            return;
        }
        let api = self.api.clone();
        let slot = self.slot.clone();
        let ctx = ctx.clone();
        wasm_bindgen_futures::spawn_local(async move {
            let result = api.download_release(release_id).await;
            deposit(
                &slot,
                match result {
                    Ok(bytes) => Outcome::ReleaseDownloaded {
                        file_name,
                        bytes,
                        intent,
                    },
                    Err(e) => Outcome::Failed(api_error_msg("download_release", &e)),
                },
            );
            ctx.request_repaint();
        });
    }

    fn do_delete_release(
        &mut self,
        ctx: &egui::Context,
        project_id: Uuid,
        release_id: Uuid,
    ) {
        if !self.start_request("delete_release") {
            return;
        }
        let api = self.api.clone();
        let slot = self.slot.clone();
        let ctx = ctx.clone();
        wasm_bindgen_futures::spawn_local(async move {
            let result = api.delete_release(release_id).await;
            deposit(
                &slot,
                match result {
                    Ok(()) => Outcome::ReleaseDeleted { project_id, release_id },
                    Err(e) => Outcome::Failed(api_error_msg("delete_release", &e)),
                },
            );
            ctx.request_repaint();
        });
    }

    fn do_delete(&mut self, ctx: &egui::Context, id: Uuid) {
        if !self.start_request("delete") {
            return;
        }
        let api = self.api.clone();
        let slot = self.slot.clone();
        let ctx = ctx.clone();
        wasm_bindgen_futures::spawn_local(async move {
            let result = api.delete_project(id).await;
            deposit(
                &slot,
                match result {
                    Ok(()) => Outcome::Deleted(id),
                    Err(e) => Outcome::Failed(api_error_msg("delete", &e)),
                },
            );
            ctx.request_repaint();
        });
    }

    // ── Slot bookkeeping ────────────────────────────────────────────────────

    fn is_busy(&self) -> bool {
        self.slot
            .lock()
            .map(|s| s.in_flight.is_some())
            .unwrap_or(false)
    }

    /// Mark an operation in-flight. Returns false if another request is
    /// already running (UI buttons re-disabled until it lands).
    fn start_request(&self, kind: &'static str) -> bool {
        let Ok(mut s) = self.slot.lock() else {
            return false;
        };
        if s.in_flight.is_some() {
            return false;
        }
        s.in_flight = Some(kind);
        true
    }

    fn drain_slot(&mut self, ctx: &egui::Context) {
        let Ok(mut s) = self.slot.lock() else { return };
        let Some(outcome) = s.pending.take() else {
            return;
        };
        s.in_flight = None;
        drop(s);

        match outcome {
            Outcome::Listed(projects) => {
                self.projects = projects;
                self.initial_load_done = true;
                self.error = None;
                // Selected row may have been removed server-side.
                if let Some(id) = self.selected {
                    if !self.projects.iter().any(|p| p.id == id) {
                        self.selected = None;
                    }
                }
            }
            Outcome::Created(p) => {
                self.projects.insert(0, p);
                self.error = None;
            }
            Outcome::Updated(p) => {
                if let Some(slot) = self.projects.iter_mut().find(|x| x.id == p.id) {
                    *slot = p;
                } else {
                    self.projects.insert(0, p);
                }
                self.error = None;
            }
            Outcome::Deleted(id) => {
                self.projects.retain(|p| p.id != id);
                if self.selected == Some(id) {
                    self.selected = None;
                }
                if self.expanded == Some(id) {
                    self.expanded = None;
                }
                self.releases_by_project.remove(&id);
                self.error = None;
            }
            Outcome::ReleasesListed { project_id, releases } => {
                self.releases_by_project.insert(project_id, releases);
                self.error = None;
            }
            Outcome::ReleaseCreated { release, auto_view_bytes } => {
                let entry = self
                    .releases_by_project
                    .entry(release.project_id)
                    .or_default();
                entry.insert(0, release);
                self.error = None;
                // Auto-load the just-uploaded release into the gerber
                // canvas — same pipeline as Upload Release ZIP. The
                // bytes were stashed pre-upload so we can pipe them
                // through without re-downloading from the server.
                if let Some((name, bytes)) = auto_view_bytes {
                    self.pipe_into_viewer(name, bytes, ctx);
                }
            }
            Outcome::ReleaseDownloaded { file_name, bytes, intent } => match intent {
                DownloadIntent::SaveAs => {
                    if let Err(e) =
                        crate::release_pkg::trigger_download(&file_name, &bytes)
                    {
                        self.error = Some(format!("download trigger: {e}"));
                    } else {
                        self.error = None;
                    }
                }
                DownloadIntent::ViewInViewer => {
                    self.pipe_into_viewer(file_name, bytes, ctx);
                    self.error = None;
                }
            },
            Outcome::ReleaseDeleted { project_id, release_id } => {
                if let Some(list) = self.releases_by_project.get_mut(&project_id) {
                    list.retain(|r| r.id != release_id);
                }
                self.error = None;
            }
            Outcome::Failed(msg) => {
                self.error = Some(msg);
            }
        }
    }
}

impl ProjectsPanel {
    /// Drop a release zip into the gerber-canvas pipeline. The unzip
    /// runs synchronously here (deflate is fast — even a 50 MB release
    /// is sub-second), then we write the `LoadedRelease` to the shared
    /// `load_slot` and `WebApp::drain_pending` picks it up on its next
    /// frame, building scene/centroid/BOM exactly as if the user had
    /// clicked Upload Release ZIP from the toolbar.
    fn pipe_into_viewer(&self, file_name: String, bytes: Vec<u8>, ctx: &egui::Context) {
        let result = crate::app::unzip_release(file_name, bytes);
        if let Ok(mut slot) = self.load_slot.lock() {
            *slot = Some(result);
        }
        // Without this, the gerber canvas wouldn't update until the
        // user moved the mouse — `WebApp::drain_pending` only runs
        // when egui renders a frame, and a frame is only scheduled on
        // input or an explicit repaint request.
        ctx.request_repaint();
    }
}

/// Pretty-print a byte count: `< 1 KB → "B"`, `< 1 MB → "KB"`, else "MB".
/// One decimal place for the larger units; integer for bytes.
fn format_size(n: i64) -> String {
    let n = n.max(0) as f64;
    if n < 1024.0 {
        format!("{n:.0} B")
    } else if n < 1024.0 * 1024.0 {
        format!("{:.1} KB", n / 1024.0)
    } else {
        format!("{:.2} MB", n / (1024.0 * 1024.0))
    }
}


// ─── Free helpers ───────────────────────────────────────────────────────────

fn deposit(slot: &SlotRef, outcome: Outcome) {
    if let Ok(mut s) = slot.lock() {
        s.pending = Some(outcome);
    }
}

fn api_error_msg(verb: &str, e: &ApiCallError) -> String {
    match e {
        ApiCallError::Server { status, error } => {
            format!("{verb}: HTTP {status} [{}] {}", error.code, error.message)
        }
        ApiCallError::Status { status, body } => {
            format!("{verb}: HTTP {status} {body}")
        }
        ApiCallError::Transport(msg) => format!("{verb}: {msg}"),
    }
}
