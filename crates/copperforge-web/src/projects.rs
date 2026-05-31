//! Browser-side Projects panel — talks to `cuforge-services` via
//! `cuforge_api::CuforgeApi`.
//!
//! Slice 4 of WASM-Phase-E. List + create + edit + delete; releases
//! sub-view comes in a follow-up slice. Async API calls spawn via
//! `wasm_bindgen_futures::spawn_local` and deposit their results into
//! an `Arc<Mutex<...>>` slot, mirroring the upload pattern in `app.rs`.

use std::sync::{Arc, Mutex};

use copperforge_core::cuforge_api::{
    ApiCallError, CuforgeApi, NewProject, Project, ProjectUpdate,
};
use eframe::egui;
use uuid::Uuid;

const DEFAULT_BASE_URL: &str = "http://127.0.0.1:8421";

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
    Failed(String),
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

// ─── Panel ──────────────────────────────────────────────────────────────────

pub struct ProjectsPanel {
    api: CuforgeApi,
    slot: SlotRef,
    base_url_input: String,
    /// Last known list of projects. Populated by `Outcome::Listed`.
    projects: Vec<Project>,
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
}

impl ProjectsPanel {
    pub fn new() -> Self {
        // Persisted URL beats the hardcoded default so power users don't
        // have to re-type their server every reload.
        let base = local_storage_get(BASE_URL_STORAGE_KEY)
            .unwrap_or_else(|| DEFAULT_BASE_URL.to_string());
        Self::with_base_url(base)
    }

    pub fn with_base_url(base_url: impl Into<String>) -> Self {
        let base_url = base_url.into();
        Self {
            api: CuforgeApi::new(&base_url),
            slot: SlotRef::default(),
            base_url_input: base_url,
            projects: Vec::new(),
            error: None,
            selected: None,
            editing: None,
            delete_confirm: None,
            pcb_pick_slot: Arc::new(Mutex::new(None)),
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
        self.drain_slot();

        let ctx = ui.ctx().clone();
        let initial_load_needed = self.projects.is_empty()
            && self.error.is_none()
            && !self.is_busy();
        if initial_load_needed {
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

        if self.projects.is_empty() && !self.is_busy() {
            self.empty_state(ui, &ctx);
            return;
        }

        self.project_list(ui, &ctx);
        self.edit_modal(&ctx);
        self.delete_modal(&ctx);
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
        egui::ScrollArea::vertical().show(ui, |ui| {
            // Take an owned clone of the list so the closure doesn't
            // alias `self`. Cheap — projects.len() is small.
            let rows: Vec<Project> = self.projects.clone();
            for p in rows {
                let selected = self.selected == Some(p.id);
                let label = egui::RichText::new(&p.name).strong();

                let resp = ui.add(egui::Button::selectable(selected, label));

                if resp.clicked() {
                    self.selected = Some(p.id);
                }

                resp.context_menu(|ui| {
                    if ui.button("✏ Edit").clicked() {
                        self.editing = Some(EditForm::from_project(&p));
                        ui.close();
                    }
                    if ui.button("🗑 Delete…").clicked() {
                        self.delete_confirm = Some(p.id);
                        ui.close();
                    }
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
                });
                ui.add_space(4.0);
            }
            let _ = ctx; // future use — context menus may need it
        });
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

    fn drain_slot(&mut self) {
        let Ok(mut s) = self.slot.lock() else { return };
        let Some(outcome) = s.pending.take() else {
            return;
        };
        s.in_flight = None;
        drop(s);

        match outcome {
            Outcome::Listed(projects) => {
                self.projects = projects;
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
                self.error = None;
            }
            Outcome::Failed(msg) => {
                self.error = Some(msg);
            }
        }
    }
}

impl Default for ProjectsPanel {
    fn default() -> Self {
        Self::new()
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
