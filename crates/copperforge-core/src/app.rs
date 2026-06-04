use std::{fs, path::PathBuf, sync::Arc};

use eframe::emath::{Rect, Vec2};
use egui::Pos2;
use egui_dock::{DockArea, DockState, NodeIndex, Style, SurfaceIndex};

use crate::display;
use crate::display::DisplayManager;
use crate::drc_operations::DrcManager;

use crate::event_logger::{ReactiveEventLogger, ReactiveEventLoggerState, LogColors};
use egui_mobius_reactive::*;
use gerber_viewer::{
   BoundingBox,
   ViewState, UiState, GerberTransform
};
use crate::platform::parameters::gui::VERSION;
use crate::project;
use crate::ui;
use crate::project_manager;
use crate::services::SharedServices;

use crate::ui::{Tab, TabKind, TabViewer, initialize_and_show_banner, show_system_info};

use crate::project::{load_demo_gerber, ProjectState, manager::ProjectConfig};
use crate::display::GridSettings;

/// A single discovered (or user-configured) kicad-cli install. Stored on
/// SharedServices so the settings modal can list every working option.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KicadCandidate {
    /// Method key. One of `path`, `path-nightly`, `flatpak`, `snap`, or
    /// `custom:/abs/path/to/kicad-cli`. Persisted in `ProjectConfig.kicad_cli_override`.
    pub method: String,
    /// Human-readable label for the selector UI.
    pub label: String,
    /// Reported version, e.g. `"10.0.2"` or `"10.99.0 (nightly)"`.
    pub version: String,
}

fn kicad_method_label(method: &str) -> String {
    if let Some(path) = method.strip_prefix("custom:") {
        return format!("Custom path ({})", path);
    }
    match method {
        "path" => "Native on PATH (kicad-cli)".into(),
        "path-nightly" => "Native nightly on PATH (kicad-cli-nightly)".into(),
        "flatpak" => "Flatpak (org.kicad.KiCad)".into(),
        "snap" => "Snap (kicad.kicad-cli)".into(),
        other => other.into(),
    }
}

/// The main application struct. A thin outer layer around `SharedServices`;
/// panels operate on services, never directly on fields of this struct.
pub struct CopperForgeApp {
    /// All cross-panel state lives here.
    pub services: SharedServices,

    // ── Citizen infrastructure ────────────────────────────────
    pub dispatcher: egui_citizen::Dispatcher,
    pub app_messages: Vec<crate::messages::AppMessage>,

    // ── Dock state (eframe owns) ──────────────────────────────
    dock_state: DockState<Tab>,

    // ── File dialogs (I/O handles) ────────────────────────────
    pub pcb_file_dialog: egui_file_dialog::FileDialog,
    pub last_picked_pcb_file: Option<PathBuf>,
    pub projects_directory_dialog: egui_file_dialog::FileDialog,
    pub last_picked_projects_directory: Option<PathBuf>,

    // ── Persisted citizen panels ───────────────────────────────
    /// Persisted so panel-local state (BOM cache, terminal buffer, shell
    /// history, etc.) survives across frames. Stateless panels (DRC,
    /// ViewSettings, Project, Projects, Settings) are still created fresh
    /// per frame in tabs.rs since they have nothing to carry.
    pub bom_panel: crate::panels::BomPanel,
    pub terminal_panel: crate::panels::TerminalPanel,
    pub logger_panel: crate::panels::LoggerPanel,
    pub gerber_view_3d_panel: crate::panels::GerberView3dPanel,

    // ── Render backend handles ────────────────────────────────
    /// Cached OpenGL context (from eframe's glow backend). Stashed on the
    /// first `update()` call so 3D panels can reach it without threading
    /// `&mut eframe::Frame` through `TabViewer`.
    pub gl_context: Option<Arc<glow::Context>>,

    // ── Projects panel (PM = paid tier-2) ────────────────────────
    /// Stored citizen owning its `ProjectsPanelState`. Registered with a
    /// real `CitizenState` (not `::default()`). Held as a named field for
    /// now like the other stored panels; lifts into `copperforge-pro` via
    /// the dock registry in the next step.
    pub projects_panel: crate::panels::ProjectsPanel,

    /// Panels contributed by external crates (e.g. `copperforge-pro`),
    /// registered via [`CopperForgeApp::register_panel`]. Dispatched
    /// through the `DockPanel` trait — core never names them. Empty in
    /// the free build.
    pub plugin_panels: Vec<Box<dyn crate::dock_panel::DockPanel>>,
}

/// Persistent state for the Projects panel (the paid tier-2 PM feature).
/// Grouped here so the whole PM surface walks out of `CopperForgeApp`
/// together when the Projects citizen is lifted into `copperforge-pro`.
pub struct ProjectsPanelState {
    pub project_manager_state: Option<project_manager::ProjectManagerState>,

    // ── Modal states (UI ephemeral) ─────────────────────────────
    pub release_modal: Option<ReleaseModalState>,
    /// Read-only Release Details modal — opened by right-click →
    /// "ℹ View Release Info" on a rev node. Holds a cloned Release.
    pub release_info_modal: Option<crate::release::Release>,
    /// Confirmation modal for "🗑 Delete Release". Seeded by the
    /// right-click intent; Confirm runs the DB + disk + cache delete.
    pub delete_release_confirmation: Option<DeleteReleaseConfirmation>,
    pub project_edit_modal: Option<ProjectEditModalState>,
    pub project_import_modal: Option<ProjectImportModalState>,
    /// File dialog used exclusively by the Project Import modal (kept so
    /// state survives across frames while the dialog is open).
    pub project_import_dialog: egui_file_dialog::FileDialog,
    pub project_import_last_picked: Option<std::path::PathBuf>,
}

impl Default for ProjectsPanelState {
    fn default() -> Self {
        Self {
            project_manager_state: None,
            release_modal: None,
            release_info_modal: None,
            delete_release_confirmation: None,
            project_edit_modal: None,
            project_import_modal: None,
            project_import_dialog: egui_file_dialog::FileDialog::new(),
            project_import_last_picked: None,
        }
    }
}

/// Form state for the Project Import modal (opened from the Projects tab's
/// top-bar "Import KiCad Project" button).
pub struct ProjectImportModalState {
    pub pcb_file_path: Option<std::path::PathBuf>, // derived .kicad_pcb
    pub name: String,
    pub description: String,
    pub tags: String,
    /// Read-only, auto-populated from .kicad_pro metadata.
    pub author: Option<String>,
    pub company: Option<String>,
    pub missing_pedigree: Vec<&'static str>,
    pub error: Option<String>,
}

/// Form state for the Project Edit modal (opened from the Projects tab via
/// right-click → Update).
pub struct ProjectEditModalState {
    pub project_id: String,
    pub name: String,
    pub description: String,
    pub tags: String,
    // Snapshot of read-only fields for display:
    pub author: Option<String>,
    pub company: Option<String>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub last_modified: chrono::DateTime<chrono::Utc>,
    pub pcb_file_path: std::path::PathBuf,
    pub releases: Vec<crate::release::Release>,
    pub error: Option<String>,
}

/// Form state for the Delete Release confirmation modal. Seeded from
/// the right-click intent's composite id; carries enough info to run
/// the delete (project_id + rev_tag for the DB; archive_path for the
/// disk dir + cache lookup) and to display in the confirmation window.
pub struct DeleteReleaseConfirmation {
    pub project_id: String,
    pub rev_tag: String,
    pub archive_path: std::path::PathBuf,
    /// Populated if the delete attempt failed; shown in the modal.
    pub error: Option<String>,
}

/// Form state for the Release modal.
pub struct ReleaseModalState {
    pub rev_tag: String,
    pub description: String,
    pub changes: String,
    pub include_date_in_name: bool,
    pub include_notes_in_zip: bool,
    pub error: Option<String>,
    /// True when opened via right-click → Regenerate on an existing rev.
    /// Skips the tag-collision check and updates the existing Release entry
    /// in place rather than appending a new one.
    pub overwrite_existing: bool,
    /// Vendor target. `None` = standard release; `Some(...)` triggers
    /// vendor-specific extras during `create_release` (e.g. PCBWay's
    /// fab-specs README). Set when the modal is opened via the vendor
    /// button (e.g. "🏭 Release for PCBWay").
    pub target: Option<crate::vendor::VendorKind>,
}

