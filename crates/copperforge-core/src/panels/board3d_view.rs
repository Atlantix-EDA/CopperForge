//! Citizen-free 3D board view — the rendering core shared by the native
//! dock panel (`gerber_view_3d.rs`, which wraps this in an `egui_citizen`
//! `Citizen`) and the wasm browser app (`copperforge-web`, which drives it
//! directly). Knows about gerber geometry IR + `render3d`; knows nothing
//! about the panel/citizen framework so it compiles cleanly on wasm32
//! without pulling `egui_citizen` (and its egui-version constraints) into
//! the web build.

use std::sync::{Arc, Mutex};

use glow::HasContext as _;

use crate::gerber_geom::{CopperData, DrillData, MaskData, OutlineData};
use crate::render3d::{
    axes::axes_vertices, grid::grid_vertices, project, unproject_to_z0, Camera, ColoredMesh,
    UnlitProgram,
};
use nalgebra::Vector3;

/// FR-4 green for the flat board mesh. Matches the conventional soldermask
/// tone so a board rendered without mask+copper still reads as a PCB.
const FR4_COLOR: [f32; 3] = [0.18, 0.42, 0.22];
/// ENIG gold (#D8AD4F) for F.Cu. Matches what a HASL/ENIG finished board
/// reads as under a green mask.
const COPPER_COLOR_TOP: [f32; 3] = [0.85, 0.68, 0.31];
/// Slightly-redder copper for B.Cu — same family, but distinct enough to
/// call out "this is the bottom" at a glance when both happen to be in
/// view (orbit halfway, edge-on, depth-test sanity check). Until Phase 4b
/// thickens layers with side walls, this is also our primary visual
/// confirmation that the depth test is correctly hiding B.Cu behind the
/// FR-4 from a top view and hiding F.Cu from a bottom view.
const COPPER_COLOR_BOTTOM: [f32; 3] = [0.72, 0.45, 0.20];
/// Inner copper planes read as copper too, a touch dimmer than F.Cu so the
/// buried layers don't visually compete with the outer ones when seen
/// through the translucent board edge.
const INNER_COPPER_COLOR: [f32; 3] = [0.78, 0.60, 0.28];
/// FR-4 substrate is a real slab with side walls (was a flat sheet in the
/// Phase-4a interim). Top face at z=0, bottom one board-thickness below.
/// 1.55 mm is the common 2-layer default and reads as a believable edge.
const BOARD_Z_TOP: f32 = 0.0;
const BOARD_THICKNESS: f32 = 1.55;
const BOARD_Z_BOTTOM: f32 = BOARD_Z_TOP - BOARD_THICKNESS;
/// Caps stay opaque; the side walls render translucent so buried inner
/// copper reads through the board edge.
const BOARD_WALL_ALPHA: f32 = 0.45;

/// Real film thicknesses so layers stack flush on the slab faces instead of
/// floating: 1 oz Cu ≈ 35 µm, LPI mask ≈ 20 µm, silk ink ≈ 15 µm. Each
/// sheet is still zero-thickness; its Z marks the layer's outer surface.
const COPPER_THICKNESS: f32 = 0.035;
const MASK_THICKNESS: f32 = 0.020;
const SILK_THICKNESS: f32 = 0.015;
/// F.Cu outer face = board top + copper plating; B.Cu mirrors on the bottom.
const COPPER_Z_TOP: f32 = BOARD_Z_TOP + COPPER_THICKNESS;
const COPPER_Z_BOTTOM: f32 = BOARD_Z_BOTTOM - COPPER_THICKNESS;

/// Soldermask green (~RAL 6000 territory). A hair darker + more saturated
/// than the bare-FR-4 colour so the mask reads as a distinct layer when
/// both happen to be visible at the same pixel (viewport alignment, edge
/// pixels).
const MASK_COLOR_TOP: [f32; 3] = [0.11, 0.38, 0.18];
const MASK_COLOR_BOTTOM: [f32; 3] = [0.09, 0.32, 0.15];
/// Mask outer face = copper outer face + mask film (conforms over copper).
const MASK_Z_TOP: f32 = COPPER_Z_TOP + MASK_THICKNESS;
const MASK_Z_BOTTOM: f32 = COPPER_Z_BOTTOM - MASK_THICKNESS;
/// Mask blend alpha. 0.55 lets copper traces read through as a darker
/// green tint — matches KiCad's built-in 3D viewer at default settings.
const MASK_ALPHA: f32 = 0.55;

