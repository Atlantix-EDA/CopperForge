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

const GRID_COLOR: [f32; 3] = [0.28, 0.30, 0.35];

const MM_PER_MIL: f32 = 0.0254;

const RIBBON_HEIGHT: f32 = 26.0;

/// Grid-step selection surfaced by the ribbon ComboBox. Manual picks are
/// stored in mils (matching the human-facing unit), then converted to mm
/// before the grid mesh is built.
#[derive(Clone, Copy, PartialEq)]
enum GridStep {
    /// Scales with board size — picks a natural 1-2-5 mm step sized to
    /// ~1/10th of the board's largest dimension.
    Auto,
    /// User-chosen step in mils.
    Mils(f32),
}

impl GridStep {
    fn label(&self) -> String {
        match self {
            GridStep::Auto => "Auto".to_string(),
            GridStep::Mils(m) => format!("{} mils", *m as i32),
        }
    }

    /// Resolve to a concrete step in mm for the given board dimensions.
    fn to_mm(self, board_max_dim_mm: f32) -> f32 {
        match self {
            GridStep::Auto => pick_natural_step(board_max_dim_mm / 10.0),
            GridStep::Mils(m) => m * MM_PER_MIL,
        }
    }
}

const MIL_CHOICES: &[f32] = &[20.0, 50.0, 100.0, 250.0, 500.0];

/// Phase-3 3D viewport: axes gizmo + ground grid + flat board outline,
/// sourced from the gerber polygon IR (FDD Stage 6 output). A top ribbon
/// hosts the grid-step ComboBox.
pub struct GerberView3dPanel {
    citizen_id: CitizenId,
    citizen_state: CitizenState,
    camera: Camera,
    /// Lazily created on the first frame where a gl context is available.
    gpu: Option<Arc<Mutex<GpuResources>>>,
    /// Whether the last uploaded board mesh came from a real outline. Flips
    /// between `false` (nothing loaded) and `true` (outline present). Used
    /// to decide when to re-upload the board mesh and re-fit the camera.
    last_had_outline: bool,
    /// Board dims (mm) cached from the last uploaded outline. The grid
    /// mesh is rebuilt off these whenever the step changes.
    last_board_dim: Option<(f32, f32)>,
    /// User-selected grid step. Persisted across outline loads; the grid
    /// mesh re-uploads whenever the resolved mm step differs from the
    /// value last sent to the GPU.
    grid_step: GridStep,
    /// Step in mm actually on the GPU right now, so we only re-upload when
    /// the resolved value changes (e.g. Auto → step-per-board, or user
    /// picking a new mil value).
    last_uploaded_grid_step_mm: Option<f32>,
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
            last_board_dim: None,
            grid_step: GridStep::Auto,
            last_uploaded_grid_step_mm: None,
        }
    }

    pub fn show(
        &mut self,
        ui: &mut egui::Ui,
        gl: Option<&Arc<glow::Context>>,
        board_outline: Option<&OutlineData>,
    ) {
        // ── Ribbon ─────────────────────────────────────────────────
        // Carve a fixed-height strip off the top of the panel for the
        // grid-step ComboBox. The remainder is the 3D canvas.
        let total = ui.available_rect_before_wrap();
        let ribbon_rect = egui::Rect::from_min_max(
            total.min,
            egui::Pos2::new(total.max.x, total.min.y + RIBBON_HEIGHT),
        );
        let canvas_rect = egui::Rect::from_min_max(
            egui::Pos2::new(total.min.x, total.min.y + RIBBON_HEIGHT),
            total.max,
        );

        let mut ribbon_ui = ui.new_child(
            egui::UiBuilder::new()
                .max_rect(ribbon_rect)
                .layout(egui::Layout::left_to_right(egui::Align::Center)),
        );
        self.show_ribbon(&mut ribbon_ui);

        // ── 3D canvas ──────────────────────────────────────────────
        let mut canvas_ui = ui.new_child(egui::UiBuilder::new().max_rect(canvas_rect));
        self.show_canvas(&mut canvas_ui, gl, board_outline);
    }

    fn show_ribbon(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.add_space(6.0);
            ui.label("Grid:");
            egui::ComboBox::from_id_salt("gerber_view_3d_grid_step")
                .selected_text(self.grid_step.label())
                .width(110.0)
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut self.grid_step, GridStep::Auto, "Auto");
                    for &m in MIL_CHOICES {
                        ui.selectable_value(
                            &mut self.grid_step,
                            GridStep::Mils(m),
                            format!("{} mils", m as i32),
                        );
                    }
                });
        });
    }

    fn show_canvas(
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

                    // Placeholder grid — replaced on first outline load /
                    // first grid-step change, whichever comes first.
                    let mut grid = ColoredMesh::new(gl, glow::LINES);
                    grid.upload(gl, &grid_vertices(5.0, 1.0, GRID_COLOR));

                    let board = ColoredMesh::new(gl, glow::TRIANGLES);

                    GpuResources { unlit, axes, grid, board, board_ready: false }
                };
                Arc::new(Mutex::new(resources))
            })
            .clone();

        // ── Board mesh upload (absent ↔ present transition) ────────
        let has_outline = board_outline.is_some();
        if has_outline != self.last_had_outline {
            if let (Some(outline), Ok(mut g)) = (board_outline, gpu.lock()) {
                let w = (outline.bbox.max.x - outline.bbox.min.x) as f32;
                let h = (outline.bbox.max.y - outline.bbox.min.y) as f32;
                let verts = build_board_vertices(outline, FR4_COLOR, 0.0);
                unsafe {
                    g.board.upload(gl, &verts);
                }
                g.board_ready = true;
                self.last_board_dim = Some((w, h));
                // Auto-fit the camera to the new board's extent.
                self.camera.fit_to_bbox(w, h);
            } else if let Ok(mut g) = gpu.lock() {
                g.board_ready = false;
                self.last_board_dim = None;
            }
            self.last_had_outline = has_outline;
            // Force the grid to re-evaluate at the next step — board dim
            // changed, and Auto's resolved step scales with it.
            self.last_uploaded_grid_step_mm = None;
        }

        // ── Grid mesh upload (when resolved step changes) ──────────
        // If no board is loaded, fall back to a fixed 10 mm reference grid
        // so there's still spatial context on empty projects.
        let (grid_step_mm, grid_half_extent) = match self.last_board_dim {
            Some((w, h)) => {
                let max_dim = w.max(h).max(1.0);
                let step = self.grid_step.to_mm(max_dim);
                let half_extent = (max_dim * 0.75).max(step * 3.0);
                (step, half_extent)
            }
            None => {
                let step = match self.grid_step {
                    GridStep::Auto => 1.0,
                    GridStep::Mils(m) => m * MM_PER_MIL,
                };
                (step, (step * 5.0).max(5.0))
            }
        };
        if self.last_uploaded_grid_step_mm != Some(grid_step_mm) {
            if let Ok(mut g) = gpu.lock() {
                let verts = grid_vertices(grid_half_extent, grid_step_mm, GRID_COLOR);
                unsafe {
                    g.grid.upload(gl, &verts);
                }
            }
            self.last_uploaded_grid_step_mm = Some(grid_step_mm);
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

fn pick_natural_step(target: f32) -> f32 {
    const STEPS: &[f32] = &[0.1, 0.25, 0.5, 1.0, 2.5, 5.0, 10.0, 25.0, 50.0, 100.0];
    for &s in STEPS {
        if s >= target {
            return s;
        }
    }
    *STEPS.last().unwrap()
}