impl Drop for CopperForgeApp {
    fn drop(&mut self) {
        self.save_dock_state();
        self.save_settings();
    }
}

/// Delete a release: DB record (entry in `project.releases`) + the
/// on-disk `outputs/<rev>/` folder + any cached extracted gerbers.
/// Free function so it doesn't conflict with `&mut self` of the
/// caller (modal-render method); takes the disjoint fields it needs.
fn delete_release_artifacts(
    project_db: &project_manager::database::ProjectDatabase,
    pm_state: Option<&mut project_manager::ProjectManagerState>,
    project_id: &str,
    rev_tag: &str,
    archive_path: &std::path::Path,
    logger: &ReactiveEventLogger,
) -> Result<(), String> {
    // 1. DB: load → mutate → save.
    let mut project = project_db
        .load_project(project_id)
        .map_err(|e| format!("load_project: {e}"))?
        .ok_or_else(|| format!("project '{project_id}' not found in DB"))?;
    let before = project.releases.len();
    project.releases.retain(|r| r.tag != rev_tag);
    if project.releases.len() == before {
        logger.log_warning(&format!(
            "Release '{rev_tag}' not found in DB record for '{project_id}' (already gone?)"
        ));
    }
    project_db
        .save_project(&project)
        .map_err(|e| format!("save_project: {e}"))?;

    // 2. In-memory cache (so the tree refreshes immediately).
    if let Some(pm) = pm_state {
        if let Some(releases) = pm.project_releases.get_mut(project_id) {
            releases.retain(|r| r.tag != rev_tag);
        }
        if let Some(cp) = pm.current_project.as_mut() {
            if cp.metadata.id == project_id {
                cp.releases.retain(|r| r.tag != rev_tag);
            }
        }
    }

    // 3. Disk: the outputs/<rev>/ folder.
    if let Some(rev_dir) = archive_path.parent() {
        match std::fs::remove_dir_all(rev_dir) {
            Ok(()) => logger.log_info(&format!("Removed {}", rev_dir.display())),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                logger.log_warning(&format!("{} was already gone", rev_dir.display()));
            }
            Err(e) => {
                logger.log_warning(&format!(
                    "Could not remove {}: {} (DB entry still removed)",
                    rev_dir.display(),
                    e
                ));
            }
        }
    }

    // 4. Cached extract under the user cache dir (if it exists).
    if let Some(stem) = archive_path.file_stem() {
        if let Some(cache_dir) = dirs::cache_dir() {
            let cached = cache_dir.join("copperforge").join("extracted").join(stem);
            if cached.exists() {
                let _ = std::fs::remove_dir_all(&cached);
            }
        }
    }

    Ok(())
}

/// Panic with a lengthy, stage-aware diagnostic when init fails.
fn panic_init(stage: &str, err: impl std::fmt::Display, hints: &[&str]) -> ! {
    eprintln!();
    eprintln!("============================================================");
    eprintln!("  CopperForge — initialization failure");
    eprintln!("============================================================");
    eprintln!();
    eprintln!("  Stage: {stage}");
    eprintln!("  Error: {err}");
    eprintln!();
    if !hints.is_empty() {
        eprintln!("  Hints:");
        for h in hints {
            eprintln!("    • {h}");
        }
        eprintln!();
    }
    eprintln!("  If this persists, capture the above and file an issue:");
    eprintln!("    https://github.com/Atlantix-EDA/CopperForge/issues");
    eprintln!("============================================================");
    eprintln!();
    panic!("CopperForge init failed at stage: {stage}");
}

impl CopperForgeApp {
    pub fn sync_units_to_ecs(&mut self) {
        if self.services.global_units_mils {
            self.services.layer_store.units.display_unit = crate::layer_store::DisplayUnit::Mils;
        } else {
            self.services.layer_store.units.display_unit = crate::layer_store::DisplayUnit::Millimeters;
        }
    }

    pub fn sync_units_from_ecs(&mut self) {
        self.services.global_units_mils = self.services.layer_store.units.is_mils();
    }

    pub fn sync_zoom_to_ecs(&mut self) {
        self.services.layer_store.zoom.set_scale(self.services.view_state.scale);
        self.services.layer_store.zoom.center_x = self.services.view_state.translation.x;
        self.services.layer_store.zoom.center_y = self.services.view_state.translation.y;
    }

    pub fn sync_zoom_from_ecs(&mut self) {
        self.services.view_state.scale = self.services.layer_store.zoom.scale;
        self.services.view_state.translation.x = self.services.layer_store.zoom.center_x;
        self.services.view_state.translation.y = self.services.layer_store.zoom.center_y;
    }

    pub fn render_layers_ecs(&mut self, painter: &egui::Painter) {
        let view_state = self.services.view_state;
        let rotation = self.services.rotation_degrees;
        self.services.layer_store.render(painter, view_state, &self.services.display_manager, rotation);
    }

    pub fn new() -> Self {
        let dummy_viewport = Rect::from_min_size(Pos2::ZERO, Vec2::new(1280.0, 768.0));

        // ── Stage 1: LoadConfig ──────────────────────────────────
        let config_path: PathBuf = dirs::config_dir()
            .map(|d| d.join("copperforge"))
            .unwrap_or_else(|| {
                panic_init(
                    "LoadConfig",
                    "dirs::config_dir() returned None",
                    &["The OS didn't expose a config directory (XDG_CONFIG_HOME / %APPDATA%).",
                      "Set XDG_CONFIG_HOME to a writable path and retry."],
                )
            });
        let config = match ProjectConfig::load_from_file(&config_path) {
            Ok(cfg) => cfg,
            Err(e) => panic_init(
                "LoadConfig",
                e,
                &[
                    "project_config.json exists but failed to deserialize.",
                    "If the file is from an older CopperForge with incompatible schema,",
                    "delete it: rm ~/.config/copperforge/project_config.json",
                    "(CopperForge will recreate it with defaults on next launch.)",
                ],
            ),
        };

        // ── Stage 2: DiscoverKiCad (slow on first Flatpak launch) ─
        let (kicad_version, kicad_cli_method, _kicad_candidates) = Self::probe_kicad_cli(&config);

        // ── Stage 3: InitializeDb ────────────────────────────────
        // Single-file redb DB; requires exclusive file-level lock so only one
        // CopperForge instance can run at a time. Old sled directory at
        // ~/.config/copperforge/projects.db/ (if present from pre-0.4.0) is
        // harmless; manual deletion reclaims disk space.
        let db_path = config_path.join("projects.redb");
        let project_db = match crate::project_manager::database::ProjectDatabase::new(&db_path) {
            Ok(db) => db,
            Err(e) => panic_init(
                "InitializeDb",
                e,
                &[
                    &format!("Failed to open redb database at {}", db_path.display()),
                    "Another CopperForge process is holding a lock on the file.",
                    "  pgrep copperforge       (find running instances)",
                    "  pkill copperforge       (kill them)",
                    "Or if no other instance is running (prior crash left a stale lock),",
                    "delete the file and restart — any imported projects will need re-importing:",
                    "  rm ~/.config/copperforge/projects.redb",
                ],
            ),
        };

        // ── Stage 4: Wire SharedServices ─────────────────────────
        let mut initial_logger_state = ReactiveEventLoggerState::new();
        initial_logger_state.show_timestamps = false;
        let logger_state = Dynamic::new(initial_logger_state);
        let log_colors = Dynamic::new(LogColors::default());
        let project_state = Dynamic::new(config.state.clone());
        let bom_state = Dynamic::new(None);

        let mut layer_store = crate::layer_store::LayerStore::default();
        if config.global_units_mils {
            layer_store.units.display_unit = crate::layer_store::DisplayUnit::Mils;
        } else {
            layer_store.units.display_unit = crate::layer_store::DisplayUnit::Millimeters;
        }

        let services = SharedServices {
            project_state,
            bom_state,
            logger_state,
            log_colors,
            config_path: config_path.clone(),
            kicad_version,
            kicad_cli_method,
            project_db,
            layer_store,
            gerber_layer: load_demo_gerber(),
            view_state: ViewState::default(),
            ui_state: UiState::default(),
            needs_initial_view: true,
            rotation_degrees: 0.0,
            board_outline: None,
            top_copper: None,
            bottom_copper: None,
            top_mask: None,
            bottom_mask: None,
            display_manager: DisplayManager::new(),
            drc_manager: DrcManager::new(),
            grid_settings: GridSettings::default(),
            global_units_mils: config.global_units_mils,
            user_timezone: config.user_timezone.clone(),
            use_24_hour_clock: config.use_24_hour_clock,
            zoom_window_start: None,
            zoom_window_dragging: false,
            setting_origin_mode: false,
            origin_has_been_set: false,
            ruler_active: false,
            ruler_start: None,
            ruler_end: None,
            ruler_dragging: false,
            ruler_drag_start: None,
            latched_measurement_start: None,
            latched_measurement_end: None,
            show_about_modal: false,
            show_kicad_version_modal: false,
            show_cuforge_services_modal: false,
            bom_component_count: 0,
            cuforge_status: egui_mobius_reactive::Dynamic::new(
                crate::cuforge_client::CuforgeStatus::Unknown,
            ),
            config,
        };

        // ── Stage 5: Register citizens ───────────────────────────
        let mut dispatcher = egui_citizen::Dispatcher::new();
        use egui_citizen::message::CitizenId;
        for id in [
            "gerber_view", "gerber_view_3d", "view_settings", "drc",
            "settings", "bom",
            "terminal", "logger",
        ] {
            dispatcher.register(CitizenId::new(id));
        }
        // Projects is a stored citizen — capture its registered CitizenState
        // (NOT ::default(), which severs the reactive link with the
        // dispatcher) and hand it to the panel below.
        let projects_citizen_state = dispatcher.register(CitizenId::new("projects"));
        dispatcher.activate(&CitizenId::new("gerber_view"));
        let _ = dispatcher.drain_messages();

        let dock_state = Self::create_default_dock_state();

        let mut app = Self {
            services,
            dispatcher,
            app_messages: Vec::new(),
            dock_state,
            pcb_file_dialog: egui_file_dialog::FileDialog::new(),
            last_picked_pcb_file: None,
            projects_directory_dialog: egui_file_dialog::FileDialog::new(),
            last_picked_projects_directory: None,
            bom_panel: crate::panels::BomPanel::new(egui_citizen::CitizenState::default()),
            terminal_panel: crate::panels::TerminalPanel::new(egui_citizen::CitizenState::default()),
            logger_panel: crate::panels::LoggerPanel::new(egui_citizen::CitizenState::default()),
            gerber_view_3d_panel: crate::panels::GerberView3dPanel::new(egui_citizen::CitizenState::default()),
            gl_context: None,
            projects_panel: crate::panels::ProjectsPanel::new(projects_citizen_state),
            plugin_panels: Vec::new(),
        };

        let logger = ReactiveEventLogger::with_colors(&app.services.logger_state, &app.services.log_colors);
        initialize_and_show_banner(&logger);
        app.prune_stale_project_state();

        app.reset_view(dummy_viewport);

        app
    }

