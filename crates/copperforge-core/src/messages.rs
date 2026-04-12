//! Application-level messages for Elm-style dispatch.
//!
//! These flow through the same message loop as `CitizenMessage`, enabling
//! clean separation between UI events and domain logic.

use std::path::PathBuf;
use egui_citizen::CitizenMessage;

/// Application-level messages beyond citizen lifecycle events.
#[derive(Debug, Clone)]
pub enum AppMessage {
    /// A citizen lifecycle event (activated, deactivated, etc.)
    Citizen(CitizenMessage),

    // ── Project ───────────────────────────────────────────────
    ProjectLoaded { path: PathBuf },
    ProjectClosed,
    PcbFileSelected { path: PathBuf },

    // ── Gerber / Layers ───────────────────────────────────────
    GerbersLoaded { count: usize },
    LayerVisibilityChanged { layer_name: String, visible: bool },
    LayersReloaded,

    // ── View ──────────────────────────────────────────────────
    ResetView,
    FlipBoard,
    Rotate { degrees: f32 },
    UnitsToggled { mils: bool },

    // ── DRC ───────────────────────────────────────────────────
    DrcRunRequested,
    DrcCompleted { violation_count: usize },

    // ── BOM ───────────────────────────────────────────────────
    BomUpdated { component_count: usize },
    CrossProbe { reference: String, x: f64, y: f64 },

    // ── Hotkeys ───────────────────────────────────────────────
    HotkeyPressed(Hotkey),

    // ── Future: release management ────────────────────────────
    // ReleaseTagged { tag: String, gerber_snapshot: PathBuf },
    // VendorPackageReady { vendor: VendorKind, archive_path: PathBuf },
}

/// Keyboard hotkey actions routed through the message system.
#[derive(Debug, Clone)]
pub enum Hotkey {
    Flip,
    Rotate,
    ToggleUnits,
    AlignGrid,
    ToggleRuler,
    CancelMeasurement,
}
