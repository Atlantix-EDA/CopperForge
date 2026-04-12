//! Shared domain services accessible by all citizen panels.
//!
//! This struct will replace the flat fields on `CopperForgeApp` once the
//! migration to citizen panels is complete. Panels receive `&SharedServices`
//! (read) or `&mut SharedServices` (write) instead of `&mut CopperForgeApp`.

use std::path::PathBuf;

use egui::Pos2;
use egui_mobius_reactive::Dynamic;
use gerber_viewer::{GerberLayer, ViewState, UiState};

use crate::display::{DisplayManager, GridSettings};
use crate::drc_operations::DrcManager;
use crate::event_logger::{ReactiveEventLoggerState, LogColors};
use crate::project::ProjectManager;

/// Shared domain state accessible by all citizen panels.
pub struct SharedServices {
    // ── Logger (reactive, cloneable) ──────────────────────────
    pub logger_state: Dynamic<ReactiveEventLoggerState>,
    pub log_colors: Dynamic<LogColors>,

    // ── Gerber / viewport ─────────────────────────────────────
    pub gerber_layer: GerberLayer,
    pub view_state: ViewState,
    pub ui_state: UiState,
    pub needs_initial_view: bool,
    pub rotation_degrees: f32,

    // ── Display ───────────────────────────────────────────────
    pub display_manager: DisplayManager,
    pub grid_settings: GridSettings,
    pub global_units_mils: bool,

    // ── Domain managers ───────────────────────────────────────
    pub drc_manager: DrcManager,
    pub project_manager: ProjectManager,

    // ── Layer management ────────────────────────────────────
    pub layer_store: crate::layer_store::LayerStore,

    // ── Config persistence ────────────────────────────────────
    pub config_path: PathBuf,

    // ── Viewport interaction state ────────────────────────────
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

    // ── User preferences ──────────────────────────────────────
    pub user_timezone: Option<String>,
    pub use_24_hour_clock: bool,

    // ── Modal states ──────────────────────────────────────────
    pub show_about_modal: bool,
    pub show_kicad_version_modal: bool,
    pub kicad_version: Option<String>,
}
