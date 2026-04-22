use std::sync::{Arc, Mutex};

use egui_citizen::{Citizen, CitizenId, CitizenState};
use glow::HasContext as _;

use crate::gerber_geom::OutlineData;
use crate::render3d::{
    axes::axes_vertices, grid::grid_vertices, Camera, ColoredMesh, UnlitProgram,
};

/// FR-4 green for the flat board mesh. Matches the conventional soldermask
/// tone so a board rendered without mask+copper still reads as a PCB.
const FR4_COLOR: [f32; 3] = [0.18, 0.42, 0.22];

/// Phase-3 3D viewport: axes gizmo + ground grid + flat board outline,
/// sourced from the gerber polygon IR (FDD Stage 6 output).
pub struct GerberView3dPanel {
    citizen_id: CitizenId,
    citizen_state: CitizenState,
    camera: Camera,
    /// Lazily created on the first frame where a gl context is available.
    gpu: Option<Arc<Mutex<GpuResources>>>,
    /// Whether the last uploaded board mesh came from a real outline. Flips
    /// between `false` (nothing loaded) and `true` (outline present). The
    /// upload pass uses this to skip re-uploading identical geometry every
    /// frame while still picking up newly loaded projects.
    last_had_outline: bool,
}

struct GpuResources {
    unlit: UnlitProgram,
    axes: ColoredMesh,
    grid: ColoredMesh,
    /// Flat board outline. Triangle soup with FR-4 colour. Empty until a
    /// project with a parseable Edge.Cuts gerber loads.
    board: ColoredMesh,
    board_ready: bool,
}

impl GerberView3dPanel {
    pub fn new(citizen_state: CitizenState) -> Self {
        Self {
            citizen_id: CitizenId::new("gerber_view_3d"),
            citizen_state,
            camera: Camera::default(),
            gpu: None,
            last_had_outline: false,
        }
    }

    pub fn show(
        &mut self,
        ui: &mut egui::Ui,
        gl: Option<&Arc<glow::Context>>,
        board_outline: Option<&OutlineData>,
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

                    let board = ColoredMesh::new(gl, glow::TRIANGLES);

                    GpuResources { unlit, axes, grid, board, board_ready: false }
                };
                Arc::new(Mutex::new(resources))
            })
            .clone();

        // Upload (or re-upload) the board mesh whenever the outline transitions
        // between absent ↔ present. Outline content is stable for a given
        // project, so we don't need a deeper comparison than that.
        let has_outline = board_outline.is_some();
        if has_outline != self.last_had_outline {
            if let (Some(outline), Ok(mut g)) = (board_outline, gpu.lock()) {
                let verts = build_board_vertices(outline, FR4_COLOR, 0.0);
                unsafe {
                    g.board.upload(gl, &verts);
                }
                g.board_ready = true;
            } else if let Ok(mut g) = gpu.lock() {
                g.board_ready = false;
            }
            self.last_had_outline = has_outline;
        }

        let mvp = self.camera.mvp(rect);
        let callback = egui_glow::CallbackFn::new(move |_info, painter| {
            let gl = painter.gl();
            let Ok(g) = gpu.lock() else { return };
            unsafe {
                gl.enable(glow::DEPTH_TEST);
                gl.depth_func(glow::LEQUAL);
                gl.depth_mask(true);
                gl.clear(glow::DEPTH_BUFFER_BIT);
                g.unlit.bind(gl, &mvp);
                // Board (flat FR-4 fill) first so line primitives land on top
                // via the depth test and the axes gizmo (lifted by z_base)
                // wins at the centerlines.
                if g.board_ready {
                    g.board.draw(gl);
                }
                gl.line_width(1.0);
                g.grid.draw(gl);
                gl.line_width(2.5);
                g.axes.draw(gl);
                gl.depth_mask(false);
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

/// Convert the gerber_geom outline mesh into the `xyz rgb` flat buffer that
/// `ColoredMesh::upload` expects, placing every vertex at `z` with the given
/// RGB colour.
fn build_board_vertices(outline: &OutlineData, rgb: [f32; 3], z: f32) -> Vec<f32> {
    let [r, g, b] = rgb;
    let mut out = Vec::with_capacity(outline.mesh_indices.len() * 6);
    for &idx in &outline.mesh_indices {
        let v = outline.mesh_vertices_2d[idx as usize];
        out.extend_from_slice(&[v[0], v[1], z, r, g, b]);
    }
    out
}
