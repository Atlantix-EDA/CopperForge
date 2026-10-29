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
    grid::grid_vertices, project, unproject_to_z0, Camera, ColoredMesh,
    UnlitProgram,
};
#[cfg(not(target_arch = "wasm32"))]
use crate::step_model::{load_step_component, StepComponentMesh};
use nalgebra::Vector3;

/// FR-4 green for the flat board mesh. Matches the conventional soldermask
/// tone so a board rendered without mask+copper still reads as a PCB.
const FR4_COLOR: [f32; 3] = [0.10, 0.34, 0.19];
/// ENIG gold (#D8AD4F) for F.Cu. Matches what a HASL/ENIG finished board
/// reads as under a green mask.
const COPPER_COLOR_TOP: [f32; 3] = [0.85, 0.68, 0.31];
/// B.Cu matches F.Cu — a fabbed board's two sides are the same finish,
/// and the side-distinct debug tint read as "wrong color" next to the
/// JLCPCB viewer (2026-10-29). Orientation cues come from geometry/silk.
const COPPER_COLOR_BOTTOM: [f32; 3] = [0.85, 0.68, 0.31];
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
/// The board is a solid: caps AND side walls opaque, so buried copper can
/// never bleed through a face (the translucent-wall x-ray look read as a
/// rendering bug next to real-board viewers, 2026-10-29). True edge
/// cross-sections of inner copper would need CSG against the slab —
/// out of scope for this decal renderer.

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
const MASK_COLOR_TOP: [f32; 3] = [0.06, 0.38, 0.21];
const MASK_COLOR_BOTTOM: [f32; 3] = [0.06, 0.38, 0.21];
/// Mask outer face = copper outer face + mask film (conforms over copper).
const MASK_Z_TOP: f32 = COPPER_Z_TOP + MASK_THICKNESS;
const MASK_Z_BOTTOM: f32 = COPPER_Z_BOTTOM - MASK_THICKNESS;
/// Mask blend alpha. At 0.55 the copper's yellow bled through and turned
/// every under-mask region olive ("canned spinach"); 0.72 still read
/// yellow/washed side-by-side with JLCPCB's viewer (2026-10-29, Interface
/// rev_03b gerbers). 0.85 + the cooler mask/FR-4 greens above match JLC's
/// deep saturated tone; under-mask traces stay visible as a ~15% lighter
/// green.
const MASK_ALPHA: f32 = 0.85;

/// Off-white silkscreen legend. Top reads a hair brighter than bottom so the
/// two are distinguishable when both are edge-on in the same view.
const SILK_COLOR_TOP: [f32; 3] = [0.92, 0.92, 0.89];
const SILK_COLOR_BOTTOM: [f32; 3] = [0.92, 0.92, 0.89];
/// Silk ink printed on top of the mask: mask outer face + ink thickness.
const SILK_Z_TOP: f32 = MASK_Z_TOP + SILK_THICKNESS;
const SILK_Z_BOTTOM: f32 = MASK_Z_BOTTOM - SILK_THICKNESS;

/// Exposed copper (mask openings ∩ copper, bores punched) sits a hair above
/// the mask sheet so each pad reads as solid exposed copper regardless of
/// how its opening overlaps neighbours.
const EXPOSED_Z_TOP: f32 = MASK_Z_TOP + 0.002;
const EXPOSED_Z_BOTTOM: f32 = MASK_Z_BOTTOM - 0.002;

const GRID_COLOR: [f32; 3] = [0.28, 0.30, 0.35];

