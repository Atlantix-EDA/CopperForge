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
   GerberTransform
};
use crate::platform::parameters::gui::VERSION;
use crate::project;
use crate::ui;
use crate::project_manager;
use crate::services::{SharedServices, GerberViewState};

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
    /// Last persisted dock-layout JSON, for change-detected eager saving — the
    /// working layout is written the moment panels are rearranged, so a
    /// force-kill (or crash) can't lose it the way a Drop-only save would.
    last_layout: Option<String>,
    /// Named, saveable/deletable dock perspectives + a startup default.
    perspectives: crate::perspectives::PerspectiveStore,
    /// "Save current as…" name buffer for the Perspectives menu.
    perspective_save_buf: String,

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

    // ── Projects panel ────────────────────────────────────────────
    /// Stored citizen owning its `ProjectsPanelState`. Registered with a
    /// real `CitizenState` (not `::default()`). Self-contained: it
    /// renders over its own state + `SharedServices` only.
    pub projects_panel: crate::panels::ProjectsPanel,

    /// Panels contributed by external crates, registered via
    /// [`CopperForgeApp::register_panel`]. Dispatched through the
    /// `DockPanel` trait — core never names them. Empty by default.
    pub plugin_panels: Vec<Box<dyn crate::dock_panel::DockPanel>>,

    // ── App-shell modal flags ─────────────────────────────────
    // Which app-level modal is open. These are shell concerns — not citizen
    // or shared-panel state — so they live on the app, not in SharedServices.
    pub show_about_modal: bool,
    pub show_kicad_version_modal: bool,
    /// Toggled by clicking the ribbon's CuForge Services indicator;
    /// renders the connection-details modal (URL, version, recheck).
    pub show_cuforge_services_modal: bool,
}

