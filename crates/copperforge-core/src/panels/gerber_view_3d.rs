use std::sync::{Arc, Mutex};

use egui_citizen::{Citizen, CitizenId, CitizenState};
use glow::HasContext as _;

use crate::gerber_geom::OutlineData;
use crate::render3d::{
    axes::axes_vertices, grid::grid_vertices, project, unproject_to_z0, Camera, ColoredMesh,
    UnlitProgram,
};
use nalgebra::Vector3;

/// FR-4 green for the flat board mesh. Matches the conventional soldermask
/// tone so a board rendered without mask+copper still reads as a PCB.
const FR4_COLOR: [f32; 3] = [0.18, 0.42, 0.22];

const GRID_COLOR: [f32; 3] = [0.28, 0.30, 0.35];

const MM_PER_MIL: f32 = 0.0254;

const RIBBON_HEIGHT: f32 = 26.0;

/// Grid-step selection surfaced by the ribbon ComboBox. Manual picks are
/// stored in mm so toggling the display unit between mm and mils doesn't
/// drift the world-space grid; the ribbon label just translates on the fly.
#[derive(Clone, Copy, PartialEq)]
enum GridStep {
    /// Scales with board size + the active display unit — picks a natural
    /// 1-2-5 step in that unit sized to ~1/20th of the board's largest
    /// dimension, so the cell count reads as a round number either way.
    Auto,
    /// User-chosen step, stored in mm.
    Manual(f32),
}