/// Off-white silkscreen legend. Top reads a hair brighter than bottom so the
/// two are distinguishable when both are edge-on in the same view.
const SILK_COLOR_TOP: [f32; 3] = [0.92, 0.92, 0.89];
const SILK_COLOR_BOTTOM: [f32; 3] = [0.88, 0.88, 0.85];
/// Silk ink printed on top of the mask: mask outer face + ink thickness.
const SILK_Z_TOP: f32 = MASK_Z_TOP + SILK_THICKNESS;
const SILK_Z_BOTTOM: f32 = MASK_Z_BOTTOM - SILK_THICKNESS;

const GRID_COLOR: [f32; 3] = [0.28, 0.30, 0.35];

/// Dark hole colour — a drilled hole / barrel reads as near-black against
/// both copper and soldermask.
const HOLE_COLOR: [f32; 3] = [0.06, 0.06, 0.07];
/// Hole-disk Z: a hair above the top mask / below the bottom mask, so the
/// dark disk sits on top of copper+mask on each face and reads as a hole
/// punched through the pad from whichever side faces the camera.
const HOLE_Z_TOP: f32 = MASK_Z_TOP + 0.006;
const HOLE_Z_BOTTOM: f32 = MASK_Z_BOTTOM - 0.006;
/// Triangle-fan segment count for a hole disk. Holes are small on screen, so
/// 16 segments is plenty smooth without ballooning the vertex count on
/// hole-dense boards.
const HOLE_SEGMENTS: usize = 16;

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
pub struct Board3dView {
    camera: Camera,
    /// Lazily created on the first frame where a gl context is available.
    gpu: Option<Arc<Mutex<GpuResources>>>,
    /// Whether the last uploaded board mesh came from a real outline.
    last_had_outline: bool,
    /// Presence flags for copper / mask meshes — re-upload only when the
    /// project loads or unloads each layer's geometry.
    last_had_top_copper: bool,
    last_had_bottom_copper: bool,
    last_had_top_mask: bool,
    last_had_bottom_mask: bool,
    last_had_top_silk: bool,
    last_had_bottom_silk: bool,
    /// Count of inner-copper meshes currently on the GPU. Inner layers are a
    /// `Vec` (variable count), so the meshes rebuild when this count changes.
    last_inner_copper_count: usize,
    /// Presence flag for the drill layer (hole disks).
    last_had_drill: bool,
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
    /// Force a full mesh re-upload on the next `show()`. Set by
    /// [`Board3dView::mark_dirty`] when the caller swaps in new board data.
    /// Without this, the per-layer change detection (which keys on
    /// absent↔present transitions) would keep the previous board's meshes
    /// when loading a *different* board, since `Some → Some` reads as "no
    /// change". Starts `true` so the first frame uploads.
    dirty: bool,
    /// Rotate mode. When on, plain left-drag orbits the board instead of
    /// drawing the zoom-to-region box. A reliable, modifier-free way to
    /// rotate — Ctrl/Shift+left-drag and middle-drag also orbit, but those
    /// depend on the browser delivering modifier/middle-button events, which
    /// isn't guaranteed. The toolbar toggle always works.
    orbit_mode: bool,
    /// Current axis-gizmo length (mm). Scales with the loaded board so the
    /// gizmo reads as a meaningful reference rather than a microscopic
    /// stub on large boards or a board-swamping monster on small ones.
    axes_len: f32,
    /// Measure tool (`M` hotkey). When active, left-drag on the canvas
    /// draws a line between two points on the Z=0 plane instead of
    /// orbiting the camera. Start/end stay visible until `M` is pressed
    /// again to exit measure mode.
    measure_active: bool,
    measure_start: Option<Vector3<f32>>,
    measure_end: Option<Vector3<f32>>,
    measure_dragging: bool,
}

struct GpuResources {
    unlit: UnlitProgram,
    axes: ColoredMesh,
    grid: ColoredMesh,
    /// FR-4 slab caps (top + bottom faces). Triangle soup with FR-4 colour.
    /// Empty until a project with a parseable Edge.Cuts gerber loads.
    board: ColoredMesh,
    /// FR-4 slab side walls (the board edge), drawn translucent so buried
    /// inner copper reads through the edge. Built alongside the caps.
    board_walls: ColoredMesh,
    board_ready: bool,
    top_copper: ColoredMesh,
    top_copper_ready: bool,
    bottom_copper: ColoredMesh,
    bottom_copper_ready: bool,
    /// Inner copper layers (variable count), each baked at its stack depth.
    /// Drawn opaque in the buried pass; read through the translucent edge.
    inner_copper: Vec<ColoredMesh>,
    top_mask: ColoredMesh,
    top_mask_ready: bool,
    bottom_mask: ColoredMesh,
    bottom_mask_ready: bool,
    /// Silkscreen legend meshes (off-white), one per side.
    top_silk: ColoredMesh,
    top_silk_ready: bool,
    bottom_silk: ColoredMesh,
    bottom_silk_ready: bool,
    /// Dark hole disks, one mesh per side (same circles, different Z) so the
    /// holes read from both the top and bottom views.
    top_holes: ColoredMesh,
    bottom_holes: ColoredMesh,
    holes_ready: bool,
}

