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

/// A generic overlay drawn over the board in the gerber view. Coordinates are
/// world (mm), in the gerber frame, and are transformed by the view like any
/// other geometry. Core draws these primitives without knowing what they
/// represent — producers (e.g. a dock panel) build whatever shapes they need.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ViewOverlay {
    pub fills: Vec<OverlayRect>,
    pub lines: Vec<OverlayLine>,
    pub labels: Vec<OverlayLabel>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct OverlayRect {
    pub min: (f64, f64),
    pub max: (f64, f64),
    pub rgba: [u8; 4],
}

#[derive(Debug, Clone, PartialEq)]
pub struct OverlayLine {
    pub from: (f64, f64),
    pub to: (f64, f64),
    pub rgba: [u8; 4],
    pub width: f32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct OverlayLabel {
    pub at: (f64, f64),
    pub text: String,
    pub rgba: [u8; 4],
    pub size: f32,
}

/// The gerber view's view + interaction state, grouped into one struct instead
/// of a flat run of fields on `SharedServices`. Shared state (relationship #2):
/// the view transform / refit flag / rotation are read by export, the grid, and
/// the app; the gerber-view citizen owns the behaviour over it.
pub struct GerberViewState {
    pub view_state: ViewState,
    pub ui_state: UiState,
    pub needs_initial_view: bool,
    pub rotation_degrees: f32,
    pub zoom_window_start: Option<Pos2>,
    pub zoom_window_dragging: bool,
    pub setting_origin_mode: bool,
    pub origin_has_been_set: bool,
    pub ruler_active: bool,
    pub ruler_start: Option<nalgebra::Point2<f64>>,
    pub ruler_end: Option<nalgebra::Point2<f64>>,
    pub ruler_dragging: bool,
    pub latched_measurement_start: Option<nalgebra::Point2<f64>>,
    pub latched_measurement_end: Option<nalgebra::Point2<f64>>,
}

impl Default for GerberViewState {
    fn default() -> Self {
        Self {
            view_state: ViewState::default(),
            ui_state: UiState::default(),
            needs_initial_view: true,
            rotation_degrees: 0.0,
            zoom_window_start: None,
            zoom_window_dragging: false,
            setting_origin_mode: false,
            origin_has_been_set: false,
            ruler_active: false,
            ruler_start: None,
            ruler_end: None,
            ruler_dragging: false,
            latched_measurement_start: None,
            latched_measurement_end: None,
        }
    }
}

/// Extracted board-geometry IR for the 3D view — the FDD Stage 3-6 output, all
/// derived from the loaded gerbers and repopulated/cleared together by
/// `load_gerbers`. Grouped out of the flat `SharedServices` field list.
#[derive(Default)]
pub struct BoardGeometry {
    /// Board-outline polygon IR from the mechanical-outline gerber.
    pub board_outline: Option<crate::gerber_geom::OutlineData>,
    /// F.Cu polygon IR — top-side copper.
    pub top_copper: Option<crate::gerber_geom::CopperData>,
    /// B.Cu polygon IR — bottom-side copper.
    pub bottom_copper: Option<crate::gerber_geom::CopperData>,
    /// F.Mask polygon IR — top soldermask (holes already cut).
    pub top_mask: Option<crate::gerber_geom::MaskData>,
    /// B.Mask polygon IR — bottom soldermask.
    pub bottom_mask: Option<crate::gerber_geom::MaskData>,
    /// F.SilkS polygon IR — top silkscreen legend. Same mesh shape as
    /// copper (the silk gerber uses the same primitive types), so it reuses
    /// `CopperData`.
    pub top_silk: Option<crate::gerber_geom::CopperData>,
    /// B.SilkS polygon IR — bottom silkscreen legend.
    pub bottom_silk: Option<crate::gerber_geom::CopperData>,
    /// Inner copper layers, paired with their 1-based stack index
    /// (`Copper(2)`..`Copper(N-1)`, between F.Cu and B.Cu). Empty on 2-layer
    /// boards. The 3D view places each at its interpolated stack depth.
    pub inner_copper: Vec<(u8, crate::gerber_geom::CopperData)>,
    /// Drilled hole centres + radii in the board's world frame.
    pub drill: Option<crate::gerber_geom::DrillData>,
}

/// Every cross-panel fact lives here. Populated once at init.
pub struct SharedServices {
    // ── Reactive (observable across panels) ───────────────────
    /// Drives BOM refresh, gerber ribbon state, and everything else that
    /// keys off "which PCB is active and what stage are we at".
    pub project_state: Dynamic<ProjectState>,
    /// Parsed BOM. Promoted from BomPanel-local to a shared cell so the
    /// Projects panel reads/writes it via a clone (Path A) rather than
    /// reaching into `bom_panel.state` — the one cross-panel dependency.
    pub bom_state: Dynamic<Option<crate::ui::BomPanelState>>,
    pub logger_state: Dynamic<ReactiveEventLoggerState>,
    pub log_colors: Dynamic<LogColors>,
    /// Liveness of the private `cuforge-services` backend. Updated in
    /// the background by `cuforge_client::spawn_health_poller`; the
    /// ribbon's connection indicator reads off this cell.
    pub cuforge_status: Dynamic<crate::cuforge_client::CuforgeStatus>,

    // ── Init-time facts (set once, rarely mutate) ─────────────
    pub config: ProjectConfig,
    pub config_path: PathBuf,
    pub kicad_version: Option<String>,
    /// One of "path" (stable `kicad-cli` on PATH) / "path-nightly"
    /// (`kicad-cli-nightly` on PATH) / "flatpak" / "snap". Used by
    /// `CopperForgeApp::kicad_cli_command()` to build Commands without probing.
    pub kicad_cli_method: Option<String>,
    pub project_db: ProjectDatabase,

    // ── Gerber / viewport ─────────────────────────────────────
    pub layer_store: crate::layer_store::LayerStore,
    pub gerber_layer: GerberLayer,
    /// The gerber view's view + interaction state (grouped — see `GerberViewState`).
    pub gerber_view: GerberViewState,
    /// Bumped every time the board geometry is (re)loaded. Lets the 3D panel
    /// detect a new board and rebuild its meshes regardless of which path
    /// triggered the load (app ribbon or a dock panel via services).
    pub board_geometry_gen: u64,

    // ── 3D pipeline geometry (FDD Stage 3-6 output) ───────────
    /// Extracted board geometry IR (outline / copper / mask / drill), grouped.
    pub geometry: BoardGeometry,

    // ── Display / DRC / grid ──────────────────────────────────
    pub display_manager: DisplayManager,
    pub drc_manager: DrcManager,
    pub grid_settings: GridSettings,
    pub global_units_mils: bool,

    // ── User preferences ──────────────────────────────────────
    pub user_timezone: Option<String>,
    pub use_24_hour_clock: bool,

    // ── Cross-panel summaries ─────────────────────────────────
    /// Count of BOM entries loaded in the BOM panel. Mirror so other panels
    /// (e.g. the Shell `status` command) can report it without reaching into
    /// BomPanel's private state. Updated by BomPanel on extraction.
    pub bom_component_count: usize,
}