    /// Prune project_state if its on-disk artifacts no longer exist.
    fn prune_stale_project_state(&mut self) {
        let current = self.services.project_state.get();
        let logger = ReactiveEventLogger::with_colors(&self.services.logger_state, &self.services.log_colors);
        match &current {
            ProjectState::NoProject => {
                logger.log_info("No previous project found. Please select a PCB file.");
            }
            ProjectState::PcbSelected { pcb_path }
            | ProjectState::GeneratingGerbers { pcb_path } => {
                if !pcb_path.exists() {
                    self.services.project_state.set(ProjectState::NoProject);
                }
            }
            ProjectState::GerbersGenerated { pcb_path, gerber_dir }
            | ProjectState::Ready { pcb_path, gerber_dir, .. }
            | ProjectState::LoadingGerbers { pcb_path, gerber_dir } => {
                if !pcb_path.exists() || !gerber_dir.exists() {
                    self.services.project_state.set(ProjectState::NoProject);
                }
            }
        }
    }

    pub fn reset_view(&mut self, viewport: Rect) {
        let combined_bbox = self.services.layer_store.combined_bounding_box();
        let bbox = combined_bbox.unwrap_or_else(|| self.services.gerber_layer.bounding_box().clone());
        let content_width = bbox.width();
        let content_height = bbox.height();

        let scale = f32::min(
            viewport.width() / (content_width as f32),
            viewport.height() / (content_height as f32),
        );
        let scale = scale * 0.95;

        if self.services.display_manager.design_offset.x != 0.0 || self.services.display_manager.design_offset.y != 0.0 {
            let origin_screen_x = viewport.left() + viewport.width() * 0.2;
            let origin_screen_y = viewport.bottom() - viewport.height() * 0.2;

            let origin_gerber_x = self.services.display_manager.design_offset.x;
            let origin_gerber_y = self.services.display_manager.design_offset.y;

            self.services.view_state.translation = Vec2::new(
                origin_screen_x - (origin_gerber_x as f32 * scale),
                origin_screen_y + (origin_gerber_y as f32 * scale),
            );
        } else {
            let gerber_center = bbox.center();

            self.services.display_manager.center_offset = display::VectorOffset {
                x: -gerber_center.x,
                y: -gerber_center.y,
            };

            let origin: nalgebra::Vector2<f64> = self.services.display_manager.center_offset.clone().into();
            let offset: nalgebra::Vector2<f64> = self.services.display_manager.design_offset.clone().into();
            let transform = GerberTransform {
                rotation: self.services.rotation_degrees.to_radians(),
                mirroring: self.services.display_manager.mirroring.clone().into(),
                origin: origin - offset,
                offset,
                scale: 1.0,
            };

            let outline_vertices: Vec<_> = bbox
                .vertices()
                .into_iter()
                .map(|v| transform.apply_to_position(v))
                .collect();

            let transformed_bbox = BoundingBox::from_points(&outline_vertices);
            let transformed_center = transformed_bbox.center();

            self.services.view_state.translation = Vec2::new(
                viewport.center().x - (transformed_center.x as f32 * scale),
                viewport.center().y + (transformed_center.y as f32 * scale),
            );
        }

        self.services.view_state.scale = scale;

        self.services.layer_store.zoom.set_scale(scale);
        self.services.layer_store.zoom.set_fit_to_view_scale(scale);
        self.services.layer_store.zoom.center_x = self.services.view_state.translation.x;
        self.services.layer_store.zoom.center_y = self.services.view_state.translation.y;

        self.services.needs_initial_view = false;
    }

    /// Zoom to a specific BOM component location
    pub fn zoom_to_component(&mut self, component: &project_manager::bom::BomComponent, viewport: Rect) {
        if !self.services.origin_has_been_set {
            let logger = ReactiveEventLogger::with_colors(&self.services.logger_state, &self.services.log_colors);
            logger.log_warning("Please set the origin before using cross-probing");
            return;
        }

        let comp_x = component.x_location;
        let comp_y = component.y_location;

        let viewport_center = viewport.center();
        self.services.view_state.translation = Vec2::new(
            viewport_center.x - (comp_x as f32 * self.services.view_state.scale),
            viewport_center.y + (comp_y as f32 * self.services.view_state.scale),
        );

        self.services.layer_store.zoom.center_x = self.services.view_state.translation.x;
        self.services.layer_store.zoom.center_y = self.services.view_state.translation.y;

        let logger = ReactiveEventLogger::with_colors(&self.services.logger_state, &self.services.log_colors);
        logger.log_info(&format!("Cross-probed to component: {} at ({:.2}, {:.2})",
                                component.reference, comp_x, comp_y));
    }

    /// Open the PCB file dialog.
    pub fn open_pcb_file_dialog(&mut self) {
        self.pcb_file_dialog.pick_file();
    }

    /// Poll the PCB file dialog. On new pick of a .kicad_pcb, updates
    /// `project_state` to `PcbSelected` and returns the path.
    pub fn update_pcb_file_dialog(&mut self, ctx: &egui::Context) -> Option<PathBuf> {
        if let Some(path) = self.pcb_file_dialog.update(ctx).picked() {
            let path_buf = path.to_path_buf();
            if self.last_picked_pcb_file.as_ref() != Some(&path_buf) {
                self.last_picked_pcb_file = Some(path_buf.clone());
                if path.extension().and_then(|s| s.to_str()) == Some("kicad_pcb") {
                    self.services.project_state.set(ProjectState::PcbSelected { pcb_path: path_buf.clone() });
                    return Some(path_buf);
                }
            }
        }
        None
    }