impl Board3dView {
    pub fn new() -> Self {
        Self {
            camera: Camera::default(),
            gpu: None,
            last_had_outline: false,
            last_had_top_copper: false,
            last_had_bottom_copper: false,
            last_had_top_mask: false,
            last_had_bottom_mask: false,
            last_had_top_silk: false,
            last_had_bottom_silk: false,
            last_inner_copper_count: 0,
            last_had_drill: false,
            last_board_dim: None,
            grid_step: GridStep::Auto,
            last_uploaded_grid_step_mm: None,
            last_units_mils: false,
            zoom_box_start: None,
            zoom_box_current: None,
            show_grid: true,
            dirty: true,
            orbit_mode: false,
            axes_len: 3.0,
            measure_active: false,
            measure_start: None,
            measure_end: None,
            measure_dragging: false,
        }
    }

    /// Tell the view its source geometry changed — forces a full mesh
    /// re-upload (and camera re-fit) on the next `show()`. Call this whenever
    /// the `OutlineData`/`CopperData`/… handed to `show()` now describe a
    /// *different* board than last frame; otherwise the change detection
    /// keeps the old meshes.
    pub fn mark_dirty(&mut self) {
        self.dirty = true;
    }

    /// Flip the camera 180° about world Y — reveals the back of the
    /// board. Bound to the `F` hotkey when the 3D tab is the active
    /// citizen.
    pub fn flip_view(&mut self) {
        self.camera.flip_y();
    }

    /// Rotate the view 90° in-plane (about world Z). Bound to the `R`
    /// hotkey when the 3D tab is the active citizen.
    pub fn rotate_in_plane_90(&mut self) {
        self.camera.rotate_in_plane(std::f32::consts::FRAC_PI_2);
    }

    /// Enter/exit the measure tool. Exiting clears the active drag but
    /// leaves the latched start/end visible so the user can re-enter
    /// measure mode and see what was measured last. Bound to the `M`
    /// hotkey when the 3D tab is the active citizen.
    pub fn toggle_measure(&mut self) -> bool {
        self.measure_active = !self.measure_active;
        self.measure_dragging = false;
        if !self.measure_active {
            // Exiting: clear the in-flight drag but keep the last line
            // visible so the user can see what was measured.
        } else {
            // Entering a fresh measurement: wipe the previous endpoints.
            self.measure_start = None;
            self.measure_end = None;
        }
        self.measure_active
    }

    pub fn show(
        &mut self,
        ui: &mut egui::Ui,
        gl: Option<&Arc<glow::Context>>,
        board_outline: Option<&OutlineData>,
        top_copper: Option<&CopperData>,
        bottom_copper: Option<&CopperData>,
        top_mask: Option<&MaskData>,
        bottom_mask: Option<&MaskData>,
        top_silk: Option<&CopperData>,
        bottom_silk: Option<&CopperData>,
        inner_copper: &[(u8, CopperData)],
        drill: Option<&DrillData>,
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
        self.show_canvas(
            &mut canvas_ui, gl, board_outline,
            top_copper, bottom_copper,
            top_mask, bottom_mask,
            top_silk, bottom_silk,
            inner_copper,
            drill,
            units_mils,
        );
    }

