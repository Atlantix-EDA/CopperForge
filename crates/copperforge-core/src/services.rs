//! Shared application services — the single source of truth for state that
//! crosses panel boundaries.
//!
//! Populated eagerly during `CopperForgeApp::new()` via an explicit init
//! sequence (config → KiCad discovery → project DB). Panels read and mutate
//! `SharedServices` directly; panel-local state lives inside the panel's
//! citizen struct.

use std::path::PathBuf;

use egui::Pos2;
use egui_mobius_reactive::Dynamic;
use gerber_viewer::{GerberLayer, ViewState, UiState};

use crate::display::{DisplayManager, GridSettings};
use crate::drc_operations::DrcManager;
use crate::event_logger::{LogColors, ReactiveEventLoggerState};
use crate::project::{manager::ProjectConfig, ProjectState};
use crate::project_manager::database::ProjectDatabase;

/// Every cross-panel fact lives here. Populated once at init.
pub struct SharedServices {
    // ── Reactive (observable across panels) ───────────────────
    /// Drives BOM refresh, gerber ribbon state, and everything else that
    /// keys off "which PCB is active and what stage are we at".
    pub project_state: Dynamic<ProjectState>,
    pub logger_state: Dynamic<ReactiveEventLoggerState>,
    pub log_colors: Dynamic<LogColors>,

    // ── Init-time facts (set once, rarely mutate) ─────────────
    pub config: ProjectConfig,
    pub config_path: PathBuf,
    pub kicad_version: Option<String>,
    /// One of "path" / "flatpak" / "snap". Used by
    /// `CopperForgeApp::kicad_cli_command()` to build Commands without probing.
    pub kicad_cli_method: Option<String>,
    pub project_db: ProjectDatabase,

    // ── Gerber / viewport ─────────────────────────────────────
    pub layer_store: crate::layer_store::LayerStore,
    pub gerber_layer: GerberLayer,
    pub view_state: ViewState,
    pub ui_state: UiState,
    pub needs_initial_view: bool,
    pub rotation_degrees: f32,

    // ── Display / DRC / grid ──────────────────────────────────
    pub display_manager: DisplayManager,
    pub drc_manager: DrcManager,
    pub grid_settings: GridSettings,
    pub global_units_mils: bool,

    // ── User preferences ──────────────────────────────────────
    pub user_timezone: Option<String>,
    pub use_24_hour_clock: bool,

    // ── Viewport interaction ──────────────────────────────────
    pub zoom_window_start: Option<Pos2>,
    pub zoom_window_dragging: bool,
    pub setting_origin_mode: bool,
    pub origin_has_been_set: bool,

    // ── Ruler tool ────────────────────────────────────────────
    pub ruler_active: bool,
    pub ruler_start: Option<nalgebra::Point2<f64>>,
    pub ruler_end: Option<nalgebra::Point2<f64>>,
    pub ruler_dragging: bool,
    pub ruler_drag_start: Option<nalgebra::Point2<f64>>,
    pub latched_measurement_start: Option<nalgebra::Point2<f64>>,
    pub latched_measurement_end: Option<nalgebra::Point2<f64>>,

    // ── Modal flags ───────────────────────────────────────────
    pub show_about_modal: bool,
    pub show_kicad_version_modal: bool,
}