    fn show_clock_display(&mut self, ui: &mut egui::Ui) {
        use chrono::{Local, Utc};
        use chrono_tz::Tz;

        if ui.button(egui::RichText::new(format!("CopperForge v{}", VERSION))
            .color(egui::Color32::from_rgb(180, 200, 255))).clicked() {
            self.services.show_about_modal = true;
        }

        ui.separator();

        let kicad_text = if let Some(ref version) = self.services.kicad_version {
            format!("KiCad {}", version)
        } else {
            "KiCad (not found)".to_string()
        };

        if ui.button(egui::RichText::new(kicad_text)
            .color(egui::Color32::from_rgb(180, 255, 200))).clicked() {
            self.services.show_kicad_version_modal = true;
        }

        ui.separator();

        let time_format = if self.services.use_24_hour_clock { "%H:%M:%S" } else { "%I:%M:%S %p" };
        let date_format = "%Y-%m-%d";

        let clock_text = if let Some(tz_name) = &self.services.user_timezone {
            if let Ok(tz) = tz_name.parse::<Tz>() {
                let now = Utc::now().with_timezone(&tz);
                format!("{} 🕐 {} {}", now.format(date_format), now.format(time_format), tz.name())
            } else {
                let now = Local::now();
                format!("{} 🕐 {}", now.format(date_format), now.format(time_format))
            }
        } else {
            let now = Local::now();
            format!("{} 🕐 {}", now.format(date_format), now.format(time_format))
        };

        ui.label(egui::RichText::new(clock_text).color(egui::Color32::from_rgb(220, 220, 220)));
    }

    /// One-shot KiCad discovery + version parse. Returns (version_string, method).
    /// Resolve the active kicad-cli at startup. Honors `config.kicad_cli_override`
    /// if it points at a still-working install; otherwise picks the preferred
    /// candidate from discovery (stable PATH → flatpak → snap → nightly PATH).
    ///
    /// Returns `(version_label, method, all_discovered_candidates)`. The list
    /// is the full set of working installs (plus a probed custom path if the
    /// override is one) so the settings UI can offer them as choices.
    pub fn probe_kicad_cli(config: &ProjectConfig) -> (Option<String>, Option<String>, Vec<KicadCandidate>) {
        let mut candidates = Self::discover_kicad_clis();

        if let Some(override_method) = config.kicad_cli_override.as_deref() {
            let already_known = candidates.iter().any(|c| c.method == override_method);
            if !already_known {
                if let Some(custom) = Self::probe_kicad_candidate(override_method) {
                    candidates.push(custom);
                }
            }
            if let Some(picked) = candidates.iter().find(|c| c.method == override_method) {
                return (Some(picked.version.clone()), Some(picked.method.clone()), candidates);
            }
            // Override pointed at something broken; fall through to default pick
            // and let the modal show the user what's actually available.
        }

        let picked = candidates.first().cloned();
        let version = picked.as_ref().map(|c| c.version.clone());
        let method = picked.map(|c| c.method);
        (version, method, candidates)
    }

    /// Build a `kicad-cli` Command using the cached discovery method — no probe.
    pub fn kicad_cli_command(&self) -> Option<std::process::Command> {
        self.services.kicad_cli_method.as_deref().map(Self::build_kicad_cli_command)
    }

    pub fn build_kicad_cli_command(method: &str) -> std::process::Command {
        use std::process::Command;
        if let Some(path) = method.strip_prefix("custom:") {
            return Command::new(path);
        }
        match method {
            "flatpak" => {
                let mut cmd = Command::new("flatpak");
                cmd.args(["run", "--command=kicad-cli", "org.kicad.KiCad"]);
                cmd
            }
            "snap" => {
                let mut cmd = Command::new("snap");
                cmd.args(["run", "kicad.kicad-cli"]);
                cmd
            }
            "path-nightly" => Command::new("kicad-cli-nightly"),
            _ => Command::new("kicad-cli"),
        }
    }

    /// Probe a single method key by running `<cmd> --version`. Returns a
    /// populated KicadCandidate iff the binary exists and reports a version.
    pub fn probe_kicad_candidate(method: &str) -> Option<KicadCandidate> {
        let mut cmd = Self::build_kicad_cli_command(method);
        let output = cmd.arg("--version").output().ok()?;
        if !output.status.success() {
            return None;
        }
        let is_nightly = method == "path-nightly";
        let raw = Self::parse_kicad_version(&output.stdout, is_nightly)?;
        let version = match method {
            "path" | "path-nightly" => raw,
            other => format!("{} ({})", raw, other.strip_prefix("custom:").unwrap_or(other)),
        };
        Some(KicadCandidate {
            method: method.to_string(),
            label: kicad_method_label(method),
            version,
        })
    }

    /// Probe every well-known install location and return one entry per
    /// working install. Order is the default preference: stable PATH →
    /// flatpak → snap → nightly PATH. The settings UI lets the user pick
    /// a different one if they want.
    pub fn discover_kicad_clis() -> Vec<KicadCandidate> {
        ["path", "flatpak", "snap", "path-nightly"]
            .iter()
            .filter_map(|m| Self::probe_kicad_candidate(m))
            .collect()
    }

    fn parse_kicad_version(stdout: &[u8], nightly: bool) -> Option<String> {
        let version_str = String::from_utf8_lossy(stdout);
        let line = version_str.lines().next()?;
        let mut version = if line.contains("kicad-cli") {
            line.split_whitespace().nth(1)?.to_string()
        } else {
            line.trim().to_string()
        };
        if nightly {
            version.push_str(" (nightly)");
        }
        Some(version)
    }

    fn render_kicad_info_modal(&self, ui: &mut egui::Ui) {
        ui.vertical_centered(|ui| {
            ui.heading("KiCad PCB Design Software");
            ui.add_space(10.0);

            if let Some(ref version) = self.services.kicad_version {
                ui.label(egui::RichText::new(format!("Version: {}", version))
                    .size(16.0)
                    .strong());
            } else {
                ui.label(egui::RichText::new("KiCad not detected on system")
                    .size(14.0)
                    .color(egui::Color32::from_rgb(255, 200, 100)));
            }

            ui.add_space(15.0);
            ui.separator();
            ui.add_space(15.0);

            ui.label("KiCad is a free and open-source electronics design automation (EDA) suite.");
            ui.label("It features schematic capture, integrated circuit simulation,");
            ui.label("printed circuit board (PCB) layout, 3D viewing, and SPICE simulation.");

            ui.add_space(10.0);

            ui.hyperlink_to("🌐 Visit KiCad.org", "https://www.kicad.org/");
            ui.hyperlink_to("📖 Documentation", "https://docs.kicad.org/");
            ui.hyperlink_to("💬 Forums", "https://forum.kicad.info/");
        });
    }
}

impl CopperForgeApp {
    fn save_dock_state(&self) {
        if let Some(config_dir) = dirs::config_dir() {
            let copperforge_dir = config_dir.join("copperforge");
            if let Err(e) = fs::create_dir_all(&copperforge_dir) {
                eprintln!("Failed to create config directory: {}", e);
                return;
            }
            let config_path = copperforge_dir.join("dock_state.json");
            match serde_json::to_string_pretty(&self.dock_state) {
                Ok(json) => {
                    if let Err(e) = fs::write(&config_path, json) {
                        eprintln!("Failed to write dock state: {}", e);
                    }
                }
                Err(e) => {
                    eprintln!("Failed to serialize dock state: {}", e);
                }
            }
        }
    }