    fn show_ribbon(
        &mut self,
        ui: &mut egui::Ui,
        board_outline: Option<&OutlineData>,
        units_mils: bool,
    ) {
        ui.horizontal(|ui| {
            ui.add_space(6.0);
            ui.toggle_value(&mut self.orbit_mode, "⟲ Rotate")
                .on_hover_text(
                    "When on, left-drag rotates (orbits) the board instead of \
                     drawing the zoom-to-region box. Ctrl/Shift+left-drag and \
                     middle-drag also rotate.",
                );
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

    #[allow(clippy::too_many_arguments)]
    fn show_canvas(
        &mut self,
        ui: &mut egui::Ui,
        gl: Option<&Arc<glow::Context>>,
        board_outline: Option<&OutlineData>,
        top_copper: Option<&CopperData>,
        bottom_copper: Option<&CopperData>,
        top_mask: Option<&MaskData>,
        bottom_mask: Option<&MaskData>,
        top_silk: Option<&CopperData>,
        bottom_silk: Option<&CopperData>,
        inner_copper: &[(u8, CopperData)],
        drill: Option<&DrillData>,
        units_mils: bool,
    ) {
        let (rect, response) =
            ui.allocate_exact_size(ui.available_size(), egui::Sense::click_and_drag());

        ui.painter().rect_filled(
            rect,
            0.0,
            egui::Color32::from_rgb(12, 14, 20),
        );

        // Compute MVP early so we can un-project pixels for the measure
        // tool during the same frame the drag starts.
        let mvp = self.camera.mvp(rect);

        // Ctrl (or Shift, as a fallback) held → left-drag orbits instead of
        // drawing the zoom box.
        let mods = ui.input(|i| i.modifiers);
        // Orbit when the toolbar Rotate toggle is on, OR a modifier is held
        // (Ctrl/Shift) — the toggle is the reliable path; modifiers are a
        // bonus for when the browser actually delivers them.
        let orbit_mod = self.orbit_mode || mods.ctrl || mods.command || mods.shift;

        // Primary-button handling branches on measure-mode. Measure wins:
        // while active, left-drag draws a distance line on Z=0 instead of
        // orbiting the camera. Double-click always resets view regardless.
        if self.measure_active {
            if response.drag_started_by(egui::PointerButton::Primary) {
                if let Some(p) = response.interact_pointer_pos() {
                    if let Some(mvp_inv) = mvp.try_inverse() {
                        if let Some(w) = unproject_to_z0(&mvp_inv, rect, p) {
                            self.measure_start = Some(w);
                            self.measure_end = Some(w);
                            self.measure_dragging = true;
                        }
                    }
                }
            }
            if self.measure_dragging && response.dragged_by(egui::PointerButton::Primary) {
                if let Some(p) = response.interact_pointer_pos() {
                    if let Some(mvp_inv) = mvp.try_inverse() {
                        if let Some(w) = unproject_to_z0(&mvp_inv, rect, p) {
                            self.measure_end = Some(w);
                        }
                    }
                }
            }
            if response.drag_stopped_by(egui::PointerButton::Primary) {
                self.measure_dragging = false;
            }
        } else if orbit_mod {
            // Ctrl/Shift + left-drag = orbit (rotation about the axes). The 2D
            // canvas has no rotation, so this stays gated behind a modifier to
            // keep plain left-drag free for the zoom-to-region box.
            if response.dragged_by(egui::PointerButton::Primary) {
                self.camera.orbit(response.drag_delta());
            }
        } else {
            // Left-drag = zoom-to-region rubber band — matches the 2D gerber
            // canvas. Track start + live pixel during the drag; commit on
            // release once the final MVP is known.
            if response.drag_started_by(egui::PointerButton::Primary) {
                if let Some(p) = response.interact_pointer_pos() {
                    self.zoom_box_start = Some(p);
                    self.zoom_box_current = Some(p);
                }
            }
            if response.dragged_by(egui::PointerButton::Primary) {
                if let Some(p) = response.interact_pointer_pos() {
                    self.zoom_box_current = Some(p);
                }
            }
        }
        let zoom_box_released = !self.measure_active
            && !orbit_mod
            && response.drag_stopped_by(egui::PointerButton::Primary);
        // Double-click anywhere on the canvas to restore the default view.
        if response.double_clicked_by(egui::PointerButton::Primary) {
            self.reset_view(board_outline);
        }
        // Mouse model mirrors the 2D gerber canvas: right-drag pans, wheel
        // zooms (below), plain left-drag is the zoom-to-region box above.
        // Orbit — which 2D has no equivalent for — is on Ctrl+left-drag and
        // also on the middle button for users who have one.
        if response.dragged_by(egui::PointerButton::Secondary) {
            self.camera.pan(response.drag_delta(), rect);
        }
        if response.dragged_by(egui::PointerButton::Middle) {
            self.camera.orbit(response.drag_delta());
        }
        if response.hovered() {
            let scroll = ui.input(|i| i.smooth_scroll_delta.y);
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
                    let board_walls = ColoredMesh::new(gl, glow::TRIANGLES);
                    let top_copper = ColoredMesh::new(gl, glow::TRIANGLES);
                    let bottom_copper = ColoredMesh::new(gl, glow::TRIANGLES);
                    let top_mask = ColoredMesh::new(gl, glow::TRIANGLES);
                    let bottom_mask = ColoredMesh::new(gl, glow::TRIANGLES);
                    let top_silk = ColoredMesh::new(gl, glow::TRIANGLES);
                    let bottom_silk = ColoredMesh::new(gl, glow::TRIANGLES);
                    let top_holes = ColoredMesh::new(gl, glow::TRIANGLES);
                    let bottom_holes = ColoredMesh::new(gl, glow::TRIANGLES);

                    GpuResources {
                        unlit,
                        axes,
                        grid,
                        board,
                        board_walls,
                        board_ready: false,
                        top_copper,
                        top_copper_ready: false,
                        bottom_copper,
                        bottom_copper_ready: false,
                        inner_copper: Vec::new(),
                        top_mask,
                        top_mask_ready: false,
                        bottom_mask,
                        bottom_mask_ready: false,
                        top_silk,
                        top_silk_ready: false,
                        bottom_silk,
                        bottom_silk_ready: false,
                        top_holes,
                        bottom_holes,
                        holes_ready: false,
                    }
                };
                Arc::new(Mutex::new(resources))
            })
            .clone();

        // A new board (or first frame) forces every layer to re-upload,
        // re-evaluating present/absent so stale meshes from the previous
        // board don't linger. Cleared once all layers are processed.
        let force = self.dirty;

        // ── Board mesh (absent ↔ present transition, or forced) ────
        let has_outline = board_outline.is_some();
        if force || has_outline != self.last_had_outline {
            if let (Some(outline), Ok(mut g)) = (board_outline, gpu.lock()) {
                let w = (outline.bbox.max.x - outline.bbox.min.x) as f32;
                let h = (outline.bbox.max.y - outline.bbox.min.y) as f32;
                let caps = build_board_cap_vertices(outline, FR4_COLOR, BOARD_Z_TOP, BOARD_Z_BOTTOM);
                let walls = build_board_wall_vertices(outline, FR4_COLOR, BOARD_Z_TOP, BOARD_Z_BOTTOM);
                unsafe {
                    g.board.upload(gl, &caps);
                    g.board_walls.upload(gl, &walls);
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

        // ── Copper meshes (F.Cu / B.Cu) ───────────────────────────
        let has_top_copper = top_copper.is_some();
        if force || has_top_copper != self.last_had_top_copper {
            if let (Some(cu), Ok(mut g)) = (top_copper, gpu.lock()) {
                let verts = build_copper_vertices(cu, COPPER_COLOR_TOP, COPPER_Z_TOP);
                unsafe {
                    g.top_copper.upload(gl, &verts);
                }
                g.top_copper_ready = true;
            } else if let Ok(mut g) = gpu.lock() {
                g.top_copper_ready = false;
            }
            self.last_had_top_copper = has_top_copper;
        }
        let has_bottom_copper = bottom_copper.is_some();
        if force || has_bottom_copper != self.last_had_bottom_copper {
            if let (Some(cu), Ok(mut g)) = (bottom_copper, gpu.lock()) {
                let verts = build_copper_vertices(cu, COPPER_COLOR_BOTTOM, COPPER_Z_BOTTOM);
                unsafe {
                    g.bottom_copper.upload(gl, &verts);
                }
                g.bottom_copper_ready = true;
            } else if let Ok(mut g) = gpu.lock() {
                g.bottom_copper_ready = false;
            }
            self.last_had_bottom_copper = has_bottom_copper;
        }

        // ── Inner copper layers (rebuild when the count changes) ───
        if force || inner_copper.len() != self.last_inner_copper_count {
            if let Ok(mut g) = gpu.lock() {
                g.inner_copper.clear();
                // Stack height from the deepest inner index: B.Cu is
                // Copper(N), so N = max inner stack index + 1.
                let copper_count = inner_copper
                    .iter()
                    .map(|(n, _)| *n)
                    .max()
                    .map(|m| m + 1)
                    .unwrap_or(2);
                for (n, cu) in inner_copper {
                    let z = inner_layer_z(*n, copper_count);
                    let verts = build_copper_vertices(cu, INNER_COPPER_COLOR, z);
                    let mut mesh = unsafe { ColoredMesh::new(gl, glow::TRIANGLES) };
                    unsafe {
                        mesh.upload(gl, &verts);
                    }
                    g.inner_copper.push(mesh);
                }
            }
            self.last_inner_copper_count = inner_copper.len();
        }

        // ── Soldermask meshes (F.Mask / B.Mask) ───────────────────
        let has_top_mask = top_mask.is_some();
        if force || has_top_mask != self.last_had_top_mask {
            if let (Some(m), Ok(mut g)) = (top_mask, gpu.lock()) {
                let verts = build_mask_vertices(m, MASK_COLOR_TOP, MASK_Z_TOP);
                unsafe {
                    g.top_mask.upload(gl, &verts);
                }
                g.top_mask_ready = true;
            } else if let Ok(mut g) = gpu.lock() {
                g.top_mask_ready = false;
            }
            self.last_had_top_mask = has_top_mask;
        }
        let has_bottom_mask = bottom_mask.is_some();
        if force || has_bottom_mask != self.last_had_bottom_mask {
            if let (Some(m), Ok(mut g)) = (bottom_mask, gpu.lock()) {
                let verts = build_mask_vertices(m, MASK_COLOR_BOTTOM, MASK_Z_BOTTOM);
                unsafe {
                    g.bottom_mask.upload(gl, &verts);
                }
                g.bottom_mask_ready = true;
            } else if let Ok(mut g) = gpu.lock() {
                g.bottom_mask_ready = false;
            }
            self.last_had_bottom_mask = has_bottom_mask;
        }

        // ── Silkscreen meshes (F.SilkS / B.SilkS) ─────────────────
        let has_top_silk = top_silk.is_some();
        if force || has_top_silk != self.last_had_top_silk {
            if let (Some(s), Ok(mut g)) = (top_silk, gpu.lock()) {
                let verts = build_copper_vertices(s, SILK_COLOR_TOP, SILK_Z_TOP);
                unsafe {
                    g.top_silk.upload(gl, &verts);
                }
                g.top_silk_ready = true;
            } else if let Ok(mut g) = gpu.lock() {
                g.top_silk_ready = false;
            }
            self.last_had_top_silk = has_top_silk;
        }
        let has_bottom_silk = bottom_silk.is_some();
        if force || has_bottom_silk != self.last_had_bottom_silk {
            if let (Some(s), Ok(mut g)) = (bottom_silk, gpu.lock()) {
                let verts = build_copper_vertices(s, SILK_COLOR_BOTTOM, SILK_Z_BOTTOM);
                unsafe {
                    g.bottom_silk.upload(gl, &verts);
                }
                g.bottom_silk_ready = true;
            } else if let Ok(mut g) = gpu.lock() {
                g.bottom_silk_ready = false;
            }
            self.last_had_bottom_silk = has_bottom_silk;
        }

        // ── Drill holes (top + bottom dark disks) ──────────────────
        let has_drill = drill.is_some();
        if force || has_drill != self.last_had_drill {
            if let (Some(d), Ok(mut g)) = (drill, gpu.lock()) {
                let top = build_holes_vertices(d, HOLE_COLOR, HOLE_Z_TOP);
                let bottom = build_holes_vertices(d, HOLE_COLOR, HOLE_Z_BOTTOM);
                unsafe {
                    g.top_holes.upload(gl, &top);
                    g.bottom_holes.upload(gl, &bottom);
                }
                g.holes_ready = true;
            } else if let Ok(mut g) = gpu.lock() {
                g.holes_ready = false;
            }
            self.last_had_drill = has_drill;
        }

        // All layers have been re-evaluated for this board; clear the force.
        self.dirty = false;

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
                // Opaque pass first (depth-write on): B.Cu, board (FR-4),
                // F.Cu. Deepest-to-shallowest so the depth buffer settles
                // correctly before the blended pass reads from it.
                if g.bottom_copper_ready {
                    g.bottom_copper.draw(gl);
                }
                // Inner copper planes, buried at their stack depth. Opaque —
                // the FR-4 caps occlude them straight-on, but they read
                // through the translucent edge walls (the multilayer stackup).
                for m in &g.inner_copper {
                    m.draw(gl);
                }
                if g.board_ready {
                    g.board.draw(gl);
                }
                if g.top_copper_ready {
                    g.top_copper.draw(gl);
                }
                // Translucent pass for the solder-mask sheets. Alpha-blend
                // against whatever the opaque pass wrote; depth-write off
                // so the masks don't occlude each other when both are in
                // view (halfway-tilted camera). Back-to-front draw order:
                // B.Mask sits beneath the board and only shows from the
                // bottom side; F.Mask sits above F.Cu and dominates the
                // top-down view.
                gl.enable(glow::BLEND);
                gl.blend_func(glow::SRC_ALPHA, glow::ONE_MINUS_SRC_ALPHA);
                gl.depth_mask(false);
                // Translucent FR-4 side walls first, so the board edge blends
                // over whatever the opaque pass wrote (inner copper, once it
                // lands) and the multilayer stackup reads through the edge.
                if g.board_ready {
                    g.unlit.set_alpha(gl, BOARD_WALL_ALPHA);
                    g.board_walls.draw(gl);
                }
                g.unlit.set_alpha(gl, MASK_ALPHA);
                if g.bottom_mask_ready {
                    g.bottom_mask.draw(gl);
                }
                if g.top_mask_ready {
                    g.top_mask.draw(gl);
                }
                g.unlit.set_alpha(gl, 1.0);
                gl.depth_mask(true);
                gl.disable(glow::BLEND);
                // Drill holes: opaque dark disks on each face, drawn after the
                // mask so they read as crisp holes punched through the pads
                // (depth-write back on, no blend).
                if g.holes_ready {
                    g.top_holes.draw(gl);
                    g.bottom_holes.draw(gl);
                }
                // Silkscreen legend: opaque off-white, drawn last among the
                // board layers (depth-write still on) at a Z just above the
                // mask so it reads as printed on top of the soldermask.
                if g.top_silk_ready {
                    g.top_silk.draw(gl);
                }
                if g.bottom_silk_ready {
                    g.bottom_silk.draw(gl);
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

        // ── Measure tool overlay ──────────────────────────────────
        // Painted over the 3D view (not depth-tested) so the line + label
        // stay visible regardless of viewing angle. Endpoints project
        // through MVP; distance is the world-space (Z=0) length.
        if let (Some(ws), Some(we)) = (self.measure_start, self.measure_end) {
            if let (Some(ps), Some(pe)) = (project(&mvp, rect, ws), project(&mvp, rect, we)) {
                let color = if self.measure_active {
                    egui::Color32::from_rgb(255, 220, 90)
                } else {
                    egui::Color32::from_rgba_unmultiplied(255, 220, 90, 180)
                };
                ui.painter().line_segment([ps, pe], egui::Stroke::new(2.0, color));
                ui.painter().circle_filled(ps, 3.5, color);
                ui.painter().circle_filled(pe, 3.5, color);
                let dist_mm = (we - ws).norm();
                let midpoint = egui::Pos2::new(
                    (ps.x + pe.x) * 0.5,
                    (ps.y + pe.y) * 0.5 - 10.0,
                );
                let text = format_dim(dist_mm, units_mils);
                // Small dark backdrop behind the label so the number stays
                // legible over the FR-4 green.
                let galley = ui.painter().layout_no_wrap(
                    text.clone(),
                    egui::FontId::monospace(13.0),
                    color,
                );
                let bg_rect = egui::Rect::from_center_size(midpoint, galley.size() + egui::vec2(8.0, 4.0));
                ui.painter().rect_filled(
                    bg_rect,
                    3.0,
                    egui::Color32::from_rgba_unmultiplied(0, 0, 0, 180),
                );
                ui.painter().text(
                    midpoint,
                    egui::Align2::CENTER_CENTER,
                    text,
                    egui::FontId::monospace(13.0),
                    color,
                );
            }
        }

        // Measure-mode banner in the top-left of the canvas so the modal
        // input binding (left-drag = measure, not orbit) is visible.
        if self.measure_active {
            let banner_pos = egui::Pos2::new(rect.min.x + 10.0, rect.min.y + 10.0);
            ui.painter().text(
                banner_pos,
                egui::Align2::LEFT_TOP,
                "MEASURE  (M to exit)",
                egui::FontId::monospace(12.0),
                egui::Color32::from_rgb(255, 220, 90),
            );
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

        // Only keep the render loop running while the user is actively
        // dragging (orbit / pan / measure / zoom-box), or for the one frame
        // after a fresh load (`force`): the mesh upload + camera auto-fit run
        // *after* `mvp` is computed this frame, so the fitted view only shows
        // next frame — same "repaint while a view reset is pending" guard the
        // 2D view uses. When idle the scene is static and the camera has no
        // easing, so egui's input-driven repaint (clicks, scroll, hover) is
        // enough — an unconditional repaint here just renders the whole scene
        // at max FPS forever and spins the GPU fans for nothing.
        if response.dragged() || force {
            ui.ctx().request_repaint();
        }
    }
}

impl Board3dView {
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

impl Default for Board3dView {
    fn default() -> Self {
        Self::new()
    }
}

// ────────────────────────────────────────────────────────────────────────
// Mesh helpers
// ────────────────────────────────────────────────────────────────────────

/// FR-4 slab caps: the top face (+Z) and bottom face (−Z), reusing the
/// outline's centered triangle soup at each Z. No backface culling, so the
/// shared winding is fine for both faces.
fn build_board_cap_vertices(outline: &OutlineData, rgb: [f32; 3], z_top: f32, z_bottom: f32) -> Vec<f32> {
    let [r, g, b] = rgb;
    let mut out = Vec::with_capacity(outline.mesh_indices.len() * 2 * 6);
    for &idx in &outline.mesh_indices {
        let v = outline.mesh_vertices_2d[idx as usize];
        out.extend_from_slice(&[v[0], v[1], z_top, r, g, b]);
    }
    for &idx in &outline.mesh_indices {
        let v = outline.mesh_vertices_2d[idx as usize];
        out.extend_from_slice(&[v[0], v[1], z_bottom, r, g, b]);
    }
    out
}

/// FR-4 substrate side walls (the board edge), as a quad (two triangles) per
/// outline edge spanning `z_top`→`z_bottom`. `outline.contours` are in the
/// gerber's original coordinate space, so center them by the bbox to match
/// the caps and the other (already-centered) layers.
fn build_board_wall_vertices(outline: &OutlineData, rgb: [f32; 3], z_top: f32, z_bottom: f32) -> Vec<f32> {
    let [r, g, b] = rgb;
    let cx = ((outline.bbox.min.x + outline.bbox.max.x) * 0.5) as f32;
    let cy = ((outline.bbox.min.y + outline.bbox.max.y) * 0.5) as f32;
    let mut out = Vec::new();
    for contour in &outline.contours {
        let n = contour.len();
        if n < 3 {
            continue;
        }
        for i in 0..n {
            let p0 = contour[i];
            let p1 = contour[(i + 1) % n];
            let (a0x, a0y) = (p0.x - cx, p0.y - cy);
            let (a1x, a1y) = (p1.x - cx, p1.y - cy);
            out.extend_from_slice(&[a0x, a0y, z_top, r, g, b]);
            out.extend_from_slice(&[a1x, a1y, z_top, r, g, b]);
            out.extend_from_slice(&[a1x, a1y, z_bottom, r, g, b]);
            out.extend_from_slice(&[a0x, a0y, z_top, r, g, b]);
            out.extend_from_slice(&[a1x, a1y, z_bottom, r, g, b]);
            out.extend_from_slice(&[a0x, a0y, z_bottom, r, g, b]);
        }
    }
    out
}

/// Same shape as `build_board_vertices` but reads from a `CopperData` at a
/// caller-chosen Z. Kept separate so the call site reads clearly as
/// "copper mesh, not board mesh."
fn build_copper_vertices(cu: &CopperData, rgb: [f32; 3], z: f32) -> Vec<f32> {
    let [r, g, b] = rgb;
    let mut out = Vec::with_capacity(cu.mesh_indices.len() * 6);
    for &idx in &cu.mesh_indices {
        let v = cu.mesh_vertices_2d[idx as usize];
        out.extend_from_slice(&[v[0], v[1], z, r, g, b]);
    }
    out
}

/// Depth of an inner copper layer, interpolated evenly between the F.Cu and
/// B.Cu outer faces by its 1-based stack index. `Copper(1)` = top,
/// `Copper(copper_count)` = bottom; inner layers land at the fractions
/// between. Even spacing is an approximation (real stackups vary) but reads
/// correctly for registration / cross-section inspection.
fn inner_layer_z(stack_index: u8, copper_count: u8) -> f32 {
    if copper_count <= 1 {
        return COPPER_Z_TOP;
    }
    let t = (stack_index as f32 - 1.0) / (copper_count as f32 - 1.0);
    COPPER_Z_TOP + (COPPER_Z_BOTTOM - COPPER_Z_TOP) * t
}

fn build_mask_vertices(m: &MaskData, rgb: [f32; 3], z: f32) -> Vec<f32> {
    let [r, g, b] = rgb;
    let mut out = Vec::with_capacity(m.mesh_indices.len() * 6);
    for &idx in &m.mesh_indices {
        let v = m.mesh_vertices_2d[idx as usize];
        out.extend_from_slice(&[v[0], v[1], z, r, g, b]);
    }
    out
}

/// Triangle-fan disks for each drilled hole, at a fixed Z. Each hole becomes
/// `HOLE_SEGMENTS` triangles (center + ring), `xyz rgb` per vertex — the same
/// stride every other mesh here uses.
fn build_holes_vertices(drill: &DrillData, rgb: [f32; 3], z: f32) -> Vec<f32> {
    let [r, g, b] = rgb;
    let seg = HOLE_SEGMENTS;
    let mut out = Vec::with_capacity(drill.holes.len() * seg * 3 * 6);
    for hole in &drill.holes {
        let [cx, cy] = hole.center;
        let rad = hole.radius;
        for i in 0..seg {
            let a0 = (i as f32 / seg as f32) * std::f32::consts::TAU;
            let a1 = ((i + 1) as f32 / seg as f32) * std::f32::consts::TAU;
            // center, then two ring points → one triangle of the fan.
            out.extend_from_slice(&[cx, cy, z, r, g, b]);
            out.extend_from_slice(&[cx + rad * a0.cos(), cy + rad * a0.sin(), z, r, g, b]);
            out.extend_from_slice(&[cx + rad * a1.cos(), cy + rad * a1.sin(), z, r, g, b]);
        }
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