/// Via/pin barrel wall — plated copper, dimmer than the pad gold so the
/// bore mouth reads as depth rather than as more pad.
const BARREL_COLOR_PLATED: [f32; 3] = [0.62, 0.45, 0.22];
/// Non-plated bore wall (mounting holes) — bare drilled FR-4.
const BARREL_COLOR_NPTH: [f32; 3] = [0.38, 0.34, 0.25];

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
    /// STEP model loading (native prototype): file picker, meshes staged
    /// between the ribbon (no gl) and the canvas (uploads them), loaded-model
    /// labels for the ribbon, and the last load error for the ⚠ tooltip.
    #[cfg(not(target_arch = "wasm32"))]
    step_dialog: egui_file_dialog::FileDialog,
    #[cfg(not(target_arch = "wasm32"))]
    pending_step_meshes: Vec<StepComponentMesh>,
    #[cfg(not(target_arch = "wasm32"))]
    loaded_step_models: Vec<(String, usize)>,
    #[cfg(not(target_arch = "wasm32"))]
    step_error: Option<String>,
    #[cfg(not(target_arch = "wasm32"))]
    clear_step_models: bool,
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
    /// Exposed-copper pads (mask openings as solid copper), one per side.
    top_exposed: ColoredMesh,
    top_exposed_ready: bool,
    bottom_exposed: ColoredMesh,
    bottom_exposed_ready: bool,
    /// Silkscreen legend meshes (off-white), one per side.
    top_silk: ColoredMesh,
    top_silk_ready: bool,
    bottom_silk: ColoredMesh,
    bottom_silk_ready: bool,
    /// Via / hole barrels: one cylinder wall per drilled hole, spanning the
    /// copper stack. Replaces the old flat dark disks — the hole mouths are
    /// now really punched out of the caps/copper/exposed sheets, and the
    /// barrel is what you see in (and through) the bore.
    barrels: ColoredMesh,
    barrels_ready: bool,
    /// User-loaded STEP component bodies (lambert-baked triangle soup).
    /// Always present in the struct so the paint callback stays cfg-free;
    /// only the native build ever pushes into it.
    components: Vec<ColoredMesh>,
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
            #[cfg(not(target_arch = "wasm32"))]
            step_dialog: egui_file_dialog::FileDialog::new(),
            #[cfg(not(target_arch = "wasm32"))]
            pending_step_meshes: Vec::new(),
            #[cfg(not(target_arch = "wasm32"))]
            loaded_step_models: Vec::new(),
            #[cfg(not(target_arch = "wasm32"))]
            step_error: None,
            #[cfg(not(target_arch = "wasm32"))]
            clear_step_models: false,
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

            // ── STEP component models (native prototype) ───────────
            #[cfg(not(target_arch = "wasm32"))]
            {
                ui.add_space(12.0);
                if ui
                    .button("+ STEP…")
                    .on_hover_text(
                        "Load a STEP component model onto the board (prototype: \
                         placed at board centre, resting on the top face).",
                    )
                    .clicked()
                {
                    self.step_dialog.pick_file();
                }
                if !self.loaded_step_models.is_empty() {
                    let total: usize = self.loaded_step_models.iter().map(|(_, t)| t).sum();
                    let label = format!(
                        "{} model{} ({total} tris)",
                        self.loaded_step_models.len(),
                        if self.loaded_step_models.len() == 1 { "" } else { "s" },
                    );
                    ui.label(label).on_hover_ui(|ui| {
                        for (name, tris) in &self.loaded_step_models {
                            ui.label(format!("{name} — {tris} tris"));
                        }
                    });
                    if ui.button("Clear").on_hover_text("Remove all loaded STEP models").clicked() {
                        self.clear_step_models = true;
                    }
                }
                if let Some(err) = self.step_error.clone() {
                    if ui
                        .colored_label(egui::Color32::YELLOW, "⚠")
                        .on_hover_text(&err)
                        .clicked()
                    {
                        self.step_error = None;
                    }
                }

                if let Some(path) = self.step_dialog.update(ui.ctx()).picked() {
                    let path = path.to_path_buf();
                    let name = path
                        .file_stem()
                        .map(|s| s.to_string_lossy().into_owned())
                        .unwrap_or_else(|| "model".into());
                    match std::fs::read_to_string(&path) {
                        Ok(text) => match load_step_component(&text, &name) {
                            Ok(mut mesh) => {
                                // Predictable prototype placement: bbox XY
                                // centre → board centre (the board mesh frame
                                // is origin-centred), bottom face → board
                                // top. Component STEPs carry arbitrary
                                // authoring origins, so honouring them here
                                // would scatter parts off-board.
                                let cx = (mesh.bbox_min[0] + mesh.bbox_max[0]) * 0.5;
                                let cy = (mesh.bbox_min[1] + mesh.bbox_max[1]) * 0.5;
                                let dz = BOARD_Z_TOP - mesh.bbox_min[2];
                                for v in mesh.verts.chunks_exact_mut(6) {
                                    v[0] -= cx;
                                    v[1] -= cy;
                                    v[2] += dz;
                                }
                                self.step_error = if mesh.warnings.is_empty() {
                                    None
                                } else {
                                    Some(format!(
                                        "{name}: {} reader warning(s):\n{}",
                                        mesh.warnings.len(),
                                        mesh.warnings.join("\n"),
                                    ))
                                };
                                self.pending_step_meshes.push(mesh);
                            }
                            Err(e) => self.step_error = Some(format!("{name}: {e}")),
                        },
                        Err(e) => self.step_error = Some(format!("{}: {e}", path.display())),
                    }
                }
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
                    axes.upload(gl, &board_axes_vertices(3.0));

                    let mut grid = ColoredMesh::new(gl, glow::LINES);
                    grid.upload(gl, &grid_vertices(5.0, 1.0, GRID_COLOR));

                    let board = ColoredMesh::new(gl, glow::TRIANGLES);
                    let board_walls = ColoredMesh::new(gl, glow::TRIANGLES);
                    let top_copper = ColoredMesh::new(gl, glow::TRIANGLES);
                    let bottom_copper = ColoredMesh::new(gl, glow::TRIANGLES);
                    let top_mask = ColoredMesh::new(gl, glow::TRIANGLES);
                    let bottom_mask = ColoredMesh::new(gl, glow::TRIANGLES);
                    let top_exposed = ColoredMesh::new(gl, glow::TRIANGLES);
                    let bottom_exposed = ColoredMesh::new(gl, glow::TRIANGLES);
                    let top_silk = ColoredMesh::new(gl, glow::TRIANGLES);
                    let bottom_silk = ColoredMesh::new(gl, glow::TRIANGLES);
                    let barrels = ColoredMesh::new(gl, glow::TRIANGLES);

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
                        top_exposed,
                        top_exposed_ready: false,
                        bottom_exposed,
                        bottom_exposed_ready: false,
                        top_silk,
                        top_silk_ready: false,
                        bottom_silk,
                        bottom_silk_ready: false,
                        barrels,
                        barrels_ready: false,
                        components: Vec::new(),
                    }
                };
                Arc::new(Mutex::new(resources))
            })
            .clone();

        // A new board (or first frame) forces every layer to re-upload,
        // re-evaluating present/absent so stale meshes from the previous
        // board don't linger. Cleared once all layers are processed.
        let force = self.dirty;

        // Drill holes are punched out of the caps / copper / exposed sheets,
        // so all of those rebuild when the drill layer appears/disappears.
        // Captured early — the barrels block below overwrites the flag.
        let drill_changed = drill.is_some() != self.last_had_drill;
        let drill_holes: &[crate::gerber_geom::Hole] =
            drill.map(|d| d.holes.as_slice()).unwrap_or(&[]);

        // Exposed copper = (mask ∩ copper) − holes, so it rebuilds when ANY
        // input appears/disappears. Capture the transitions here — the
        // copper and mask blocks below overwrite the `last_had_*` flags as
        // they run.
        let exposed_top_changed = top_mask.is_some() != self.last_had_top_mask
            || top_copper.is_some() != self.last_had_top_copper
            || drill_changed;
        let exposed_bottom_changed = bottom_mask.is_some() != self.last_had_bottom_mask
            || bottom_copper.is_some() != self.last_had_bottom_copper
            || drill_changed;

        // ── Board mesh (absent ↔ present transition, or forced) ────
        // Caps re-upload when the drill changes too (holes are punched out
        // of them); camera fit / axes / grid only react to the outline
        // itself, so assigning a drill file later doesn't yank the view.
        let has_outline = board_outline.is_some();
        if force || has_outline != self.last_had_outline || drill_changed {
            if let (Some(outline), Ok(mut g)) = (board_outline, gpu.lock()) {
                let caps = build_board_cap_vertices(
                    outline,
                    drill_holes,
                    FR4_COLOR,
                    BOARD_Z_TOP,
                    BOARD_Z_BOTTOM,
                );
                let walls = build_board_wall_vertices(outline, FR4_COLOR, BOARD_Z_TOP, BOARD_Z_BOTTOM);
                unsafe {
                    g.board.upload(gl, &caps);
                    g.board_walls.upload(gl, &walls);
                }
                g.board_ready = true;
            } else if let Ok(mut g) = gpu.lock() {
                g.board_ready = false;
            }
        }
        if force || has_outline != self.last_had_outline {
            if let Some(outline) = board_outline {
                let w = (outline.bbox.max.x - outline.bbox.min.x) as f32;
                let h = (outline.bbox.max.y - outline.bbox.min.y) as f32;
                self.last_board_dim = Some((w, h));
                self.camera.fit_to_bbox(w, h);
                // JLC-viewer style: the axes run THROUGH the board (meshes
                // are origin-centered), extending symmetrically past the
                // edges. axes_len is the half-span per direction.
                self.axes_len = (w.max(h) * 0.75).max(3.0);
            } else {
                self.last_board_dim = None;
                self.axes_len = 3.0;
            }
            if let Ok(mut g) = gpu.lock() {
                unsafe {
                    g.axes.upload(gl, &board_axes_vertices(self.axes_len));
                }
            }
            self.last_had_outline = has_outline;
            self.last_uploaded_grid_step_mm = None;
        }

        // ── Copper meshes (F.Cu / B.Cu), holes punched out ────────
        let has_top_copper = top_copper.is_some();
        if force || has_top_copper != self.last_had_top_copper || drill_changed {
            if let (Some(cu), Ok(mut g)) = (top_copper, gpu.lock()) {
                let verts = build_punched_copper_vertices(
                    cu,
                    drill_holes,
                    board_outline,
                    COPPER_COLOR_TOP,
                    COPPER_Z_TOP,
                );
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
        if force || has_bottom_copper != self.last_had_bottom_copper || drill_changed {
            if let (Some(cu), Ok(mut g)) = (bottom_copper, gpu.lock()) {
                let verts = build_punched_copper_vertices(
                    cu,
                    drill_holes,
                    board_outline,
                    COPPER_COLOR_BOTTOM,
                    COPPER_Z_BOTTOM,
                );
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
        if force || inner_copper.len() != self.last_inner_copper_count || drill_changed {
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
                    let verts = build_punched_copper_vertices(
                        cu,
                        drill_holes,
                        board_outline,
                        INNER_COPPER_COLOR,
                        z,
                    );
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

        // ── Exposed copper ((mask openings ∩ copper) − holes) ─────
        // A real boolean, not the openings verbatim: copper colour appears
        // exactly where an opening overlaps metal, minus the drilled bores.
        // Openings over bare laminate (guard-band reliefs, NPTH windows,
        // mask expansion) stay laminate. Needs the outline for the shared
        // world frame.
        if force || exposed_top_changed {
            if let (Some(m), Some(cu), Some(outline), Ok(mut g)) =
                (top_mask, top_copper, board_outline, gpu.lock())
            {
                let exposed = build_exposed_copper_vertices(
                    m,
                    cu,
                    drill_holes,
                    outline,
                    COPPER_COLOR_TOP,
                    EXPOSED_Z_TOP,
                );
                unsafe {
                    g.top_exposed.upload(gl, &exposed);
                }
                g.top_exposed_ready = !exposed.is_empty();
            } else if let Ok(mut g) = gpu.lock() {
                g.top_exposed_ready = false;
            }
        }
        if force || exposed_bottom_changed {
            if let (Some(m), Some(cu), Some(outline), Ok(mut g)) =
                (bottom_mask, bottom_copper, board_outline, gpu.lock())
            {
                let exposed = build_exposed_copper_vertices(
                    m,
                    cu,
                    drill_holes,
                    outline,
                    COPPER_COLOR_BOTTOM,
                    EXPOSED_Z_BOTTOM,
                );
                unsafe {
                    g.bottom_exposed.upload(gl, &exposed);
                }
                g.bottom_exposed_ready = !exposed.is_empty();
            } else if let Ok(mut g) = gpu.lock() {
                g.bottom_exposed_ready = false;
            }
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

        // ── Via / hole barrels ──────────────────────────────────────
        // One cylinder wall per hole spanning the copper stack. The sheets
        // above have the hole mouths genuinely punched out, so the barrel IS
        // the hole's appearance: plated copper for PTH, bare FR-4 for NPTH,
        // with an azimuthal light bake so the wall reads as curved depth
        // rather than a flat ring.
        let has_drill = drill.is_some();
        if force || drill_changed {
            if let (Some(d), Ok(mut g)) = (drill, gpu.lock()) {
                let verts = build_barrel_vertices(d);
                unsafe {
                    g.barrels.upload(gl, &verts);
                }
                g.barrels_ready = true;
            } else if let Ok(mut g) = gpu.lock() {
                g.barrels_ready = false;
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

        // ── STEP component meshes (native prototype) ──────────────
        #[cfg(not(target_arch = "wasm32"))]
        {
            if self.clear_step_models {
                if let Ok(mut g) = gpu.lock() {
                    // NOTE: ColoredMesh has no Drop — the GL buffers leak on
                    // clear. Acceptable for the prototype; a Drop impl (or
                    // explicit delete) comes with the wgpu port.
                    g.components.clear();
                }
                self.loaded_step_models.clear();
                self.clear_step_models = false;
            }
            if !self.pending_step_meshes.is_empty() {
                if let Ok(mut g) = gpu.lock() {
                    for staged in self.pending_step_meshes.drain(..) {
                        let mut mesh = unsafe { ColoredMesh::new(gl, glow::TRIANGLES) };
                        unsafe {
                            mesh.upload(gl, &staged.verts);
                        }
                        g.components.push(mesh);
                        self.loaded_step_models
                            .push((staged.name.clone(), staged.triangle_count()));
                    }
                }
            }
        }

        // Which side of the board plane the eye is on — drives back-to-front
        // ordering of the translucent pass (masks + board edge walls). The
        // eye sits `zoom` along camera -Z from `target`, so map that offset
        // back into world space.
        let eye_world =
            self.camera.target + self.camera.rotation.inverse() * Vector3::new(0.0, 0.0, self.camera.zoom);
        let from_top = eye_world.z >= 0.0;

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
                // The layer sheets sit 2–35 µm apart — real film thicknesses,
                // far below what the depth buffer resolves at normal orbit
                // distances. Polygon offset pulls each successive decal a
                // fixed number of depth-resolution steps toward the viewer,
                // making the stack order deterministic regardless of zoom:
                // board 0 → copper 2 → mask 4 → exposed 6 → silk 10 (units).
                gl.enable(glow::POLYGON_OFFSET_FILL);
                // Opaque pass first (depth-write on): B.Cu, board (FR-4),
                // F.Cu. Deepest-to-shallowest so the depth buffer settles
                // correctly before the blended pass reads from it.
                //
                // Polygon offsets are units-only (factor 0): a slope factor
                // scales with the polygon's screen-space depth gradient and
                // explodes at grazing angles, pulling buried geometry
                // through the opaque caps (the "inner layer bleeds through
                // the top view" bug). Constant units keep the stack order
                // deterministic at every orbit angle.
                g.unlit.set_alpha(gl, 1.0);
                gl.polygon_offset(0.0, -2.0);
                if g.bottom_copper_ready {
                    g.bottom_copper.draw(gl);
                }
                // Inner copper is not drawn: the board renders as a solid —
                // opaque caps and walls — so buried sheets are never
                // visible. (Meshes are still baked; a future x-ray/section
                // mode can draw them.)
                gl.polygon_offset(0.0, 0.0);
                if g.board_ready {
                    g.board.draw(gl);
                    // Opaque edge walls, depth-written like the caps, so
                    // the slab occludes everything inside it.
                    g.board_walls.draw(gl);
                }
                gl.polygon_offset(0.0, -2.0);
                if g.top_copper_ready {
                    g.top_copper.draw(gl);
                }
                gl.polygon_offset(0.0, 0.0);
                // Via / hole barrels: real cylinders through the stack,
                // visible in the punched hole mouths and through the
                // translucent board edge. Plain depth-tested geometry.
                if g.barrels_ready {
                    g.barrels.draw(gl);
                }
                // STEP component bodies: opaque, real thickness, sitting on
                // the board top — plain depth-tested geometry, no offset.
                for m in &g.components {
                    m.draw(gl);
                }
                // Translucent pass for the solder-mask sheets. Alpha-blend
                // against the settled opaque pass; depth-write off so the
                // sheets don't occlude each other. Far-side mask first,
                // near-side last (walls are opaque now and already drawn).
                gl.enable(glow::BLEND);
                gl.blend_func(glow::SRC_ALPHA, glow::ONE_MINUS_SRC_ALPHA);
                gl.depth_mask(false);
                gl.polygon_offset(0.0, -4.0);
                g.unlit.set_alpha(gl, MASK_ALPHA);
                if from_top {
                    if g.bottom_mask_ready {
                        g.bottom_mask.draw(gl);
                    }
                    if g.top_mask_ready {
                        g.top_mask.draw(gl);
                    }
                } else {
                    if g.top_mask_ready {
                        g.top_mask.draw(gl);
                    }
                    if g.bottom_mask_ready {
                        g.bottom_mask.draw(gl);
                    }
                }
                g.unlit.set_alpha(gl, 1.0);
                gl.depth_mask(true);
                gl.disable(glow::BLEND);
                // Exposed copper pads: solid copper in the mask openings,
                // opaque on top of the mask sheet, so each pad reads as fully
                // exposed even where openings overlap. Drawn before the drill
                // holes so the dark bore caps punch back through.
                gl.polygon_offset(0.0, -6.0);
                if g.bottom_exposed_ready {
                    g.bottom_exposed.draw(gl);
                }
                if g.top_exposed_ready {
                    g.top_exposed.draw(gl);
                }
                // Silkscreen legend: opaque off-white, drawn last among the
                // board layers (depth-write still on) at a Z just above the
                // mask so it reads as printed on top of the soldermask.
                gl.polygon_offset(0.0, -10.0);
                if g.top_silk_ready {
                    g.top_silk.draw(gl);
                }
                if g.bottom_silk_ready {
                    g.bottom_silk.draw(gl);
                }
                gl.polygon_offset(0.0, 0.0);
                gl.disable(glow::POLYGON_OFFSET_FILL);
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
        let z_mid = (BOARD_Z_TOP + BOARD_Z_BOTTOM) * 0.5;
        let label_offset_px = 14.0_f32;
        let axes = [
            (Vector3::new(l, 0.0, z_mid), "X", egui::Color32::from_rgb(255, 90, 90)),
            (Vector3::new(0.0, l, z_mid), "Y", egui::Color32::from_rgb(90, 220, 90)),
            (Vector3::new(0.0, 0.0, z_mid + l * 0.6), "Z", egui::Color32::from_rgb(110, 150, 255)),
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
                ui.painter().line_segment([ps, pe], egui::Stroke::new(2.0_f32, color));
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
                egui::Stroke::new(1.5_f32, egui::Color32::from_rgb(255, 200, 60)),
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

/// FR-4 slab caps: the top face (+Z) and bottom face (−Z), tessellated from
/// the outline contours with every drill hole punched out — the bores are
/// real voids, and the barrel walls are what you see inside them. Falls back
/// to the outline's pre-tessellated soup when there are no holes to punch.
/// No backface culling, so the shared winding is fine for both faces.
/// Full-span XYZ axes through the board body, JLC-viewer style: thin lines
/// centered on the (origin-centered) board, running symmetrically ±half in
/// each direction. X/Y lie in the slab's mid-plane so the solid board
/// occludes the buried segment and the lines emerge at the edges; Z pierces
/// the board vertically through its center.
fn board_axes_vertices(half: f32) -> Vec<f32> {
    let h = half;
    let z_mid = (BOARD_Z_TOP + BOARD_Z_BOTTOM) * 0.5;
    let z_half = h * 0.6;
    vec![
        // X axis: red
        -h, 0.0, z_mid, 1.0, 0.25, 0.25,
        h, 0.0, z_mid, 1.0, 0.25, 0.25,
        // Y axis: green
        0.0, -h, z_mid, 0.25, 0.85, 0.25,
        0.0, h, z_mid, 0.25, 0.85, 0.25,
        // Z axis: blue
        0.0, 0.0, z_mid - z_half, 0.35, 0.5, 1.0,
        0.0, 0.0, z_mid + z_half, 0.35, 0.5, 1.0,
    ]
}

fn build_board_cap_vertices(
    outline: &OutlineData,
    holes: &[crate::gerber_geom::Hole],
    rgb: [f32; 3],
    z_top: f32,
    z_bottom: f32,
) -> Vec<f32> {
    let [r, g, b] = rgb;
    let (punched_v, punched_i);
    let (verts_2d, indices): (&[[f32; 2]], &[u32]) = if holes.is_empty() {
        (&outline.mesh_vertices_2d, &outline.mesh_indices)
    } else {
        (punched_v, punched_i) = crate::gerber_geom::punched_outline_mesh(
            &outline.contours,
            holes,
            &outline.bbox,
        );
        (&punched_v, &punched_i)
    };
    let mut out = Vec::with_capacity(indices.len() * 2 * 6);
    for &idx in indices {
        let v = verts_2d[idx as usize];
        out.extend_from_slice(&[v[0], v[1], z_top, r, g, b]);
    }
    for &idx in indices {
        let v = verts_2d[idx as usize];
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

/// Copper sheet with the drill holes punched out (a pad flash is a FULL
/// disc in the gerber — the bore only exists in the drill files). Without
/// holes, or without the outline needed for the shared punch frame, falls
/// back to the plain pre-tessellated copper mesh.
fn build_punched_copper_vertices(
    cu: &CopperData,
    holes: &[crate::gerber_geom::Hole],
    outline: Option<&OutlineData>,
    rgb: [f32; 3],
    z: f32,
) -> Vec<f32> {
    let Some(outline) = outline else {
        return build_copper_vertices(cu, rgb, z);
    };
    if holes.is_empty() {
        return build_copper_vertices(cu, rgb, z);
    }
    let (verts_2d, indices) =
        crate::gerber_geom::punched_copper_mesh(&cu.contours, holes, &outline.bbox);
    if indices.is_empty() {
        return build_copper_vertices(cu, rgb, z);
    }
    let [r, g, b] = rgb;
    let mut out = Vec::with_capacity(indices.len() * 6);
    for &idx in &indices {
        let v = verts_2d[idx as usize];
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

/// Exposed-copper sheet from the (mask openings ∩ copper) − holes boolean
/// (`gerber_geom::exposed_copper_mesh`), lifted to a flat sheet at `z`.
/// Empty when no opening overlaps copper.
fn build_exposed_copper_vertices(
    m: &MaskData,
    cu: &CopperData,
    holes: &[crate::gerber_geom::Hole],
    outline: &OutlineData,
    rgb: [f32; 3],
    z: f32,
) -> Vec<f32> {
    let (verts_2d, indices) = crate::gerber_geom::exposed_copper_mesh(
        &m.opening_contours,
        &cu.contours,
        holes,
        &outline.bbox,
    );
    let [r, g, b] = rgb;
    let mut out = Vec::with_capacity(indices.len() * 6);
    for &idx in &indices {
        let v = verts_2d[idx as usize];
        out.extend_from_slice(&[v[0], v[1], z, r, g, b]);
    }
    out
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

/// Via / hole barrels: a cylinder wall per hole, spanning the copper stack
/// (F.Cu outer face → B.Cu outer face). Segment count is chord-error
/// adaptive per hole and shared with the punch discs
/// (`gerber_geom::exposed::hole_segments`), so the wall meets the punched
/// sheet rims vertex-for-vertex and bores stay round at pad-level zoom.
///
/// The renderer is unlit, so an azimuthal light term is baked into the
/// vertex colours (same key direction as the STEP-model bake): the wall
/// facing the key light reads bright, the far side dark. With no backface
/// culling, looking into a bore shows the inner far wall — the gradient is
/// what makes it read as a cylindrical hole instead of a flat ring.
fn build_barrel_vertices(drill: &DrillData) -> Vec<f32> {
    // Key light azimuth (matches step_model's KEY_LIGHT xy, normalized).
    let (kx, ky) = {
        let (x, y) = (-0.35_f32, 0.25_f32);
        let len = (x * x + y * y).sqrt();
        (x / len, y / len)
    };
    let mut out = Vec::with_capacity(drill.holes.len() * 32 * 6 * 6);
    for hole in &drill.holes {
        let seg = crate::gerber_geom::exposed::hole_segments(hole.radius);
        // Ring geometry + per-ring-point shade, shared by both Z rims.
        let ring: Vec<(f32, f32, f32)> = (0..seg)
            .map(|i| {
                let a = (i as f32 / seg as f32) * std::f32::consts::TAU;
                let (nx, ny) = (a.cos(), a.sin());
                let k = (0.45 + 0.45 * (nx * kx + ny * ky).max(0.0)).min(1.0);
                (nx, ny, k)
            })
            .collect();
        let base = if hole.plated { BARREL_COLOR_PLATED } else { BARREL_COLOR_NPTH };
        let [cx, cy] = hole.center;
        let rad = hole.radius;
        for i in 0..seg {
            let (n0x, n0y, k0) = ring[i];
            let (n1x, n1y, k1) = ring[(i + 1) % seg];
            let (x0, y0) = (cx + rad * n0x, cy + rad * n0y);
            let (x1, y1) = (cx + rad * n1x, cy + rad * n1y);
            let c0 = [base[0] * k0, base[1] * k0, base[2] * k0];
            let c1 = [base[0] * k1, base[1] * k1, base[2] * k1];
            // Two triangles per wall quad, vertex colour per ring point.
            out.extend_from_slice(&[x0, y0, COPPER_Z_TOP, c0[0], c0[1], c0[2]]);
            out.extend_from_slice(&[x1, y1, COPPER_Z_TOP, c1[0], c1[1], c1[2]]);
            out.extend_from_slice(&[x1, y1, COPPER_Z_BOTTOM, c1[0], c1[1], c1[2]]);
            out.extend_from_slice(&[x0, y0, COPPER_Z_TOP, c0[0], c0[1], c0[2]]);
            out.extend_from_slice(&[x1, y1, COPPER_Z_BOTTOM, c1[0], c1[1], c1[2]]);
            out.extend_from_slice(&[x0, y0, COPPER_Z_BOTTOM, c0[0], c0[1], c0[2]]);
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