    fn load_dock_state() -> Option<DockState<Tab>> {
        if let Some(config_dir) = dirs::config_dir() {
            let config_path = config_dir.join("copperforge").join("dock_state.json");
            if let Ok(json) = fs::read_to_string(&config_path) {
                match serde_json::from_str::<DockState<Tab>>(&json) {
                    Ok(mut dock_state) => {
                        // Plugin tabs are contributed at runtime by
                        // `register_panel`. Never restore persisted ones —
                        // otherwise each launch loads the saved tab AND adds
                        // a fresh one, duplicating it. Strip them here so
                        // registration is the single source of plugin tabs.
                        dock_state.retain_tabs(|tab| !matches!(&tab.kind, TabKind::Plugin(_)));

                        // Migration: if the saved layout predates a newer TabKind
                        // variant, reset so the default layout reinstates it.
                        // Update this list whenever a tab is added that users
                        // with an older config wouldn't otherwise see.
                        let required: &[TabKind] = &[TabKind::GerberView3d];
                        let missing = required.iter().any(|needed| {
                            !dock_state
                                .iter_all_tabs()
                                .any(|(_, tab)| std::mem::discriminant(&tab.kind)
                                    == std::mem::discriminant(needed))
                        });
                        if missing {
                            eprintln!("dock_state.json is missing a newer tab — resetting to defaults");
                            fs::remove_file(&config_path).ok();
                            return None;
                        }
                        return Some(dock_state);
                    }
                    Err(e) => {
                        eprintln!("Failed to deserialize dock state: {}", e);
                        fs::remove_file(config_path).ok();
                    }
                }
            }
        }
        None
    }

    pub fn save_settings(&self) {
        let mut config = self.services.config.clone();
        config.state = self.services.project_state.get();
        config.user_timezone = self.services.user_timezone.clone();
        config.use_24_hour_clock = self.services.use_24_hour_clock;
        config.global_units_mils = self.services.global_units_mils;

        // Author / company / kiverse settings are now sourced from defaults in
        // ProjectConfig directly (set via the Shell `set` command or edited in
        // ~/.config/copperforge/project_config.json). They're not mutated here.

        if let Err(e) = config.save_to_file(&self.services.config_path) {
            eprintln!("Failed to save settings: {}", e);
        }
    }

    /// Register an external panel (the plug-in seam). The panel is added
    /// to `plugin_panels` and a tab for it is pushed into the dock. Core
    /// dispatches to it via the `DockPanel` trait without naming it — this
    /// is how `copperforge-pro` contributes its private panels.
    pub fn register_panel(&mut self, panel: Box<dyn crate::dock_panel::DockPanel>) {
        let idx = self.plugin_panels.len();
        self.plugin_panels.push(panel);
        let tab = Tab::new(TabKind::Plugin(idx), SurfaceIndex::main(), NodeIndex(0));
        self.dock_state.main_surface_mut().push_to_first_leaf(tab);
    }

    fn create_default_dock_state() -> DockState<Tab> {
        if let Some(saved_dock_state) = Self::load_dock_state() {
            return saved_dock_state;
        }

        let gerber_tab = Tab::new(TabKind::GerberView, SurfaceIndex::main(), NodeIndex(0));
        let gerber_3d_tab = Tab::new(TabKind::GerberView3d, SurfaceIndex::main(), NodeIndex(0));
        let drc_tab = Tab::new(TabKind::DRC, SurfaceIndex::main(), NodeIndex(1));
        let view_settings_tab = Tab::new(TabKind::ViewSettings, SurfaceIndex::main(), NodeIndex(2));

        let settings_tab = Tab::new(TabKind::Settings, SurfaceIndex::main(), NodeIndex(3));
        let projects_tab = Tab::new(TabKind::Projects, SurfaceIndex::main(), NodeIndex(4));

        let logger_tab = Tab::new(TabKind::Logger, SurfaceIndex::main(), NodeIndex(5));
        let terminal_tab = Tab::new(TabKind::Terminal, SurfaceIndex::main(), NodeIndex(6));
        let bom_tab = Tab::new(TabKind::BOM, SurfaceIndex::main(), NodeIndex(7));

        // Gerber 2D + 3D share the main pane as sibling tabs.
        let mut dock_state = DockState::new(vec![gerber_tab, gerber_3d_tab]);
        let surface = dock_state.main_surface_mut();

        // Projects + Settings share the left column top; there's no separate
        // "Project" tab anymore — Import is a button inside the Projects tab.
        let [left, right] = surface.split_left(
            NodeIndex::root(),
            0.3,
            vec![projects_tab, settings_tab],
        );

        let _ = left; // no bottom split; Projects already uses the full left.
        surface.split_below(right, 0.5, vec![logger_tab, terminal_tab, bom_tab]);
        surface.split_right(right, 0.5, vec![drc_tab, view_settings_tab]);
        dock_state
    }
}