/// Persistent state for the Projects panel, grouped into one struct so
/// the panel stays self-contained (its own state + `SharedServices`).
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
    /// Board part number for the BOM cover page. Prefilled from the project
    /// name; user-editable per release.
    pub board_pn: String,
    /// Copper weight for the BOM cover page, e.g. "2 oz". No board metadata
    /// records this, so it's entered per release.
    pub copper_weight: String,
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
pub(crate) fn delete_release_artifacts(
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
        self.services.layer_store.zoom.set_scale(self.services.gerber_view.view_state.scale);
        self.services.layer_store.zoom.center_x = self.services.gerber_view.view_state.translation.x;
        self.services.layer_store.zoom.center_y = self.services.gerber_view.view_state.translation.y;
    }

    pub fn sync_zoom_from_ecs(&mut self) {
        self.services.gerber_view.view_state.scale = self.services.layer_store.zoom.scale;
        self.services.gerber_view.view_state.translation.x = self.services.layer_store.zoom.center_x;
        self.services.gerber_view.view_state.translation.y = self.services.layer_store.zoom.center_y;
    }

    pub fn render_layers_ecs(&mut self, painter: &egui::Painter) {
        let view_state = self.services.gerber_view.view_state;
        let rotation = self.services.gerber_view.rotation_degrees;
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
            gerber_view: GerberViewState::default(),
            board_geometry_gen: 0,
            board_outline: None,
            top_copper: None,
            bottom_copper: None,
            top_mask: None,
            bottom_mask: None,
            drill: None,
            display_manager: DisplayManager::new(),
            drc_manager: DrcManager::new(),
            grid_settings: GridSettings::default(),
            global_units_mils: config.global_units_mils,
            user_timezone: config.user_timezone.clone(),
            use_24_hour_clock: config.use_24_hour_clock,
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

        // Restore the working layout, then let a saved default perspective
        // override it on startup (plugin tabs are re-added by `register_panel`,
        // so strip any persisted ones first).
        let perspectives = crate::perspectives::PerspectiveStore::load();
        let mut dock_state = Self::create_default_dock_state();
        if let Some(json) = perspectives.default_json() {
            if let Ok(mut ds) = serde_json::from_str::<DockState<Tab>>(json) {
                ds.retain_tabs(|tab| !matches!(&tab.kind, TabKind::Plugin(_)));
                dock_state = ds;
            }
        }
        // Seed the change-detector with the restored/default layout so we only
        // write once the user actually rearranges something.
        let last_layout = serde_json::to_string(&dock_state).ok();

        let mut app = Self {
            services,
            dispatcher,
            app_messages: Vec::new(),
            dock_state,
            last_layout,
            perspectives,
            perspective_save_buf: String::new(),
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
            show_about_modal: false,
            show_kicad_version_modal: false,
            show_cuforge_services_modal: false,
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

            self.services.gerber_view.view_state.translation = Vec2::new(
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
                rotation: self.services.gerber_view.rotation_degrees.to_radians(),
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

            self.services.gerber_view.view_state.translation = Vec2::new(
                viewport.center().x - (transformed_center.x as f32 * scale),
                viewport.center().y + (transformed_center.y as f32 * scale),
            );
        }

        self.services.gerber_view.view_state.scale = scale;

        self.services.layer_store.zoom.set_scale(scale);
        self.services.layer_store.zoom.set_fit_to_view_scale(scale);
        self.services.layer_store.zoom.center_x = self.services.gerber_view.view_state.translation.x;
        self.services.layer_store.zoom.center_y = self.services.gerber_view.view_state.translation.y;

        self.services.gerber_view.needs_initial_view = false;
    }

    /// Zoom to a specific BOM component location
    pub fn zoom_to_component(&mut self, component: &project_manager::bom::BomComponent, viewport: Rect) {
        if !self.services.gerber_view.origin_has_been_set {
            let logger = ReactiveEventLogger::with_colors(&self.services.logger_state, &self.services.log_colors);
            logger.log_warning("Please set the origin before using cross-probing");
            return;
        }

        let comp_x = component.x_location;
        let comp_y = component.y_location;

        let viewport_center = viewport.center();
        self.services.gerber_view.view_state.translation = Vec2::new(
            viewport_center.x - (comp_x as f32 * self.services.gerber_view.view_state.scale),
            viewport_center.y + (comp_y as f32 * self.services.gerber_view.view_state.scale),
        );

        self.services.layer_store.zoom.center_x = self.services.gerber_view.view_state.translation.x;
        self.services.layer_store.zoom.center_y = self.services.gerber_view.view_state.translation.y;

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
            self.show_about_modal = true;
        }

        ui.separator();

        let kicad_text = if let Some(ref version) = self.services.kicad_version {
            format!("KiCad {}", version)
        } else {
            "KiCad (not found)".to_string()
        };

        if ui.button(egui::RichText::new(kicad_text)
            .color(egui::Color32::from_rgb(180, 255, 200))).clicked() {
            self.show_kicad_version_modal = true;
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
    /// is how external crates contribute panels.
    /// Apply a saved perspective: replace the dock layout with the
    /// perspective's, then re-add the registered plugin tabs (their positions
    /// aren't persisted — indices aren't stable across runs).
    fn apply_perspective(&mut self, json: &str) {
        let Ok(mut ds) = serde_json::from_str::<DockState<Tab>>(json) else {
            return;
        };
        ds.retain_tabs(|tab| !matches!(&tab.kind, TabKind::Plugin(_)));
        self.dock_state = ds;
        for idx in 0..self.plugin_panels.len() {
            let tab = Tab::new(TabKind::Plugin(idx), SurfaceIndex::main(), NodeIndex(0));
            self.dock_state.main_surface_mut().push_to_first_leaf(tab);
        }
        // Sync the change-detector so applying doesn't immediately rewrite the
        // working-layout file with the just-applied state.
        self.last_layout = serde_json::to_string(&self.dock_state).ok();
    }

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
                    self.services.gerber_view.rotation_degrees = (self.services.gerber_view.rotation_degrees + 90.0) % 360.0;
                    self.services.layer_store.mark_dirty();

                    let logger = ReactiveEventLogger::with_colors(&self.services.logger_state, &self.services.log_colors);
                    logger.log_custom(
                        project::constants::LOG_TYPE_ROTATION,
                        &format!("Rotated board to {:.0}° (R key)", self.services.gerber_view.rotation_degrees)
                    );
                }

                if two_d_view_active && i.key_pressed(egui::Key::A) {
                    display::align_to_grid(&mut self.services.gerber_view.view_state, &self.services.grid_settings);
                    let logger = ReactiveEventLogger::with_colors(&self.services.logger_state, &self.services.log_colors);
                    logger.log_info("Aligned view to grid (A key)");
                }

                if two_d_view_active && i.key_pressed(egui::Key::M) {
                    if self.services.gerber_view.ruler_active {
                        if self.services.gerber_view.ruler_start.is_some() && self.services.gerber_view.ruler_end.is_some() {
                            self.services.gerber_view.latched_measurement_start = self.services.gerber_view.ruler_start;
                            self.services.gerber_view.latched_measurement_end = self.services.gerber_view.ruler_end;
                        }
                        self.services.gerber_view.ruler_active = false;
                        self.services.gerber_view.ruler_start = None;
                        self.services.gerber_view.ruler_end = None;
                        self.services.gerber_view.ruler_dragging = false;

                        let logger = ReactiveEventLogger::with_colors(&self.services.logger_state, &self.services.log_colors);
                        logger.log_info("Ruler mode deactivated (M key) - measurement latched");
                    } else {
                        self.services.gerber_view.latched_measurement_start = None;
                        self.services.gerber_view.latched_measurement_end = None;
                        self.services.gerber_view.ruler_active = true;

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

                if i.key_pressed(egui::Key::Escape) && self.services.gerber_view.ruler_active {
                    if self.services.gerber_view.ruler_start.is_some() && self.services.gerber_view.ruler_end.is_some() {
                        self.services.gerber_view.latched_measurement_start = self.services.gerber_view.ruler_start;
                        self.services.gerber_view.latched_measurement_end = self.services.gerber_view.ruler_end;

                        let logger = ReactiveEventLogger::with_colors(&self.services.logger_state, &self.services.log_colors);
                        if let (Some(start), Some(end)) = (self.services.gerber_view.ruler_start, self.services.gerber_view.ruler_end) {
                            logger.log_info(&format!("Latching measurement - Start: ({:.6}, {:.6}), End: ({:.6}, {:.6})",
                                                    start.x, start.y, end.x, end.y));
                            let dx = end.x - start.x;
                            let dy = end.y - start.y;
                            logger.log_info(&format!("Latching deltas - ΔX: {:.6}, ΔY: {:.6}", dx, dy));
                        }
                    }

                    self.services.gerber_view.ruler_active = false;
                    self.services.gerber_view.ruler_start = None;
                    self.services.gerber_view.ruler_end = None;
                    self.services.gerber_view.ruler_dragging = false;

                    let logger = ReactiveEventLogger::with_colors(&self.services.logger_state, &self.services.log_colors);
                    logger.log_info("Ruler mode cancelled (ESC key) - measurement latched");
                }
            });
        }

        // Project Ribbon at the top
        // Perspective-menu intents, applied after the ribbon closure so we
        // don't borrow `self` mutably while the menu reads `self.perspectives`.
        let mut persp_apply: Option<String> = None;
        let mut persp_save: Option<String> = None;
        let mut persp_delete: Option<String> = None;
        let mut persp_set_default: Option<Option<String>> = None;

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
                        self.show_cuforge_services_modal = true;
                    }
                    ui.separator();
                    self.show_clock_display(ui);
                    ui.separator();
                    ui.menu_button("🗗 Perspectives", |ui| {
                        let names: Vec<String> = self.perspectives.names().cloned().collect();
                        let default_name = self.perspectives.default_name().map(str::to_string);

                        if names.is_empty() {
                            ui.label(egui::RichText::new("(no saved perspectives)").italics());
                        } else {
                            ui.label("Open:");
                            for name in &names {
                                let star = if default_name.as_deref() == Some(name) { "★ " } else { "" };
                                if ui.button(format!("{star}{name}")).clicked() {
                                    persp_apply = self.perspectives.get(name).cloned();
                                    ui.close();
                                }
                            }
                        }

                        ui.separator();
                        ui.label("Save current layout as:");
                        ui.horizontal(|ui| {
                            ui.text_edit_singleline(&mut self.perspective_save_buf);
                            let can = !self.perspective_save_buf.trim().is_empty();
                            if ui.add_enabled(can, egui::Button::new("💾 Save")).clicked() {
                                persp_save = Some(self.perspective_save_buf.trim().to_string());
                                ui.close();
                            }
                        });

                        if !names.is_empty() {
                            ui.separator();
                            ui.menu_button("Set default (startup)", |ui| {
                                if ui.button("None — use working layout").clicked() {
                                    persp_set_default = Some(None);
                                    ui.close();
                                }
                                for name in &names {
                                    if ui.button(name).clicked() {
                                        persp_set_default = Some(Some(name.clone()));
                                        ui.close();
                                    }
                                }
                            });
                            ui.menu_button("Delete", |ui| {
                                for name in &names {
                                    if ui.button(name).clicked() {
                                        persp_delete = Some(name.clone());
                                        ui.close();
                                    }
                                }
                            });
                        }
                    });
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

        // Apply Perspectives-menu intents (deferred out of the closure).
        if let Some(json) = persp_apply {
            self.apply_perspective(&json);
        }
        if let Some(name) = persp_save {
            if let Ok(json) = serde_json::to_string(&self.dock_state) {
                self.perspectives.save(name, json);
            }
            self.perspective_save_buf.clear();
        }
        if let Some(name) = persp_delete {
            self.perspectives.delete(&name);
        }
        if let Some(d) = persp_set_default {
            self.perspectives.set_default(d);
        }

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

        // Persist the perspective the moment panels are rearranged — compared
        // against the last-saved JSON so we only write on change, and so a
        // force-kill can't lose the layout (Drop alone wouldn't have run).
        if let Ok(json) = serde_json::to_string(&self.dock_state) {
            if self.last_layout.as_deref() != Some(json.as_str()) {
                self.save_dock_state();
                self.last_layout = Some(json);
            }
        }

        crate::cuforge_client::show_modal_if_open(
            ctx,
            &mut self.show_cuforge_services_modal,
            &self.services.cuforge_status,
        );

        if self.show_about_modal {
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
                                self.show_about_modal = false;
                            }
                        });
                    });
                });
        }

        if self.show_kicad_version_modal {
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
                                self.show_kicad_version_modal = false;
                            }
                        });
                    });
                });
        }

        // PM modals + intent handlers now flow through the Projects panel
        // itself (crate::ui::show_projects_panel → show_projects_modals),
        // so they're no longer driven from here.

        if ctx.input(|i| i.time) % 30.0 < 0.1 {
            self.save_dock_state();
        }
    }
}

