use std::sync::{Arc, Mutex};

use egui_citizen::{Citizen, CitizenId, CitizenState};
use glow::HasContext as _;

use crate::render3d::{
    axes::axes_vertices, grid::grid_vertices, Camera, ColoredMesh, UnlitProgram,
};

/// Phase-2 3D viewport: axes gizmo + ground grid + mouse-orbit + wheel-zoom.
/// See `develop/task3d-plan.md` for what each phase adds.
pub struct GerberView3dPanel {
    citizen_id: CitizenId,
    citizen_state: CitizenState,
    camera: Camera,
    /// Lazily created on the first frame where a gl context is available.
    gpu: Option<Arc<Mutex<GpuResources>>>,
}

struct GpuResources {
    unlit: UnlitProgram,
    axes: ColoredMesh,
    grid: ColoredMesh,
}

impl GerberView3dPanel {
    pub fn new(citizen_state: CitizenState) -> Self {
        Self {
            citizen_id: CitizenId::new("gerber_view_3d"),
            citizen_state,
            camera: Camera::default(),
            gpu: None,
        }
    }

    pub fn show(
        &mut self,
        ui: &mut egui::Ui,
        gl: Option<&Arc<glow::Context>>,
    ) {
        let (rect, response) =
            ui.allocate_exact_size(ui.available_size(), egui::Sense::click_and_drag());

        // Background fill so the 3D viewport is visually distinct.
        ui.painter().rect_filled(
            rect,
            0.0,
            egui::Color32::from_rgb(12, 14, 20),
        );

        // Orbit on left-drag.
        if response.dragged_by(egui::PointerButton::Primary) {
            self.camera.orbit(response.drag_delta());
        }
        // Wheel zoom — only when cursor is over the viewport.
        if response.hovered() {
            let scroll = ui.input(|i| i.raw_scroll_delta.y);
            if scroll.abs() > 0.0 {
                self.camera.zoom_by(1.0 + scroll * 0.001);
            }
        }

        let Some(gl) = gl else {
            ui.painter().text(
                rect.center(),
                egui::Align2::CENTER_CENTER,
                "3D view requires eframe `glow` backend",
                egui::FontId::proportional(14.0),
                egui::Color32::YELLOW,
            );
            return;
        };

        // Lazy init — needs a gl context, only available inside update().
        let gpu = self
            .gpu
            .get_or_insert_with(|| {
                let resources = unsafe {
                    let unlit = UnlitProgram::new(gl);

                    let mut axes = ColoredMesh::new(gl, glow::LINES);
                    axes.upload(gl, &axes_vertices(3.0, 0.001));

                    let mut grid = ColoredMesh::new(gl, glow::LINES);
                    grid.upload(gl, &grid_vertices(5.0, 1.0, [0.28, 0.30, 0.35]));

                    GpuResources { unlit, axes, grid }
                };
                Arc::new(Mutex::new(resources))
            })
            .clone();

        let mvp = self.camera.mvp(rect);
        let callback = egui_glow::CallbackFn::new(move |_info, painter| {
            let gl = painter.gl();
            let Ok(g) = gpu.lock() else { return };
            unsafe {
                gl.enable(glow::DEPTH_TEST);
                gl.depth_func(glow::LEQUAL);
                gl.clear(glow::DEPTH_BUFFER_BIT);
                g.unlit.bind(gl, &mvp);
                // Grid first (thin), axes on top (thick) — depth buffer keeps
                // the axes visible where they cross the grid's centerline
                // (centerlines are skipped in the grid mesh anyway).
                gl.line_width(1.0);
                g.grid.draw(gl);
                gl.line_width(2.5);
                g.axes.draw(gl);
                gl.disable(glow::DEPTH_TEST);
            }
        });
        ui.painter().add(egui::PaintCallback {
            rect,
            callback: Arc::new(callback),
        });

        // Force continuous repaint so orbit drags feel smooth.
        ui.ctx().request_repaint();
    }
}

impl Citizen for GerberView3dPanel {
    fn id(&self) -> &CitizenId { &self.citizen_id }
    fn state(&self) -> &CitizenState { &self.citizen_state }
    fn state_mut(&mut self) -> &mut CitizenState { &mut self.citizen_state }
}