impl eframe::App for CopperForgeApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // First-frame: spawn the cuforge-services health poller. Single
        // background thread; updates self.services.cuforge_status and
        // calls ctx.request_repaint() on every status change.
        use std::sync::OnceLock;
        static CUFORGE_POLLER: OnceLock<()> = OnceLock::new();
        CUFORGE_POLLER.get_or_init(|| {
            crate::cuforge_client::spawn_health_poller(
                crate::cuforge_client::base_url(),
                self.services.cuforge_status.clone(),
                ctx.clone(),
            );
        });

        // Cache the glow context once — it lives for the app's lifetime, so
        // subsequent frames skip this. Panels reach it via `app.gl_context`.
        if self.gl_context.is_none() {
            self.gl_context = _frame.gl().cloned();
        }

        let show_system_info_clicked = ctx.memory(|mem| {
            mem.data.get_temp::<bool>(egui::Id::new("show_system_info")).unwrap_or(false)
        });

        if show_system_info_clicked {
            ctx.memory_mut(|mem| {
                mem.data.remove::<bool>(egui::Id::new("show_system_info"));
            });
            let logger = ReactiveEventLogger::with_colors(&self.services.logger_state, &self.services.log_colors);
            show_system_info(&logger);
        }

        if self.services.layer_store.is_dirty() {
            self.services.layer_store.mark_clean();
        }

        // Hotkeys (only when no text field has focus).
        //
        // Tab routing: keys that mean different things in the 2D gerber view
        // (`F` = flip, `R` = rotate, `M` = measure, `A` = align-to-grid) vs
        // the 3D view (planned: `F` = flip, `R` = 90° in-plane, `M` = 3D
        // ruler) must not fire simultaneously on both. The active tab is
        // tracked by egui_citizen — on_tab_button calls
        // `dispatcher.activate()`, which flips the one-hot active bit on
        // the matching `CitizenState`. Here we read that bit to gate the
        // 2D handlers so hitting F while the 3D tab is active doesn't
        // silently flip the 2D gerber behind it. When 3D F/R/M handlers
        // land they gate on the inverse of the same check.
        let text_input_active = ctx.memory(|mem| mem.focused().is_some());
        let three_d_active = self
            .dispatcher
            .get(&egui_citizen::message::CitizenId::new("gerber_view_3d"))
            .map(|s| s.active.get())
            .unwrap_or(false);
        let two_d_view_active = !three_d_active;

        if !text_input_active {
            ctx.input(|i| {
                if two_d_view_active && i.key_pressed(egui::Key::F) {
                    self.services.display_manager.showing_top = !self.services.display_manager.showing_top;

                    use crate::layer_store::{LayerType, Side};
                    for layer_type in LayerType::all() {
                        let visible = match layer_type {
                            LayerType::Copper(1)
                            | LayerType::Silkscreen(Side::Top)
                            | LayerType::Soldermask(Side::Top)
                            | LayerType::Paste(Side::Top)
                            | LayerType::ViaPlugging(Side::Top) => {
                                self.services.display_manager.showing_top
                            }
                            LayerType::Copper(_) => !self.services.display_manager.showing_top,
                            LayerType::Silkscreen(Side::Bottom)
                            | LayerType::Soldermask(Side::Bottom)
                            | LayerType::Paste(Side::Bottom)
                            | LayerType::ViaPlugging(Side::Bottom) => {
                                !self.services.display_manager.showing_top
                            }
                            LayerType::MechanicalOutline
                            | LayerType::Drill
                            | LayerType::UserLayer(_) => {
                                self.services.layer_store.get_visibility(layer_type)
                            }
                        };
                        self.services.layer_store.set_visibility(layer_type, visible);
                    }

                    let view_name = if self.services.display_manager.showing_top { "top" } else { "bottom" };
                    let logger = ReactiveEventLogger::with_colors(&self.services.logger_state, &self.services.log_colors);
                    logger.log_info(&format!("Flipped to {} view (F key)", view_name));
                    self.services.layer_store.mark_dirty();
                }

                if i.key_pressed(egui::Key::U) {
                    self.services.global_units_mils = !self.services.global_units_mils;
                    self.sync_units_to_ecs();
                    let units_name = if self.services.global_units_mils { "mils" } else { "mm" };
                    let logger = ReactiveEventLogger::with_colors(&self.services.logger_state, &self.services.log_colors);
                    logger.log_info(&format!("Toggled units to {} (U key)", units_name));
                }

                if two_d_view_active && i.key_pressed(egui::Key::R) {
                    self.services.rotation_degrees = (self.services.rotation_degrees + 90.0) % 360.0;
                    self.services.layer_store.mark_dirty();

                    let logger = ReactiveEventLogger::with_colors(&self.services.logger_state, &self.services.log_colors);
                    logger.log_custom(
                        project::constants::LOG_TYPE_ROTATION,
                        &format!("Rotated board to {:.0}° (R key)", self.services.rotation_degrees)
                    );
                }

                if two_d_view_active && i.key_pressed(egui::Key::A) {
                    display::align_to_grid(&mut self.services.view_state, &self.services.grid_settings);
                    let logger = ReactiveEventLogger::with_colors(&self.services.logger_state, &self.services.log_colors);
                    logger.log_info("Aligned view to grid (A key)");
                }

                if two_d_view_active && i.key_pressed(egui::Key::M) {
                    if self.services.ruler_active {
                        if self.services.ruler_start.is_some() && self.services.ruler_end.is_some() {
                            self.services.latched_measurement_start = self.services.ruler_start;
                            self.services.latched_measurement_end = self.services.ruler_end;
                        }
                        self.services.ruler_active = false;
                        self.services.ruler_start = None;
                        self.services.ruler_end = None;
                        self.services.ruler_dragging = false;

                        let logger = ReactiveEventLogger::with_colors(&self.services.logger_state, &self.services.log_colors);
                        logger.log_info("Ruler mode deactivated (M key) - measurement latched");
                    } else {
                        self.services.latched_measurement_start = None;
                        self.services.latched_measurement_end = None;
                        self.services.ruler_active = true;

                        let logger = ReactiveEventLogger::with_colors(&self.services.logger_state, &self.services.log_colors);
                        logger.log_info("Ruler mode activated (M key) - previous measurement cleared");
                    }
                }

                // ── 3D-tab hotkeys ────────────────────────────────
                if three_d_active {
                    if i.key_pressed(egui::Key::F) {
                        self.gerber_view_3d_panel.flip_view();
                        let logger = ReactiveEventLogger::with_colors(&self.services.logger_state, &self.services.log_colors);
                        logger.log_info("3D view flipped (F key)");
                    }
                    if i.key_pressed(egui::Key::R) {
                        self.gerber_view_3d_panel.rotate_in_plane_90();
                        let logger = ReactiveEventLogger::with_colors(&self.services.logger_state, &self.services.log_colors);
                        logger.log_info("3D view rotated 90° in-plane (R key)");
                    }
                    if i.key_pressed(egui::Key::M) {
                        let now_active = self.gerber_view_3d_panel.toggle_measure();
                        let logger = ReactiveEventLogger::with_colors(&self.services.logger_state, &self.services.log_colors);
                        logger.log_info(if now_active {
                            "3D measure mode activated (M key) — left-drag to measure"
                        } else {
                            "3D measure mode exited (M key)"
                        });
                    }
                }

                if i.key_pressed(egui::Key::Escape) && self.services.ruler_active {
                    if self.services.ruler_start.is_some() && self.services.ruler_end.is_some() {
                        self.services.latched_measurement_start = self.services.ruler_start;
                        self.services.latched_measurement_end = self.services.ruler_end;

                        let logger = ReactiveEventLogger::with_colors(&self.services.logger_state, &self.services.log_colors);
                        if let (Some(start), Some(end)) = (self.services.ruler_start, self.services.ruler_end) {
                            logger.log_info(&format!("Latching measurement - Start: ({:.6}, {:.6}), End: ({:.6}, {:.6})",
                                                    start.x, start.y, end.x, end.y));
                            let dx = end.x - start.x;
                            let dy = end.y - start.y;
                            logger.log_info(&format!("Latching deltas - ΔX: {:.6}, ΔY: {:.6}", dx, dy));
                        }
                    }

                    self.services.ruler_active = false;
                    self.services.ruler_start = None;
                    self.services.ruler_end = None;
                    self.services.ruler_dragging = false;

                    let logger = ReactiveEventLogger::with_colors(&self.services.logger_state, &self.services.log_colors);
                    logger.log_info("Ruler mode cancelled (ESC key) - measurement latched");
                }
            });
        }

        // Project Ribbon at the top
        egui::TopBottomPanel::top("project_ribbon").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 10.0;

                // Prominent current-project indicator on the far left — so a
                // cold start with a remembered PCB is obvious at a glance.
                let state = self.services.project_state.get();
                let (project_label, project_color) = match state.pcb_path() {
                    Some(p) => (
                        format!("📄 {}", p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| p.display().to_string())),
                        egui::Color32::from_rgb(180, 220, 180),
                    ),
                    None => (
                        "📄 (no project loaded)".to_string(),
                        egui::Color32::from_rgb(140, 140, 140),
                    ),
                };
                // Wrap in a group so vertical centering matches the other ribbon widgets.
                ui.group(|ui| {
                    ui.label(egui::RichText::new(project_label).color(project_color).strong());
                });

                ui.group(|ui| {
                    ui.horizontal(|ui| {
                        ui.label("📁 KiCad PCB File:");

                        if ui.button("Browse...").clicked() {
                            self.open_pcb_file_dialog();
                        }

                        if let Some(path_buf) = self.update_pcb_file_dialog(ui.ctx()) {
                            let logger = ReactiveEventLogger::with_colors(&self.services.logger_state, &self.services.log_colors);
                            logger.log_info(&format!("Selected PCB file: {}", path_buf.display()));
                        }
                    });
                });

                // Right-aligned section: cuforge-services status indicator
                // first (rightmost, OS-statusbar style — click for details
                // modal), then the clock, then Hotkeys (further left).
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if crate::cuforge_client::show_status_indicator(
                        ui,
                        &self.services.cuforge_status,
                    )
                    .clicked()
                    {
                        self.services.show_cuforge_services_modal = true;
                    }
                    ui.separator();
                    self.show_clock_display(ui);
                    ui.separator();
                    ui.menu_button("📋 Hotkeys", |ui| {
                        ui.heading("Keyboard Shortcuts");
                        ui.separator();

                        for (key, desc) in [
                            ("F", "Flip Top/Bottom view"),
                            ("R", "Rotate 90° clockwise"),
                            ("U", "Toggle units (mm/mils)"),
                            ("A", "Align view to grid"),
                            ("M", "Toggle ruler/measurement mode"),
                            ("ESC", "Cancel measurement mode"),
                        ] {
                            ui.horizontal(|ui| {
                                ui.label(key);
                                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                    ui.label(desc);
                                });
                            });
                        }

                        ui.separator();
                        ui.heading("Mouse Controls");

                        for (key, desc) in [
                            ("Double-click", "Center view"),
                            ("Right-click + drag", "Zoom to selection"),
                            ("Scroll wheel", "Zoom in/out"),
                            ("Left-click + drag", "Pan view"),
                            ("Escape", "Cancel zoom selection / measurement mode"),
                        ] {
                            ui.horizontal(|ui| {
                                ui.label(key);
                                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                    ui.label(desc);
                                });
                            });
                        }
                    });
                });
            });
        });

        // Main dock area below the ribbon
        let mut dock_state = self.dock_state.clone();
        let mut dispatcher = std::mem::take(&mut self.dispatcher);
        {
            let mut tab_viewer = TabViewer {
                app: self,
                dispatcher: &mut dispatcher,
            };
            let mut style = Style::from_egui(ctx.style().as_ref());
            style.dock_area_padding = None;
            style.tab_bar.fill_tab_bar = true;

            DockArea::new(&mut dock_state)
                .style(style)
                .show_add_buttons(true)
                .show_close_buttons(true)
                .show(ctx, &mut tab_viewer);
        }

        for msg in dispatcher.drain_messages() {
            self.app_messages.push(crate::messages::AppMessage::Citizen(msg));
        }
        self.dispatcher = dispatcher;
        self.dock_state = dock_state;

        crate::cuforge_client::show_modal_if_open(
            ctx,
            &mut self.services.show_cuforge_services_modal,
            &self.services.cuforge_status,
        );

        if self.services.show_about_modal {
            egui::Window::new("About CopperForge")
                .collapsible(false)
                .resizable(true)
                .default_size(egui::vec2(400.0, 550.0))
                .default_pos(egui::pos2(
                    ctx.content_rect().center().x - 200.0,
                    ctx.content_rect().center().y - 275.0
                ))
                .show(ctx, |ui| {
                    ui::AboutPanel::render(ui);

                    ui.add_space(20.0);
                    ui.horizontal(|ui| {
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui.button("Close").clicked() {
                                self.services.show_about_modal = false;
                            }
                        });
                    });
                });
        }

        if self.services.show_kicad_version_modal {
            egui::Window::new("KiCad Information")
                .collapsible(false)
                .resizable(false)
                .default_pos(egui::pos2(
                    ctx.content_rect().center().x - 200.0,
                    ctx.content_rect().center().y - 150.0
                ))
                .show(ctx, |ui| {
                    self.render_kicad_info_modal(ui);

                    ui.add_space(20.0);
                    ui.horizontal(|ui| {
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui.button("Close").clicked() {
                                self.services.show_kicad_version_modal = false;
                            }
                        });
                    });
                });
        }

        self.show_release_modal(ctx);
        self.handle_release_info_intent(ctx);
        self.show_release_info_modal(ctx);
        self.handle_delete_release_intent(ctx);
        self.show_delete_release_confirmation(ctx);
        self.handle_project_edit_open(ctx);
        self.show_project_edit_modal(ctx);
        self.handle_release_open_intent(ctx);
        self.handle_project_import_open(ctx);
        self.show_project_import_modal(ctx);

        if ctx.input(|i| i.time) % 30.0 < 0.1 {
            self.save_dock_state();
        }
    }
}

