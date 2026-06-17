//! Native dock tab hosting the 3D board view.
//!
//! Thin `egui_citizen::Citizen` wrapper around [`Board3dView`] — the actual
//! rendering (axes, grid, board outline, copper, soldermask, camera,
//! measure/zoom tools) lives in `board3d_view.rs`, which is citizen-free so
//! the wasm browser app can reuse it without the citizen framework. This
//! file is the *only* thing in the 3D path that depends on `egui_citizen`.

use std::sync::Arc;

use egui_citizen::{Citizen, CitizenId, CitizenState};

use crate::gerber_geom::{CopperData, DrillData, MaskData, OutlineData};
use crate::panels::board3d_view::Board3dView;

/// Phase-3 3D viewport as a dockable citizen. Delegates all rendering and
/// input to the inner [`Board3dView`]; owns only the citizen identity/state.
pub struct GerberView3dPanel {
    citizen_id: CitizenId,
    citizen_state: CitizenState,
    view: Board3dView,
}

impl GerberView3dPanel {
    pub fn new(citizen_state: CitizenState) -> Self {
        Self {
            citizen_id: CitizenId::new("gerber_view_3d"),
            citizen_state,
            view: Board3dView::new(),
        }
    }

    /// Force a full mesh re-upload on the next frame — call after loading a
    /// different board so the 3D view doesn't keep the previous one's meshes.
    pub fn mark_dirty(&mut self) {
        self.view.mark_dirty();
    }

    /// Flip the camera 180° about world Y — reveals the back of the board.
    /// Bound to the `F` hotkey when the 3D tab is the active citizen.
    pub fn flip_view(&mut self) {
        self.view.flip_view();
    }

    /// Rotate the view 90° in-plane (about world Z). Bound to the `R`
    /// hotkey when the 3D tab is the active citizen.
    pub fn rotate_in_plane_90(&mut self) {
        self.view.rotate_in_plane_90();
    }

    /// Enter/exit the measure tool. Returns the new active state. Bound to
    /// the `M` hotkey when the 3D tab is the active citizen.
    pub fn toggle_measure(&mut self) -> bool {
        self.view.toggle_measure()
    }

    #[allow(clippy::too_many_arguments)]
    pub fn show(
        &mut self,
        ui: &mut egui::Ui,
        gl: Option<&Arc<glow::Context>>,
        board_outline: Option<&OutlineData>,
        top_copper: Option<&CopperData>,
        bottom_copper: Option<&CopperData>,
        top_mask: Option<&MaskData>,
        bottom_mask: Option<&MaskData>,
        drill: Option<&DrillData>,
        units_mils: bool,
    ) {
        self.view.show(
            ui, gl, board_outline,
            top_copper, bottom_copper,
            top_mask, bottom_mask,
            drill,
            units_mils,
        );
    }
}

impl Citizen for GerberView3dPanel {
    fn id(&self) -> &CitizenId { &self.citizen_id }
    fn citizen_state(&self) -> &CitizenState { &self.citizen_state }
    fn citizen_state_mut(&mut self) -> &mut CitizenState { &mut self.citizen_state }
}