/// Phase-3 3D viewport: axes gizmo + ground grid + flat board outline,
/// sourced from the gerber polygon IR (FDD Stage 6 output). A top ribbon
/// hosts the grid-step ComboBox, which tracks the global display unit.
pub struct GerberView3dPanel {
    citizen_id: CitizenId,
    citizen_state: CitizenState,
    camera: Camera,
    /// Lazily created on the first frame where a gl context is available.
    gpu: Option<Arc<Mutex<GpuResources>>>,
    /// Whether the last uploaded board mesh came from a real outline.
    last_had_outline: bool,
    /// Board dims (mm) cached from the last uploaded outline. Drives the
    /// grid's half-extent + Auto step picker.
    last_board_dim: Option<(f32, f32)>,
    /// User-selected grid step. Persisted across outline loads; the grid
    /// mesh re-uploads whenever the resolved mm value changes (which can
    /// happen from a new board, a new manual pick, or a unit toggle under
    /// Auto — where the step is re-picked in the active unit).
    grid_step: GridStep,
    /// Step in mm currently on the GPU. Change-detection key.
    last_uploaded_grid_step_mm: Option<f32>,
    /// Display unit at the last grid upload. Auto mode re-picks its step
    /// in the active unit when this flips, so a 30 mm board that shows
    /// "5 mm" under mm flips to "250 mils" under mils — round number in
    /// either unit rather than "394 mils" converted from 10 mm.
    last_units_mils: bool,
    /// Right-mouse-drag zoom-to-region: anchor pixel on drag start, live
    /// pixel during drag. On release we un-project both corners to the
    /// Z=0 plane and retarget + rescale the camera to frame the selection.
    zoom_box_start: Option<egui::Pos2>,
    zoom_box_current: Option<egui::Pos2>,
    /// Ground grid visibility. Toggled by the ribbon button + `G` hotkey
    /// (when pointer is over the canvas, so typing G in other panels
    /// doesn't flip it).
    show_grid: bool,
    /// Current axis-gizmo length (mm). Scales with the loaded board so the
    /// gizmo reads as a meaningful reference rather than a microscopic
    /// stub on large boards or a board-swamping monster on small ones.
    axes_len: f32,
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
            last_units_mils: false,
            zoom_box_start: None,
            zoom_box_current: None,
            show_grid: true,
            axes_len: 3.0,
        }
    }

    pub fn show(
        &mut self,
        ui: &mut egui::Ui,
        gl: Option<&Arc<glow::Context>>,
        board_outline: Option<&OutlineData>,
        units_mils: bool,
    ) {
        // ── Ribbon ─────────────────────────────────────────────────
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
        self.show_ribbon(&mut ribbon_ui, board_outline, units_mils);

        // ── 3D canvas ──────────────────────────────────────────────
        let mut canvas_ui = ui.new_child(egui::UiBuilder::new().max_rect(canvas_rect));
        self.show_canvas(&mut canvas_ui, gl, board_outline, units_mils);
    }

    fn show_ribbon(
        &mut self,
        ui: &mut egui::Ui,
        board_outline: Option<&OutlineData>,
        units_mils: bool,
    ) {
        ui.horizontal(|ui| {
            ui.add_space(6.0);
            ui.toggle_value(&mut self.show_grid, "Grid")
                .on_hover_text("Toggle ground grid (G when cursor is over the 3D view)");
            ui.add_space(6.0);
            if ui.button("Reset View")
                .on_hover_text("Restore default tilt and fit the board to the viewport (double-click the canvas does the same).")
                .clicked()
            {
                self.reset_view(board_outline);
            }
            ui.add_space(12.0);
            ui.label("Grid:");

            // What step is Auto resolving to right now — used for the
            // ribbon's "Auto (<step>)" label so the user can tell at a
            // glance what the grid is actually showing.
            let auto_step_mm = auto_grid_step_mm(
                board_outline.map(|o| {
                    (
                        (o.bbox.max.x - o.bbox.min.x) as f32,
                        (o.bbox.max.y - o.bbox.min.y) as f32,
                    )
                }),
                units_mils,
            );

            let selected_label = match self.grid_step {
                GridStep::Auto => format!("Auto ({})", format_step(auto_step_mm, units_mils)),
                GridStep::Manual(mm) => format_step(mm, units_mils),
            };

            egui::ComboBox::from_id_salt("gerber_view_3d_grid_step")
                .selected_text(selected_label)
                .width(130.0)
                .show_ui(ui, |ui| {
                    ui.selectable_value(
                        &mut self.grid_step,
                        GridStep::Auto,
                        format!("Auto ({})", format_step(auto_step_mm, units_mils)),
                    );
                    for &choice in grid_step_choices(units_mils) {
                        let mm = if units_mils { choice * MM_PER_MIL } else { choice };
                        ui.selectable_value(
                            &mut self.grid_step,
                            GridStep::Manual(mm),
                            format_step(mm, units_mils),
                        );
                    }
                });

            ui.add_space(12.0);
            ui.label(format!("Units: {}", if units_mils { "mils" } else { "mm" }));
            if let Some(outline) = board_outline {
                let w = (outline.bbox.max.x - outline.bbox.min.x) as f32;
                let h = (outline.bbox.max.y - outline.bbox.min.y) as f32;
                ui.add_space(12.0);
                ui.label(format!(
                    "Board: {} × {}",
                    format_dim(w, units_mils),
                    format_dim(h, units_mils),
                ));
            }
        });
    }

    fn show_canvas(
        &mut self,
        ui: &mut egui::Ui,
        gl: Option<&Arc<glow::Context>>,
        board_outline: Option<&OutlineData>,
        units_mils: bool,
    ) {
        let (rect, response) =
            ui.allocate_exact_size(ui.available_size(), egui::Sense::click_and_drag());

        ui.painter().rect_filled(
            rect,
            0.0,
            egui::Color32::from_rgb(12, 14, 20),
        );

        if response.dragged_by(egui::PointerButton::Primary) {
            self.camera.orbit(response.drag_delta());
        }
        // Double-click anywhere on the canvas to restore the default view.
        if response.double_clicked_by(egui::PointerButton::Primary) {
            self.reset_view(board_outline);
        }
        // Right-mouse-drag zoom-to-region. Track start + live pixel during
        // the drag; commit on release once the final MVP is known.
        if response.drag_started_by(egui::PointerButton::Secondary) {
            if let Some(p) = response.interact_pointer_pos() {
                self.zoom_box_start = Some(p);
                self.zoom_box_current = Some(p);
            }
        }
        if response.dragged_by(egui::PointerButton::Secondary) {
            if let Some(p) = response.interact_pointer_pos() {
                self.zoom_box_current = Some(p);
            }
        }
        let zoom_box_released = response.drag_stopped_by(egui::PointerButton::Secondary);
        if response.hovered() {
            let scroll = ui.input(|i| i.raw_scroll_delta.y);
            if scroll.abs() > 0.0 {
                self.camera.zoom_by(1.0 + scroll * 0.001);
            }
            // `G` toggles grid only when the pointer is over this view, so
            // typing G in another panel (e.g. the Terminal) doesn't flip
            // the grid under the user.
            if ui.input(|i| i.key_pressed(egui::Key::G)) {
                self.show_grid = !self.show_grid;
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

        let gpu = self
            .gpu
            .get_or_insert_with(|| {
                let resources = unsafe {
                    let unlit = UnlitProgram::new(gl);

                    let mut axes = ColoredMesh::new(gl, glow::LINES);
                    axes.upload(gl, &axes_vertices(3.0, 0.001));

                    let mut grid = ColoredMesh::new(gl, glow::LINES);
                    grid.upload(gl, &grid_vertices(5.0, 1.0, GRID_COLOR));

                    let board = ColoredMesh::new(gl, glow::TRIANGLES);

                    GpuResources { unlit, axes, grid, board, board_ready: false }
                };
                Arc::new(Mutex::new(resources))
            })
            .clone();

        // ── Board mesh (absent ↔ present transition) ───────────────
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
                self.camera.fit_to_bbox(w, h);
                // Scale the axes gizmo to ~15 % of the board's largest
                // dimension so it reads as a meaningful reference instead
                // of a microscopic stub or a board-swamping monster.
                self.axes_len = (w.max(h) * 0.15).max(3.0);
                unsafe {
                    g.axes.upload(gl, &axes_vertices(self.axes_len, 0.001));
                }
            } else if let Ok(mut g) = gpu.lock() {
                g.board_ready = false;
                self.last_board_dim = None;
                self.axes_len = 3.0;
                unsafe {
                    g.axes.upload(gl, &axes_vertices(self.axes_len, 0.001));
                }
            }
            self.last_had_outline = has_outline;
            self.last_uploaded_grid_step_mm = None;
        }

        // ── Grid mesh (when resolved step changes) ─────────────────
        // Unit toggle under Auto re-picks the step in the active unit; we
        // detect that via `last_units_mils`.
        if self.last_units_mils != units_mils {
            self.last_uploaded_grid_step_mm = None;
            self.last_units_mils = units_mils;
        }
        let (grid_step_mm, grid_half_extent) = match self.last_board_dim {
            Some((w, h)) => {
                let step = resolve_grid_step_mm(self.grid_step, Some((w, h)), units_mils);
                let half_extent = (w.max(h) * 0.75).max(step * 3.0);
                (step, half_extent)
            }
            None => {
                let step = resolve_grid_step_mm(self.grid_step, None, units_mils);
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

        // On right-drag release: un-project the two screen corners onto the
        // Z=0 plane, retarget the camera to the region's center, and scale
        // zoom so the selection fills the viewport.
        if zoom_box_released {
            if let (Some(s), Some(e)) = (self.zoom_box_start, self.zoom_box_current) {
                // Ignore accidental clicks (< 8 px on both axes).
                if (s.x - e.x).abs() >= 8.0 || (s.y - e.y).abs() >= 8.0 {
                    if let Some(mvp_inv) = mvp.try_inverse() {
                        if let (Some(ws), Some(we)) = (
                            unproject_to_z0(&mvp_inv, rect, s),
                            unproject_to_z0(&mvp_inv, rect, e),
                        ) {
                            let min_x = ws.x.min(we.x);
                            let max_x = ws.x.max(we.x);
                            let min_y = ws.y.min(we.y);
                            let max_y = ws.y.max(we.y);
                            let w = max_x - min_x;
                            let h = max_y - min_y;
                            if w > 0.0 && h > 0.0 {
                                self.camera.target = Vector3::new(
                                    (min_x + max_x) * 0.5,
                                    (min_y + max_y) * 0.5,
                                    0.0,
                                );
                                self.camera.fit_to_bbox(w, h);
                            }
                        }
                    }
                }
            }
            self.zoom_box_start = None;
            self.zoom_box_current = None;
        }

        let show_grid = self.show_grid;
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
                if show_grid {
                    gl.line_width(1.0);
                    g.grid.draw(gl);
                }
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

        // ── HUD: XYZ labels just past each axis tip ───────────────
        // For each axis we project the origin and the tip, then push the
        // label a fixed screen-space distance *past* the tip along the
        // origin→tip direction so the letter never sits on top of the
        // coloured line it's labelling.
        let l = self.axes_len;
        let label_offset_px = 14.0_f32;
        let axes = [
            (Vector3::new(l, 0.0, 0.0), "X", egui::Color32::from_rgb(255, 90, 90)),
            (Vector3::new(0.0, l, 0.0), "Y", egui::Color32::from_rgb(90, 220, 90)),
            (Vector3::new(0.0, 0.0, l), "Z", egui::Color32::from_rgb(110, 150, 255)),
        ];
        let font = egui::FontId::monospace(13.0);
        let origin_screen = project(&mvp, rect, Vector3::zeros());
        for (end, text, color) in axes {
            let Some(tip) = project(&mvp, rect, end) else { continue };
            let pos = match origin_screen {
                Some(origin) => {
                    let dir = tip - origin;
                    let len = (dir.x * dir.x + dir.y * dir.y).sqrt();
                    if len > 0.5 {
                        egui::Pos2::new(
                            tip.x + dir.x / len * label_offset_px,
                            tip.y + dir.y / len * label_offset_px,
                        )
                    } else {
                        tip
                    }
                }
                None => tip,
            };
            if rect.contains(pos) {
                ui.painter().text(pos, egui::Align2::CENTER_CENTER, text, font.clone(), color);
            }
        }

        // ── Right-drag zoom-box overlay ───────────────────────────
        if let (Some(s), Some(e)) = (self.zoom_box_start, self.zoom_box_current) {
            let box_rect = egui::Rect::from_two_pos(s, e).intersect(rect);
            ui.painter().rect_filled(
                box_rect,
                0.0,
                egui::Color32::from_rgba_unmultiplied(255, 200, 60, 40),
            );
            ui.painter().rect_stroke(
                box_rect,
                0.0,
                egui::Stroke::new(1.5, egui::Color32::from_rgb(255, 200, 60)),
                egui::StrokeKind::Inside,
            );
        }

        ui.ctx().request_repaint();
    }
}

impl GerberView3dPanel {
    /// Restore the default tilt and re-fit the board (if any) to the
    /// viewport. Bound to the ribbon button + double-click on the canvas.
    fn reset_view(&mut self, board_outline: Option<&OutlineData>) {
        self.camera.reset_top_down();
        if let Some(outline) = board_outline {
            let w = (outline.bbox.max.x - outline.bbox.min.x) as f32;
            let h = (outline.bbox.max.y - outline.bbox.min.y) as f32;
            self.camera.fit_to_bbox(w, h);
        }
    }
}

impl Citizen for GerberView3dPanel {
    fn id(&self) -> &CitizenId { &self.citizen_id }
    fn state(&self) -> &CitizenState { &self.citizen_state }
    fn state_mut(&mut self) -> &mut CitizenState { &mut self.citizen_state }
}

// ────────────────────────────────────────────────────────────────────────
// Mesh helpers
// ────────────────────────────────────────────────────────────────────────

fn build_board_vertices(outline: &OutlineData, rgb: [f32; 3], z: f32) -> Vec<f32> {
    let [r, g, b] = rgb;
    let mut out = Vec::with_capacity(outline.mesh_indices.len() * 6);
    for &idx in &outline.mesh_indices {
        let v = outline.mesh_vertices_2d[idx as usize];
        out.extend_from_slice(&[v[0], v[1], z, r, g, b]);
    }
    out
}

// ────────────────────────────────────────────────────────────────────────
// Grid step helpers (ported from the archived PCB-direct iteration —
// unit-aware natural step picker + ComboBox menu population)
// ────────────────────────────────────────────────────────────────────────

/// Pick the smallest value in a sorted "natural step" series that yields
/// roughly ~20 cells across the board's largest dimension. Values are in
/// whatever unit the caller prefers; the caller converts to mm afterwards.
fn pick_natural_step(max_dim: f32, series: &[f32]) -> f32 {
    let target = max_dim / 20.0;
    for &c in series {
        if c >= target {
            return c;
        }
    }
    *series.last().unwrap_or(&1.0)
}

/// Round mm steps engineers actually type into CAD tools.
fn grid_step_mm_for(max_dim_mm: f32) -> f32 {
    const STEPS: &[f32] = &[0.1, 0.25, 0.5, 1.0, 2.5, 5.0, 10.0, 25.0, 50.0, 100.0, 250.0];
    pick_natural_step(max_dim_mm, STEPS)
}

/// Round mil steps engineers actually type into CAD tools. User-preferred
/// set: 20 / 50 / 100 / 250 / 500 / 1000 mils.
fn grid_step_mils_for(max_dim_mils: f32) -> f32 {
    const STEPS: &[f32] = &[20.0, 50.0, 100.0, 250.0, 500.0, 1000.0];
    pick_natural_step(max_dim_mils, STEPS)
}

/// Auto-picked step in mm based on board size + active display unit. Picks
/// in the active unit so the cell count across the board is a round
/// number either way — "5 mm" or "250 mils" rather than "394 mils"
/// converted from 10 mm.
fn auto_grid_step_mm(board_dim: Option<(f32, f32)>, units_mils: bool) -> f32 {
    let max_dim_mm = board_dim
        .map(|(w, h)| w.max(h))
        .filter(|d| *d > 0.0)
        .unwrap_or(10.0);
    if units_mils {
        grid_step_mils_for(max_dim_mm / MM_PER_MIL) * MM_PER_MIL
    } else {
        grid_step_mm_for(max_dim_mm)
    }
}

fn resolve_grid_step_mm(
    step: GridStep,
    board_dim: Option<(f32, f32)>,
    units_mils: bool,
) -> f32 {
    match step {
        GridStep::Auto => auto_grid_step_mm(board_dim, units_mils),
        GridStep::Manual(mm) => mm,
    }
}

/// Natural-step options the ribbon ComboBox offers in the active unit.
fn grid_step_choices(units_mils: bool) -> &'static [f32] {
    if units_mils {
        &[20.0, 50.0, 100.0, 250.0, 500.0, 1000.0]
    } else {
        &[0.1, 0.25, 0.5, 1.0, 2.5, 5.0, 10.0, 25.0, 50.0, 100.0]
    }
}

// ────────────────────────────────────────────────────────────────────────
// Number formatting
// ────────────────────────────────────────────────────────────────────────

fn fmt_trim(v: f32) -> String {
    if (v - v.round()).abs() < 1e-4 {
        format!("{:.0}", v)
    } else {
        format!("{:.3}", v)
            .trim_end_matches('0')
            .trim_end_matches('.')
            .to_string()
    }
}

fn format_step(mm: f32, units_mils: bool) -> String {
    if units_mils {
        format!("{} mils", fmt_trim(mm / MM_PER_MIL))
    } else {
        format!("{} mm", fmt_trim(mm))
    }
}

fn format_dim(mm: f32, units_mils: bool) -> String {
    if units_mils {
        format!("{} mils", fmt_trim(mm / MM_PER_MIL))
    } else {
        format!("{} mm", fmt_trim(mm))
    }
}