impl CopperForgeApp {
    /// Render the release modal and handle Create/Cancel actions.
    fn show_release_modal(&mut self, ctx: &egui::Context) {
        if self.projects_panel.panel_state.release_modal.is_none() {
            return;
        }

        let mut close = false;
        let mut trigger_create = false;

        let window_title = if self.projects_panel.panel_state.release_modal.as_ref().map(|m| m.overwrite_existing).unwrap_or(false) {
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
                let modal = self.projects_panel.panel_state.release_modal.as_mut().unwrap();

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
            self.execute_release_from_modal();
        }
        if close {
            self.projects_panel.panel_state.release_modal = None;
        }
    }

    /// Validate + run the create-release flow using the modal's current state.
    fn execute_release_from_modal(&mut self) {
        // Clone modal data out so we can mutably borrow self for the release call.
        let (modal, overwrite) = match self.projects_panel.panel_state.release_modal.as_ref() {
            Some(m) => (m.clone_for_exec(), m.overwrite_existing),
            None => return,
        };

        // Validate
        if modal.rev_tag.trim().is_empty() {
            if let Some(ref mut m) = self.projects_panel.panel_state.release_modal {
                m.error = Some("Rev tag cannot be empty".into());
            }
            return;
        }

        // Require project record + Ready state
        use crate::project::ProjectState;
        let (pcb_path, gerber_dir) = match self.services.project_state.get() {
            ProjectState::Ready { pcb_path, gerber_dir, .. } => (pcb_path, gerber_dir),
            _ => {
                if let Some(ref mut m) = self.projects_panel.panel_state.release_modal {
                    m.error = Some("Gerbers must be loaded (state: Ready) before releasing.".into());
                }
                return;
            }
        };

        // Collision check against existing releases — skipped in regenerate mode.
        if !overwrite {
            let current_pm_state = self.projects_panel.panel_state.project_manager_state.as_ref();
            let has_collision = current_pm_state
                .and_then(|s| s.current_project.as_ref())
                .map(|p| p.releases.iter().any(|r| r.tag == modal.rev_tag))
                .unwrap_or(false);
            if has_collision {
                if let Some(ref mut m) = self.projects_panel.panel_state.release_modal {
                    m.error = Some(format!(
                        "Release '{}' already exists. Right-click the rev in the Projects tree → Regenerate.",
                        modal.rev_tag
                    ));
                }
                return;
            }
        }

        // Build kicad-cli Command for drill export
        let Some(kicad_cli) = self.kicad_cli_command() else {
            if let Some(ref mut m) = self.projects_panel.panel_state.release_modal {
                m.error = Some("kicad-cli not discovered at startup — cannot export drill files.".into());
            }
            return;
        };

        let os_description = Self::build_os_description();
        let kicad_version = self.services.kicad_version.clone();
        let logger_state = self.services.logger_state.clone();
        let log_colors = self.services.log_colors.clone();
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
                if let Some(ref mut pm) = self.projects_panel.panel_state.project_manager_state {
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
                        if let Err(e) = self.services.project_db.save_project(current) {
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
                self.projects_panel.panel_state.release_modal = None;
            }
            Err(e) => {
                logger.log_error(&format!("Release failed: {}", e));
                if let Some(ref mut m) = self.projects_panel.panel_state.release_modal {
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
    fn handle_project_edit_open(&mut self, ctx: &egui::Context) {
        let pid = ctx.memory(|mem| {
            mem.data.get_temp::<String>(egui::Id::new("open_project_edit_modal"))
        });
        let Some(pid) = pid else { return; };
        ctx.memory_mut(|mem| {
            mem.data.remove::<String>(egui::Id::new("open_project_edit_modal"));
        });

        // Load the full ProjectData so the modal can show releases + kicad_pro
        // metadata (author, company).
        let data = match self.services.project_db.load_project(&pid) {
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

        self.projects_panel.panel_state.project_edit_modal = Some(ProjectEditModalState {
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
    fn handle_release_info_intent(&mut self, ctx: &egui::Context) {
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

        let release = self.projects_panel.panel_state.project_manager_state
            .as_ref()
            .and_then(|pm| pm.project_releases.get(project_id))
            .and_then(|releases| releases.iter().find(|r| r.tag == rev_tag))
            .cloned();
        if let Some(r) = release {
            self.projects_panel.panel_state.release_info_modal = Some(r);
        }
    }

    fn show_release_info_modal(&mut self, ctx: &egui::Context) {
        let Some(release) = self.projects_panel.panel_state.release_info_modal.clone() else { return; };
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
            self.projects_panel.panel_state.release_info_modal = None;
        }
    }

    // ─── Delete Release confirmation ───────────────────────────────

    /// Pick up "delete_release_intent" and seed the confirmation modal.
    fn handle_delete_release_intent(&mut self, ctx: &egui::Context) {
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

        let archive_path = self.projects_panel.panel_state.project_manager_state
            .as_ref()
            .and_then(|pm| pm.project_releases.get(&project_id))
            .and_then(|releases| releases.iter().find(|r| r.tag == rev_tag))
            .map(|r| r.archive_path.clone());
        let Some(archive_path) = archive_path else { return; };

        self.projects_panel.panel_state.delete_release_confirmation = Some(DeleteReleaseConfirmation {
            project_id,
            rev_tag,
            archive_path,
            error: None,
        });
    }

    fn show_delete_release_confirmation(&mut self, ctx: &egui::Context) {
        if self.projects_panel.panel_state.delete_release_confirmation.is_none() {
            return;
        }
        let (project_id, rev_tag, archive_path, error) = {
            let c = self.projects_panel.panel_state.delete_release_confirmation.as_ref().unwrap();
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
            self.projects_panel.panel_state.delete_release_confirmation = None;
            return;
        }

        if confirm {
            let logger_state = self.services.logger_state.clone();
            let log_colors = self.services.log_colors.clone();
            let logger = ReactiveEventLogger::with_colors(&logger_state, &log_colors);

            let result = delete_release_artifacts(
                &self.services.project_db,
                self.projects_panel.panel_state.project_manager_state.as_mut(),
                &project_id,
                &rev_tag,
                &archive_path,
                &logger,
            );
            match result {
                Ok(()) => {
                    logger.log_info(&format!("Deleted release '{}'", rev_tag));
                    self.projects_panel.panel_state.delete_release_confirmation = None;
                }
                Err(e) => {
                    if let Some(c) = self.projects_panel.panel_state.delete_release_confirmation.as_mut() {
                        c.error = Some(format!("Delete failed: {e}"));
                    }
                    logger.log_error(&format!("Delete release '{}' failed: {}", rev_tag, e));
                }
            }
        }
    }

    fn show_project_edit_modal(&mut self, ctx: &egui::Context) {
        if self.projects_panel.panel_state.project_edit_modal.is_none() { return; }
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
                let modal = self.projects_panel.panel_state.project_edit_modal.as_mut().unwrap();

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
            self.save_project_edit_modal();
        }
        if close {
            self.projects_panel.panel_state.project_edit_modal = None;
        }
    }

    fn save_project_edit_modal(&mut self) {
        let modal = match self.projects_panel.panel_state.project_edit_modal.as_ref() {
            Some(m) => (m.project_id.clone(), m.name.clone(), m.description.clone(), m.tags.clone(), m.pcb_file_path.clone()),
            None => return,
        };
        let (pid, name, description, tags_str, pcb_path) = modal;

        if name.trim().is_empty() {
            if let Some(ref mut m) = self.projects_panel.panel_state.project_edit_modal {
                m.error = Some("Name cannot be empty".into());
            }
            return;
        }

        let tags: Vec<String> = tags_str
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();

        let logger_state = self.services.logger_state.clone();
        let log_colors = self.services.log_colors.clone();
        let logger = ReactiveEventLogger::with_colors(&logger_state, &log_colors);

        // Update DB record via ProjectManagerState (keeps its project_list in sync).
        let result = if let Some(ref mut pm) = self.projects_panel.panel_state.project_manager_state {
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
                self.projects_panel.panel_state.project_edit_modal = None;
            }
            Err(e) => {
                if let Some(ref mut m) = self.projects_panel.panel_state.project_edit_modal {
                    m.error = Some(format!("Save failed: {}", e));
                }
            }
        }
    }

    // ─── Release right-click → open folder ─────────────────────────

    /// Pick up "open_release_intent" (value: "proj_X:rev:rev_01") and open the
    /// containing release dir via xdg-open / open / explorer.
    #[cfg(not(target_arch = "wasm32"))]
    fn handle_release_open_intent(&self, ctx: &egui::Context) {
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

        let logger_state = self.services.logger_state.clone();
        let log_colors = self.services.log_colors.clone();
        let logger = ReactiveEventLogger::with_colors(&logger_state, &log_colors);

        // Find the release in the cache to get its archive path.
        let releases = self.projects_panel.panel_state.project_manager_state
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
    fn handle_release_open_intent(&self, _ctx: &egui::Context) {
    }

    // ─── Project Import modal ──────────────────────────────────────

    /// Pick up the "open_project_import_modal" memory key set by the
    /// Projects tab's Import button click, and seed a fresh modal.
    fn handle_project_import_open(&mut self, ctx: &egui::Context) {
        let fire = ctx.memory(|mem| {
            mem.data.get_temp::<bool>(egui::Id::new("open_project_import_modal")).unwrap_or(false)
        });
        if !fire { return; }
        ctx.memory_mut(|mem| {
            mem.data.remove::<bool>(egui::Id::new("open_project_import_modal"));
        });
        self.projects_panel.panel_state.project_import_modal = Some(ProjectImportModalState {
            pcb_file_path: None,
            name: String::new(),
            description: String::new(),
            tags: String::new(),
            author: None,
            company: None,
            missing_pedigree: Vec::new(),
            error: None,
        });
        self.projects_panel.panel_state.project_import_last_picked = None;
    }

    fn show_project_import_modal(&mut self, ctx: &egui::Context) {
        if self.projects_panel.panel_state.project_import_modal.is_none() { return; }

        // Poll the file dialog first so auto-population runs this frame.
        if let Some(pro_path) = self.projects_panel.panel_state.project_import_dialog.update(ctx).picked() {
            let pro_path = pro_path.to_path_buf();
            if self.projects_panel.panel_state.project_import_last_picked.as_ref() != Some(&pro_path) {
                self.projects_panel.panel_state.project_import_last_picked = Some(pro_path.clone());
                if let Some(ref mut m) = self.projects_panel.panel_state.project_import_modal {
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
                let modal = self.projects_panel.panel_state.project_import_modal.as_mut().unwrap();

                // File picker row
                ui.horizontal(|ui| {
                    ui.label("KiCad Project File (.kicad_pro):");
                    if ui.button("Browse...").clicked() {
                        use std::sync::Arc;
                        use std::mem;
                        use egui_file_dialog::FileDialog;
                        let dialog = mem::replace(&mut self.projects_panel.panel_state.project_import_dialog, FileDialog::new());
                        let mut dialog = dialog
                            .add_file_filter("KiCad Project", Arc::new(|path: &std::path::Path| {
                                path.extension().and_then(|e| e.to_str()).map(|e| e == "kicad_pro").unwrap_or(false)
                            }))
                            .default_file_filter("KiCad Project");
                        if let Some(ref dir) = self.services.config.preferred_projects_directory {
                            dialog = dialog.initial_directory(dir.clone());
                        }
                        self.projects_panel.panel_state.project_import_dialog = dialog;
                        self.projects_panel.panel_state.project_import_dialog.pick_file();
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
            self.execute_project_import();
        }
        if close {
            self.projects_panel.panel_state.project_import_modal = None;
            self.projects_panel.panel_state.project_import_last_picked = None;
        }
    }

    fn execute_project_import(&mut self) {
        let (pcb_path, name, description, tags_str) = match self.projects_panel.panel_state.project_import_modal.as_ref() {
            Some(m) => (m.pcb_file_path.clone(), m.name.clone(), m.description.clone(), m.tags.clone()),
            None => return,
        };
        if name.trim().is_empty() {
            if let Some(ref mut m) = self.projects_panel.panel_state.project_import_modal {
                m.error = Some("Name cannot be empty".into());
            }
            return;
        }
        let pcb_path = match pcb_path {
            Some(p) => p,
            None => {
                if let Some(ref mut m) = self.projects_panel.panel_state.project_import_modal {
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

        let logger_state = self.services.logger_state.clone();
        let log_colors = self.services.log_colors.clone();
        let logger = ReactiveEventLogger::with_colors(&logger_state, &log_colors);

        let bom_cell = self.services.bom_state.clone();
        let bom_components: Vec<crate::project_manager::bom::BomComponent> =
            if let Some(ref bom_state) = *bom_cell.lock() {
                bom_state.entries.iter().cloned().map(Into::into).collect()
            } else {
                Vec::new()
            };

        // Ensure ProjectManagerState is initialized so create_project can persist.
        if self.projects_panel.panel_state.project_manager_state.is_none() {
            let mut state = project_manager::ProjectManagerState::with_config(&self.services.config);
            if let Err(e) = state.initialize_database(&self.services.project_db) {
                logger.log_error(&format!("Failed to initialize project database: {}", e));
            }
            self.projects_panel.panel_state.project_manager_state = Some(state);
        }

        let result = self.projects_panel.panel_state.project_manager_state.as_mut().unwrap().create_project(
            name.clone(),
            description,
            pcb_path,
            tags,
            bom_components,
        );
        match result {
            Ok(id) => {
                logger.log_info(&format!("Imported project: {} (ID: {})", name, id));
                self.projects_panel.panel_state.project_import_modal = None;
                self.projects_panel.panel_state.project_import_last_picked = None;
            }
            Err(e) => {
                if let Some(ref mut m) = self.projects_panel.panel_state.project_import_modal {
                    m.error = Some(format!("Import failed: {}", e));
                }
            }
        }
    }
}

impl crate::app::ReleaseModalState {
    /// Snapshot the values needed to execute the release, avoiding borrow
    /// conflicts with `self.projects_panel.panel_state.release_modal` during the call.
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
