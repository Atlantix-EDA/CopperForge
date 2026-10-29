//! Startup lifecycle FSM — the app's control plane for bring-up.
//!
//! One transition per frame, driven from `CopperForgeApp::ui`:
//!
//! ```text
//! Init → LoadSettings → LoadPerspective → ClearData → Banner → SystemInfo → ConnectCheck → Run
//! ```
//!
//! - **Init** — entry state, first frame.
//! - **LoadSettings** — re-read the config and apply user settings (time
//!   format / 24h clock, timezone, …) to the shared services.
//! - **LoadPerspective** — apply the saved default dock perspective, *after*
//!   plugin panels are registered, so their tabs are re-added correctly.
//! - **ClearData** — reset all derived / "recursive" data + displays (layer
//!   store, board geometry) so nothing stale from a prior run lingers.
//! - **Banner** — emit the app banner to the logger.
//! - **SystemInfo** — emit `platform::details` OS/system info to the logger,
//!   and kick off the backend health poller.
//! - **ConnectCheck** — resolve backend reachability: advance to Run once
//!   `cuforge_status` is decided (Connected / Disconnected) or a short
//!   deadline passes, so a down backend never wedges startup.
//! - **Run** — normal operation (IDLE + running).
//!
//! This is the control plane; kept separate from `SharedServices` (the shared
//! data plane) — the two-state-planes discipline.

use std::time::Instant;

#[derive(Debug, Clone)]
pub enum StartupPhase {
    Init,
    LoadSettings,
    LoadPerspective,
    ClearData,
    Banner,
    SystemInfo,
    ConnectCheck { since: Instant },
    Run,
}

impl Default for StartupPhase {
    fn default() -> Self {
        Self::Init
    }
}

impl StartupPhase {
    /// True once bring-up is complete and the app is running normally.
    pub fn is_running(&self) -> bool {
        matches!(self, StartupPhase::Run)
    }
}

/// How long ConnectCheck waits for a decisive backend status before giving up
/// and proceeding to Run anyway.
pub const CONNECT_DEADLINE_SECS: f32 = 3.0;
