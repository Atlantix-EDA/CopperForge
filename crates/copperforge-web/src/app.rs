//! Browser-side `WebApp` — upload, parse, render.
//!
//! Top bar: brand + Upload button + loaded-file status.
//! Left side panel: per-layer visibility checkboxes (only once loaded).
//! Central panel: the actual gerber canvas.
//!
//! Compiled only for wasm32; native main is a one-shot help message in
//! `main.rs`.

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

use copperforge_core::display::{draw_grid, GridSettings};
use gerber_viewer::{GerberTransform, Mirroring, ViewState};
use nalgebra::{Point2, Vector2};

use std::collections::HashMap;

use crate::board_weight::{self, WeightInputs};
use crate::bom::{self, Mount};
use crate::canvas::model::LayerSide;
use crate::canvas::{paint as paint_canvas, GerberScene};
use crate::centroid::{self, CentroidEntry};
use crate::pad_count::{self, SmtPadCount};
use crate::release_pkg;

// ── Upload result types ─────────────────────────────────────────────────

/// One decompressed `.gbr` / `.drl` entry from the uploaded ZIP.
///
/// `BTreeMap` keeps insertion order stable (alphabetical by basename).
/// Subdirectories inside the archive are flattened to basenames.
#[derive(Debug, Clone)]
pub struct LoadedRelease {
    /// The source zip's filename — for the "Loaded …" status line.
    pub source_name: String,
    /// `<basename> → bytes`. Last-write-wins on collisions; release
    /// zips don't have any in practice.
    pub entries: BTreeMap<String, Vec<u8>>,
}

impl LoadedRelease {
    pub fn gerber_count(&self) -> usize {
        self.entries
            .keys()
            .filter(|n| n.to_lowercase().ends_with(".gbr"))
            .count()
    }
    pub fn drill_count(&self) -> usize {
        self.entries
            .keys()
            .filter(|n| n.to_lowercase().ends_with(".drl"))
            .count()
    }
    pub fn total_bytes(&self) -> usize {
        self.entries.values().map(|v| v.len()).sum()
    }
}

/// Result slot shared between the async file-load task and the egui
/// update loop. `wasm32` is single-threaded so the Mutex is never
/// actually contended; the bound exists for the future's `Send`.
type LoadSlot = Arc<Mutex<Option<Result<LoadedRelease, String>>>>;

// ── App state ───────────────────────────────────────────────────────────

pub struct WebApp {
    pending_load: LoadSlot,
    loading: bool,
    loaded: Option<LoadedRelease>,
    error: Option<String>,

    /// Parsed scene, rebuilt every time a new `LoadedRelease` arrives.
    scene: Option<GerberScene>,
    /// Pan/zoom state. `fit_view` is called once after each new scene
    /// (`view_initialized = false` triggers it on the next frame).
    view_state: ViewState,
    view_initialized: bool,

    /// Grid overlay. Same struct + draw fn the native viewer uses.
    grid_settings: GridSettings,

    /// Ruler tool — two click points and a distance label. Local
    /// implementation rather than reaching into copperforge-core's
    /// app-services-coupled state.
    ruler_active: bool,
    ruler_start: Option<Point2<f64>>,
    ruler_end: Option<Point2<f64>>,

    /// Zoom-to-region: `Some(start_screen_pos)` while the left button
    /// is held mid-drag. On release with > 10px drag, `fit_view` is
    /// called against the gerber-space bbox of the rubber-band rect.
    /// Clicks (no significant drag) pass through to the ruler tool.
    zoom_rect_start: Option<egui::Pos2>,

    /// Image-level mirror toggles, applied uniformly to every layer
    /// during paint (pivot = scene bbox center, same as the native
    /// viewer's display_manager.mirroring).
    mirror_x: bool,
    mirror_y: bool,

    /// Stats / board panel: show dimensions in mil when true, mm when
    /// false.
    units_mils: bool,

    /// About dialog visibility (opened by clicking the brand label).
    about_open: bool,

    /// Parsed CPL/centroid rows from the uploaded zip (if present).
    /// Drives the component-count rows in the Board stats panel and the
    /// PCBWAY_FAB_SPECS.md table.
    centroid: Vec<CentroidEntry>,
    /// `designator → Mount` from parsing the BOM CSV in the upload.
    /// Joined against `centroid` to derive SMT / THT totals.
    bom_mount: HashMap<String, Mount>,
    /// SMT pad count per side, parsed from F.Paste / B.Paste flashes.
    /// Independent of the BOM — works on any release zip with paste
    /// layers present.
    smt_pads: SmtPadCount,

    /// Bare-board weight inputs (thickness, copper oz, fill %).
    /// Knob-driven approximation — gerber_viewer doesn't expose the
    /// primitives needed for an exact polygon-area tessellation, so
    /// we follow the standard PCB-calculator method of `bbox × fill %`.
    weight_inputs: WeightInputs,

    /// Manual board origin in gerber coords (mm). `(0, 0)` keeps the
    /// raw gerber coordinate frame; any other value shifts the cursor
    /// readout so distances are reported relative to that point.
    /// Matches KiCad's "Set Drill/Place Origin" feature.
    design_offset: Point2<f64>,
    /// True between toggling "📍 Origin" on and the next left-click
    /// (or Escape). One-shot mode — click, set, exit.
    setting_origin: bool,
    /// Last-known gerber-coord position under the cursor (mm). Painted
    /// in the bottom status bar; updated each frame the canvas is
    /// hovered.
    cursor_world: Option<Point2<f64>>,
}

impl Default for WebApp {
    fn default() -> Self {
        Self {
            pending_load: LoadSlot::default(),
            loading: false,
            loaded: None,
            error: None,
            scene: None,
            view_state: ViewState::default(),
            view_initialized: false,
            grid_settings: GridSettings::default(),
            ruler_active: false,
            ruler_start: None,
            ruler_end: None,
            zoom_rect_start: None,
            mirror_x: false,
            mirror_y: false,
            units_mils: false,
            about_open: false,
            centroid: Vec::new(),
            bom_mount: HashMap::new(),
            smt_pads: SmtPadCount::default(),
            weight_inputs: WeightInputs::default(),
            design_offset: Point2::new(0.0, 0.0),
            setting_origin: false,
            cursor_world: None,
        }
    }
}

/// Component-count rollup derived by joining the centroid (designator →
/// side) with the BOM (designator → mount). `total` always reflects
/// centroid rows; `smt + tht + unknown_mount` equals `total` when both
/// CSVs are present and refdes lists line up. If the BOM is missing,
/// `smt = tht = 0` and `unknown_mount = total`.
#[derive(Debug, Clone, Default)]
struct ComponentStats {
    total: usize,
    top: usize,
    bottom: usize,
    smt: usize,
    tht: usize,
    unknown_mount: usize,
}

impl ComponentStats {
    fn build(
        centroid: &[CentroidEntry],
        bom_mount: &HashMap<String, Mount>,
    ) -> Self {
        let mut s = Self::default();
        for c in centroid {
            s.total += 1;
            match c.side {
                crate::centroid::Side::Top => s.top += 1,
                crate::centroid::Side::Bottom => s.bottom += 1,
            }
            match bom_mount.get(&c.designator).copied().unwrap_or(Mount::Unknown) {
                Mount::Smt => s.smt += 1,
                Mount::Tht => s.tht += 1,
                Mount::Unknown => s.unknown_mount += 1,
            }
        }
        s
    }
}

impl WebApp {
    fn pick_release_zip(&mut self, ctx: &egui::Context) {
        if self.loading {
            return;
        }
        self.loading = true;
        self.error = None;
        let slot = self.pending_load.clone();
        let ctx = ctx.clone();
        wasm_bindgen_futures::spawn_local(async move {
            let result = run_upload().await;
            *slot.lock().unwrap() =
                Some(result.unwrap_or_else(|| Err("canceled".to_string())));
            ctx.request_repaint();
        });
    }

    fn drain_pending(&mut self) {
        let Ok(mut guard) = self.pending_load.lock() else {
            return;
        };
        let Some(result) = guard.take() else { return };
        self.loading = false;
        match result {
            Ok(loaded) => {
                let scene = GerberScene::from_entries(&loaded.entries);
                let total_layers = scene.layers.len();
                // Centroid: designator → side.  BOM: designator → mount.
                // Both feed the joined ComponentStats in the panel and
                // the PCBWay fab-specs sheet.
                let centroid = centroid::find_and_parse(&loaded.entries)
                    .unwrap_or_default();
                let bom_mount = bom::find_and_parse(&loaded.entries);
                let smt_pads = pad_count::count_from_entries(&loaded.entries);
                if !centroid.is_empty() {
                    log::info!(
                        "Parsed {} centroid entries from release zip",
                        centroid.len()
                    );
                }
                if !bom_mount.is_empty() {
                    log::info!(
                        "Parsed {} BOM mount-type entries from release zip",
                        bom_mount.len()
                    );
                }
                self.scene = Some(scene);
                self.view_initialized = false;
                self.centroid = centroid;
                self.bom_mount = bom_mount;
                self.smt_pads = smt_pads;
                self.loaded = Some(loaded);
                self.error = None;
                log::info!("Parsed {} gerber layers", total_layers);
            }
            Err(e) if e == "canceled" => {}
            Err(e) => self.error = Some(e),
        }
    }

    /// Right-button drag pans, mouse wheel zooms anchored at the
    /// cursor, left-click places ruler points when ruler mode is on.
    /// Mirrors the native viewer's KiCad convention.
    fn handle_canvas_input(&mut self, ui: &egui::Ui, response: &egui::Response) {
        // ── Right-drag pans ─────────────────────────────────────────
        if response.dragged_by(egui::PointerButton::Secondary) {
            self.view_state.translation += response.drag_delta();
        }

        // ── Wheel zoom anchored at the cursor ───────────────────────
        // Mirrors copperforge-core's handle_mouse_wheel_zoom: read
        // raw_scroll_delta, multiply scale by 1.1 per notch, then shift
        // translation so the gerber point under the cursor stays under
        // the cursor. smooth_scroll_delta was wrong — it's zero unless
        // there's already an animation in flight.
        if response.hovered() {
            let scroll = ui.input(|i| i.raw_scroll_delta.y);
            if scroll != 0.0 {
                if let Some(mouse_pos) = response.hover_pos() {
                    let zoom_factor = if scroll > 0.0 { 1.1 } else { 1.0 / 1.1 };
                    let gerber_point =
                        self.view_state.screen_to_gerber_coords(mouse_pos);
                    let new_scale =
                        (self.view_state.scale * zoom_factor).clamp(0.01, 1000.0);
                    self.view_state.scale = new_scale;
                    let new_screen_pos =
                        self.view_state.gerber_to_screen_coords(gerber_point);
                    self.view_state.translation += mouse_pos - new_screen_pos;
                }
            }
        }

        // ── Zoom-to-region: left-drag rubber band ───────────────────
        // Press inside the canvas captures the start position. Release
        // with > 10px drag triggers fit_view against the gerber-space
        // bbox of the rubber-band rect. Tiny releases (clicks) drop
        // through to the ruler handler below — same threshold + dance
        // the desktop viewer uses.
        let primary_pressed =
            ui.input(|i| i.pointer.button_pressed(egui::PointerButton::Primary));
        let primary_released =
            ui.input(|i| i.pointer.button_released(egui::PointerButton::Primary));
        if primary_pressed && response.contains_pointer() {
            if let Some(pos) = response.hover_pos() {
                self.zoom_rect_start = Some(pos);
            }
        }
        if primary_released {
            if let Some(start) = self.zoom_rect_start.take() {
                if let Some(end) = ui.input(|i| i.pointer.interact_pos()) {
                    let rect = egui::Rect::from_two_pos(start, end);
                    if rect.width() > 10.0 && rect.height() > 10.0 {
                        let g1 = self.view_state.screen_to_gerber_coords(rect.min);
                        let g2 = self.view_state.screen_to_gerber_coords(rect.max);
                        let bbox = gerber_viewer::BoundingBox {
                            min: nalgebra::Point2::new(
                                g1.x.min(g2.x),
                                g1.y.min(g2.y),
                            ),
                            max: nalgebra::Point2::new(
                                g1.x.max(g2.x),
                                g1.y.max(g2.y),
                            ),
                        };
                        // Refit using the canvas rect from the response.
                        self.view_state.fit_view(response.rect, &bbox, 1.0);
                    }
                }
            }
        }

        // ── Double-click fit ────────────────────────────────────────
        // Same gesture as the desktop viewer: double-click anywhere on
        // the canvas to re-fit the whole board. Cheap to do via the
        // existing `view_initialized = false` path; the next frame
        // calls fit_view against the combined bbox.
        if response.double_clicked_by(egui::PointerButton::Primary) {
            self.view_initialized = false;
            // Cancel any in-flight zoom-rect — the double-click's
            // second press would otherwise leave a stale start point.
            self.zoom_rect_start = None;
        }

        // ── Origin placement: one-shot left-click while armed ───────
        // Checked BEFORE the ruler handler so a click while setting
        // origin doesn't also place a ruler point. The mode auto-
        // disarms after a successful click.
        if self.setting_origin && response.clicked_by(egui::PointerButton::Primary) {
            if let Some(pos) = response.interact_pointer_pos() {
                let g = self.view_state.screen_to_gerber_coords(pos);
                self.design_offset = g;
                self.setting_origin = false;
                self.zoom_rect_start = None;
                log::info!("Origin set to ({:.3}, {:.3}) mm", g.x, g.y);
            }
        }

        // ── Ruler tool: left-click places points ────────────────────
        // Only fires on real clicks (no drag), so zoom-to-region above
        // takes precedence whenever the user actually drags. Skipped
        // while setting_origin so the same click can't do both.
        if self.ruler_active
            && !self.setting_origin
            && response.clicked_by(egui::PointerButton::Primary)
        {
            if let Some(pos) = response.interact_pointer_pos() {
                let gerber_pos = self.view_state.screen_to_gerber_coords(pos);
                match (self.ruler_start, self.ruler_end) {
                    (None, _) => {
                        self.ruler_start = Some(gerber_pos);
                        self.ruler_end = None;
                    }
                    (Some(_), None) => {
                        self.ruler_end = Some(gerber_pos);
                    }
                    (Some(_), Some(_)) => {
                        self.ruler_start = Some(gerber_pos);
                        self.ruler_end = None;
                    }
                }
            }
        }

        // ESC cancels ruler, origin-setting, and any in-flight zoom-rect.
        if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
            if self.ruler_active {
                self.ruler_active = false;
                self.ruler_start = None;
                self.ruler_end = None;
            }
            self.setting_origin = false;
            self.zoom_rect_start = None;
        }
    }

    /// Paint the manual origin marker — a small red crosshair at the
    /// user-set origin (skipped when the origin is still the default
    /// `(0, 0)` so we don't clutter the canvas).
    fn paint_origin_marker(&self, painter: &egui::Painter) {
        if self.design_offset.x == 0.0 && self.design_offset.y == 0.0 {
            return;
        }
        let p = self.view_state.gerber_to_screen_coords(self.design_offset);
        let arm = 10.0;
        let stroke = egui::Stroke::new(
            1.5,
            egui::Color32::from_rgb(255, 80, 80),
        );
        painter.line_segment(
            [egui::pos2(p.x - arm, p.y), egui::pos2(p.x + arm, p.y)],
            stroke,
        );
        painter.line_segment(
            [egui::pos2(p.x, p.y - arm), egui::pos2(p.x, p.y + arm)],
            stroke,
        );
        painter.circle_filled(p, 2.5, egui::Color32::from_rgb(255, 80, 80));
    }

    /// Build a release zip from the currently-loaded entries and
    /// trigger a browser download. `with_pcbway_specs = true` injects
    /// a generated `PCBWAY_FAB_SPECS.md` next to the original files.
    fn export_release(&mut self, with_pcbway_specs: bool) {
        let Some(ref loaded) = self.loaded else {
            self.error = Some("Nothing loaded to export.".to_string());
            return;
        };

        // Project stem + rev tag from the source filename. The naming
        // convention is `<project>_<rev>[_<DDMmmYYYY>].zip` — split on
        // `_rev_` to find the boundary so a project name with
        // underscores still parses cleanly.
        let stem = loaded
            .source_name
            .strip_suffix(".zip")
            .or_else(|| loaded.source_name.strip_suffix(".ZIP"))
            .unwrap_or(&loaded.source_name);
        let (project_stem, rev_tag) = stem
            .split_once("_rev_")
            .map(|(p, r)| (p.to_string(), format!("rev_{}", r)))
            .unwrap_or_else(|| (stem.to_string(), "rev".to_string()));

        let mut extras: Vec<(String, Vec<u8>)> = Vec::new();
        if with_pcbway_specs {
            let bbox = self.scene.as_ref().and_then(|s| s.combined_bbox());
            // Build the PCBWay payload from joined centroid + BOM data.
            // `smt` / `tht` are wrapped in Option so the writer can
            // omit them when no BOM was uploaded.
            let stats = (!self.centroid.is_empty()).then(|| {
                let s = ComponentStats::build(&self.centroid, &self.bom_mount);
                let bom_known = !self.bom_mount.is_empty();
                let pads_present = self.smt_pads.any();
                release_pkg::PcbwayStats {
                    total: s.total,
                    top: s.top,
                    bottom: s.bottom,
                    smt: bom_known.then_some(s.smt),
                    tht: bom_known.then_some(s.tht),
                    unknown_mount: s.unknown_mount,
                    smt_pads_top: pads_present.then_some(self.smt_pads.top),
                    smt_pads_bottom: pads_present.then_some(self.smt_pads.bottom),
                }
            });
            let md = release_pkg::pcbway_fab_specs_md(
                &project_stem,
                &rev_tag,
                bbox.as_ref().map(|b| b.width()),
                bbox.as_ref().map(|b| b.height()),
                stats.as_ref(),
            );
            extras.push(("PCBWAY_FAB_SPECS.md".to_string(), md.into_bytes()));
        }

        let bytes = match release_pkg::build_zip(&loaded.entries, &extras) {
            Ok(b) => b,
            Err(e) => {
                self.error = Some(e);
                return;
            }
        };

        let download_name = if with_pcbway_specs {
            format!("{}_{}_pcbway.zip", project_stem, rev_tag)
        } else {
            format!("{}_{}.zip", project_stem, rev_tag)
        };
        if let Err(e) = release_pkg::trigger_download(&download_name, &bytes) {
            self.error = Some(format!("Download failed: {}", e));
        } else {
            log::info!("Exported {} ({} bytes)", download_name, bytes.len());
        }
    }

    /// Paint the rubber-band zoom rectangle, if a drag is in progress.
    fn paint_zoom_rect(&self, painter: &egui::Painter, response: &egui::Response) {
        let Some(start) = self.zoom_rect_start else { return };
        let Some(now) = response.hover_pos() else { return };
        let rect = egui::Rect::from_two_pos(start, now);
        if rect.width() < 2.0 || rect.height() < 2.0 {
            return;
        }
        // Bright-white rubber band so it reads cleanly over copper,
        // green soldermask, and silkscreen alike. Fully-opaque 2-px
        // stroke + low-alpha white fill so the inside is tinted but
        // the underlying gerber detail is still legible.
        painter.rect_filled(
            rect,
            0.0,
            egui::Color32::from_rgba_unmultiplied(255, 255, 255, 40),
        );
        painter.rect_stroke(
            rect,
            0.0,
            egui::Stroke::new(2.0, egui::Color32::WHITE),
            egui::StrokeKind::Inside,
        );
    }

    /// Paint the ruler over the canvas — start marker, end marker, line
    /// between them, distance label at the midpoint. Live preview while
    /// the user has placed the start but not the end yet.
    fn paint_ruler(
        &self,
        painter: &egui::Painter,
        response: &egui::Response,
    ) {
        let Some(start_g) = self.ruler_start else { return };
        let start_screen = self.view_state.gerber_to_screen_coords(start_g);

        // Always draw the start marker.
        painter.circle_filled(start_screen, 4.0, egui::Color32::from_rgb(255, 220, 60));

        // Resolve the "end" — either the placed end or the current
        // cursor position for a live preview when start is placed but
        // end isn't yet.
        let (end_g, is_preview) = match self.ruler_end {
            Some(g) => (g, false),
            None => match response.hover_pos() {
                Some(pos) => (self.view_state.screen_to_gerber_coords(pos), true),
                None => return,
            },
        };
        let end_screen = self.view_state.gerber_to_screen_coords(end_g);

        let stroke = egui::Stroke::new(
            1.5,
            egui::Color32::from_rgb(255, if is_preview { 200 } else { 220 }, 60),
        );
        painter.line_segment([start_screen, end_screen], stroke);

        if !is_preview {
            painter.circle_filled(end_screen, 4.0, egui::Color32::from_rgb(255, 220, 60));
        }

        // Distance in mm (gerber coords are in mm post-parse).
        let dx = end_g.x - start_g.x;
        let dy = end_g.y - start_g.y;
        let dist_mm = (dx * dx + dy * dy).sqrt();
        let label = format!("{:.3} mm  ({:.2} mil)", dist_mm, dist_mm / 0.0254);
        let mid = egui::pos2(
            (start_screen.x + end_screen.x) * 0.5,
            (start_screen.y + end_screen.y) * 0.5,
        );
        // Background pill behind the text so it reads over any layer.
        let galley = painter.layout_no_wrap(
            label.clone(),
            egui::FontId::proportional(13.0),
            egui::Color32::WHITE,
        );
        let pad = egui::vec2(6.0, 3.0);
        let bg_rect = egui::Rect::from_center_size(
            mid + egui::vec2(0.0, -16.0),
            galley.size() + pad * 2.0,
        );
        painter.rect_filled(
            bg_rect,
            3.0,
            egui::Color32::from_rgba_unmultiplied(0, 0, 0, 200),
        );
        painter.galley(bg_rect.left_top() + pad, galley, egui::Color32::WHITE);
    }
}

// ── eframe::App ─────────────────────────────────────────────────────────

impl eframe::App for WebApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.drain_pending();

        // ── Top bar ─────────────────────────────────────────────────
        egui::TopBottomPanel::top("top_bar")
            .exact_height(36.0)
            .show(ctx, |ui| {
                ui.horizontal_centered(|ui| {
                    // Brand label is clickable → About modal. Bigger,
                    // bolder text and an underline-on-hover style so
                    // it reads as a real affordance.
                    let brand = ui.add(
                        egui::Label::new(
                            egui::RichText::new("CopperForge")
                                .strong()
                                .size(18.0)
                                .color(egui::Color32::from_rgb(184, 115, 51)),
                        )
                        .sense(egui::Sense::click()),
                    );
                    if brand.hovered() {
                        ctx.set_cursor_icon(egui::CursorIcon::PointingHand);
                    }
                    if brand.clicked() {
                        self.about_open = true;
                    }
                    ui.label(
                        egui::RichText::new("· Browser Demo")
                            .small()
                            .color(egui::Color32::from_rgb(140, 150, 170)),
                    );
                    ui.separator();
                    let btn_label = if self.loading {
                        "⏳ Loading…"
                    } else {
                        "📦 Upload Release ZIP"
                    };
                    if ui
                        .add_enabled(!self.loading, egui::Button::new(btn_label))
                        .clicked()
                    {
                        self.pick_release_zip(ctx);
                    }

                    if self.scene.is_some() {
                        ui.separator();
                        // Grid on/off + spacing + dot size live in the
                        // top bar so they're discoverable without
                        // opening anything.
                        ui.checkbox(&mut self.grid_settings.enabled, "Grid");
                        ui.add_enabled_ui(self.grid_settings.enabled, |ui| {
                            ui.add(
                                egui::DragValue::new(&mut self.grid_settings.spacing_mm)
                                    .speed(0.1)
                                    .range(0.1..=50.0)
                                    .suffix(" mm"),
                            )
                            .on_hover_text("Grid spacing");
                            ui.add(
                                egui::DragValue::new(&mut self.grid_settings.dot_size)
                                    .speed(0.1)
                                    .range(0.5..=6.0)
                                    .suffix(" px"),
                            )
                            .on_hover_text("Grid dot size");
                        });
                        ui.separator();
                        // Ruler toggle. Toggling off clears any partial
                        // measurement so the next activation starts clean.
                        let was_active = self.ruler_active;
                        if ui
                            .toggle_value(&mut self.ruler_active, "📏 Ruler")
                            .changed()
                            && was_active
                            && !self.ruler_active
                        {
                            self.ruler_start = None;
                            self.ruler_end = None;
                        }
                        ui.separator();
                        // Mirror X / Y — applied uniformly to every
                        // layer in paint(), pivoted on the scene bbox
                        // center.
                        ui.toggle_value(&mut self.mirror_x, "↔ X mirror")
                            .on_hover_text("Mirror about the vertical axis");
                        ui.toggle_value(&mut self.mirror_y, "↕ Y mirror")
                            .on_hover_text("Mirror about the horizontal axis");
                        ui.separator();
                        // Origin: click-to-place; one-shot mode that
                        // auto-disarms after the click. Reset returns
                        // the cursor readout to raw gerber coords.
                        ui.toggle_value(&mut self.setting_origin, "📍 Origin")
                            .on_hover_text(
                                "Click anywhere on the canvas to set the \
                                 board origin. Cursor coordinates become \
                                 relative to that point.",
                            );
                        let origin_is_default = self.design_offset.x == 0.0
                            && self.design_offset.y == 0.0;
                        if ui
                            .add_enabled(
                                !origin_is_default,
                                egui::Button::new("↺"),
                            )
                            .on_hover_text("Reset origin to gerber (0, 0)")
                            .clicked()
                        {
                            self.design_offset = Point2::new(0.0, 0.0);
                        }
                        ui.separator();
                        // Release buttons — repackage uploaded entries
                        // into a fresh zip and download via the
                        // browser. The PCBWay variant additionally
                        // generates a PCBWAY_FAB_SPECS.md sheet from
                        // the in-browser bbox + centroid data.
                        if ui
                            .button("🚀 Release")
                            .on_hover_text("Download the uploaded files re-bundled as a release zip")
                            .clicked()
                        {
                            self.export_release(false);
                        }
                        if ui
                            .button("🏭 Release for PCBWay")
                            .on_hover_text(
                                "Same, plus PCBWAY_FAB_SPECS.md \
                                 (board dimensions + component counts)",
                            )
                            .clicked()
                        {
                            self.export_release(true);
                        }
                    }

                    if let Some(ref e) = self.error {
                        ui.separator();
                        ui.colored_label(
                            egui::Color32::from_rgb(220, 100, 100),
                            format!("⚠ {}", e),
                        );
                    }
                    if let Some(ref loaded) = self.loaded {
                        ui.with_layout(
                            egui::Layout::right_to_left(egui::Align::Center),
                            |ui| {
                                ui.label(
                                    egui::RichText::new(format!(
                                        "{} gerber · {} drill · {:.1} KB",
                                        loaded.gerber_count(),
                                        loaded.drill_count(),
                                        loaded.total_bytes() as f64 / 1024.0,
                                    ))
                                    .small()
                                    .color(egui::Color32::from_rgb(160, 180, 200)),
                                );
                                ui.label(
                                    egui::RichText::new(&loaded.source_name)
                                        .strong()
                                        .small(),
                                );
                            },
                        );
                    }
                });
            });

        // ── Left panel: layer checkboxes + presets ─────────────────
        if self.scene.is_some() {
            egui::SidePanel::left("layer_panel")
                .resizable(true)
                .default_width(220.0)
                .width_range(180.0..=360.0)
                .show(ctx, |ui| {
                    ui.add_space(4.0);
                    ui.horizontal(|ui| {
                        ui.label(egui::RichText::new("Layers").strong());
                        ui.with_layout(
                            egui::Layout::right_to_left(egui::Align::Center),
                            |ui| {
                                if ui.small_button("⟲ fit").clicked() {
                                    self.view_initialized = false;
                                }
                            },
                        );
                    });

                    // Presets — All / None / Top / Bottom. EdgeCuts is
                    // forced visible on Top/Bottom so the user sees the
                    // board outline regardless.
                    ui.horizontal(|ui| {
                        if ui.small_button("All").clicked() {
                            apply_preset(self.scene.as_mut(), Preset::All);
                        }
                        if ui.small_button("None").clicked() {
                            apply_preset(self.scene.as_mut(), Preset::None);
                        }
                        if ui.small_button("Top").clicked() {
                            apply_preset(self.scene.as_mut(), Preset::Top);
                        }
                        if ui.small_button("Bottom").clicked() {
                            apply_preset(self.scene.as_mut(), Preset::Bottom);
                        }
                    });
                    ui.separator();

                    egui::ScrollArea::vertical()
                        .auto_shrink([false, false])
                        .show(ui, |ui| {
                            // Iterate in *display* order — render order is
                            // z_order ascending; flip to top-first so the
                            // visible surface sits at the top of the list.
                            if let Some(ref mut scene) = self.scene {
                                let mut indices: Vec<usize> =
                                    (0..scene.layers.len()).collect();
                                indices.sort_by_key(|&i| {
                                    std::cmp::Reverse(scene.layers[i].kind.z_order())
                                });
                                for i in indices {
                                    // Per-layer row: color swatch
                                    // (clickable color picker) + visibility
                                    // checkbox with the layer label. Take
                                    // ONE `&mut RenderLayer` out of the
                                    // Vec, then borrow disjoint fields of
                                    // it inside the closure — borrowck
                                    // sees through field-level access
                                    // when going via a single `&mut T`.
                                    let label = scene.layers[i].display_label();
                                    let layer = &mut scene.layers[i];
                                    let label_color = layer.color;
                                    ui.horizontal(|ui| {
                                        ui.color_edit_button_srgba(&mut layer.color);
                                        ui.checkbox(
                                            &mut layer.visible,
                                            egui::RichText::new(label).color(label_color),
                                        );
                                    });
                                }
                            }
                        });
                });
        }

        // ── Right panel: board stats ───────────────────────────────
        if self.scene.is_some() {
            egui::SidePanel::right("board_stats")
                .resizable(true)
                .default_width(220.0)
                .width_range(180.0..=320.0)
                .show(ctx, |ui| {
                    ui.add_space(4.0);
                    ui.horizontal(|ui| {
                        ui.label(egui::RichText::new("Board").strong());
                        ui.with_layout(
                            egui::Layout::right_to_left(egui::Align::Center),
                            |ui| {
                                ui.selectable_value(&mut self.units_mils, false, "mm");
                                ui.selectable_value(&mut self.units_mils, true, "mil");
                            },
                        );
                    });
                    ui.separator();
                    if let Some(bbox) =
                        self.scene.as_ref().and_then(|s| s.combined_bbox())
                    {
                        let w_mm = bbox.width();
                        let h_mm = bbox.height();
                        let (w, h, unit) = if self.units_mils {
                            (w_mm / 0.0254, h_mm / 0.0254, "mil")
                        } else {
                            (w_mm, h_mm, "mm")
                        };
                        ui.horizontal(|ui| {
                            ui.label("Width");
                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    ui.monospace(format!("{:.2} {}", w, unit));
                                },
                            );
                        });
                        ui.horizontal(|ui| {
                            ui.label("Height");
                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    ui.monospace(format!("{:.2} {}", h, unit));
                                },
                            );
                        });
                        ui.horizontal(|ui| {
                            ui.label("Area");
                            let area = w * h;
                            let suffix = if self.units_mils { "mil²" } else { "mm²" };
                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    ui.monospace(format!("{:.1} {}", area, suffix));
                                },
                            );
                        });
                    } else {
                        ui.label(
                            egui::RichText::new("No Edge.Cuts in loaded zip")
                                .small()
                                .italics()
                                .color(egui::Color32::from_rgb(140, 150, 170)),
                        );
                    }
                    ui.add_space(8.0);
                    ui.separator();
                    ui.label(egui::RichText::new("Components").strong());
                    if self.centroid.is_empty() {
                        ui.label(
                            egui::RichText::new(
                                "No centroid CSV in the upload — \
                                 component counts unavailable.",
                            )
                            .small()
                            .italics()
                            .color(egui::Color32::from_rgb(140, 150, 170)),
                        );
                    } else {
                        let s = ComponentStats::build(
                            &self.centroid,
                            &self.bom_mount,
                        );
                        let row = |ui: &mut egui::Ui, label: &str, n: usize| {
                            ui.horizontal(|ui| {
                                ui.label(label);
                                ui.with_layout(
                                    egui::Layout::right_to_left(egui::Align::Center),
                                    |ui| {
                                        ui.monospace(format!("{}", n));
                                    },
                                );
                            });
                        };
                        row(ui, "Total", s.total);
                        row(ui, "Top", s.top);
                        row(ui, "Bottom", s.bottom);
                        if self.bom_mount.is_empty() {
                            ui.add_space(4.0);
                            ui.label(
                                egui::RichText::new(
                                    "SMT / THT split: no BOM CSV in upload.",
                                )
                                .small()
                                .italics()
                                .color(egui::Color32::from_rgb(140, 150, 170)),
                            );
                        } else {
                            row(ui, "SMT", s.smt);
                            row(ui, "Through-hole", s.tht);
                            if s.unknown_mount > 0 {
                                row(ui, "Unclassified", s.unknown_mount);
                                ui.label(
                                    egui::RichText::new(
                                        "Unclassified = footprint name didn't match \
                                         the SMT/THT heuristics. Usually custom or \
                                         non-KiCad-stock libraries.",
                                    )
                                    .small()
                                    .italics()
                                    .color(egui::Color32::from_rgb(140, 150, 170)),
                                );
                            }
                        }
                    }
                    // SMT pad count — independent of the BOM; derived
                    // from F.Paste / B.Paste flashes. Only render the
                    // sub-table if at least one paste layer was found.
                    if self.smt_pads.any() {
                        ui.add_space(6.0);
                        ui.label(egui::RichText::new("SMT pads").strong());
                        let row = |ui: &mut egui::Ui, label: &str, n: usize| {
                            ui.horizontal(|ui| {
                                ui.label(label);
                                ui.with_layout(
                                    egui::Layout::right_to_left(egui::Align::Center),
                                    |ui| {
                                        ui.monospace(format!("{}", n));
                                    },
                                );
                            });
                        };
                        row(ui, "Top", self.smt_pads.top);
                        row(ui, "Bottom", self.smt_pads.bottom);
                        row(ui, "Total", self.smt_pads.total());
                    }

                    // ── Weight (approximation) ─────────────────────
                    ui.add_space(6.0);
                    ui.separator();
                    ui.label(egui::RichText::new("Weight").strong());
                    ui.label(
                        egui::RichText::new(
                            "Bare board, no components. Approximation: bbox × \
                             fill %. Adjust the inputs for your stackup.",
                        )
                        .small()
                        .italics()
                        .color(egui::Color32::from_rgb(140, 150, 170)),
                    );
                    egui::Grid::new("weight_inputs")
                        .num_columns(2)
                        .spacing([8.0, 4.0])
                        .show(ui, |ui| {
                            ui.label("Thickness");
                            ui.add(
                                egui::DragValue::new(
                                    &mut self.weight_inputs.board_thickness_mm,
                                )
                                .speed(0.1)
                                .range(0.2..=6.0)
                                .suffix(" mm"),
                            );
                            ui.end_row();
                            ui.label("Outer Cu");
                            ui.add(
                                egui::DragValue::new(
                                    &mut self.weight_inputs.copper_oz_outer,
                                )
                                .speed(0.25)
                                .range(0.25..=8.0)
                                .suffix(" oz"),
                            );
                            ui.end_row();
                            ui.label("Inner Cu");
                            ui.add(
                                egui::DragValue::new(
                                    &mut self.weight_inputs.copper_oz_inner,
                                )
                                .speed(0.25)
                                .range(0.25..=8.0)
                                .suffix(" oz"),
                            );
                            ui.end_row();
                            ui.label("Fill %");
                            ui.add(
                                egui::DragValue::new(
                                    &mut self.weight_inputs.copper_fill_pct,
                                )
                                .speed(1.0)
                                .range(5.0..=100.0)
                                .suffix(" %"),
                            );
                            ui.end_row();
                        });

                    if let Some(w) =
                        self.scene
                            .as_ref()
                            .and_then(|s| board_weight::compute(s, &self.weight_inputs))
                    {
                        ui.add_space(4.0);
                        let row = |ui: &mut egui::Ui, label: &str, g: f64| {
                            ui.horizontal(|ui| {
                                ui.label(label);
                                ui.with_layout(
                                    egui::Layout::right_to_left(egui::Align::Center),
                                    |ui| {
                                        ui.monospace(format!("{:.2} g", g));
                                    },
                                );
                            });
                        };
                        row(ui, "Substrate (FR4)", w.substrate_g);
                        row(
                            ui,
                            &format!("Outer Cu ×{}", w.outer_copper_layers),
                            w.copper_outer_g,
                        );
                        if w.inner_copper_layers > 0 {
                            row(
                                ui,
                                &format!("Inner Cu ×{}", w.inner_copper_layers),
                                w.copper_inner_g,
                            );
                        }
                        ui.add_space(2.0);
                        ui.horizontal(|ui| {
                            ui.label(egui::RichText::new("Total").strong());
                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    ui.monospace(
                                        egui::RichText::new(format!(
                                            "{:.2} g",
                                            w.total_g
                                        ))
                                        .strong(),
                                    );
                                },
                            );
                        });
                    }
                });
        }

        // ── Bottom status bar: cursor coordinates ──────────────────
        if self.scene.is_some() {
            egui::TopBottomPanel::bottom("status_bar")
                .exact_height(22.0)
                .show(ctx, |ui| {
                    ui.horizontal_centered(|ui| {
                        match self.cursor_world {
                            Some(p) => {
                                // Coordinates are reported RELATIVE to
                                // the user-set origin. With the default
                                // origin (0, 0) this is identical to
                                // the raw gerber position.
                                let rx = p.x - self.design_offset.x;
                                let ry = p.y - self.design_offset.y;
                                let (x, y, unit) = if self.units_mils {
                                    (rx / 0.0254, ry / 0.0254, "mil")
                                } else {
                                    (rx, ry, "mm")
                                };
                                let origin_is_default = self.design_offset.x == 0.0
                                    && self.design_offset.y == 0.0;
                                let origin_tag = if origin_is_default {
                                    String::new()
                                } else {
                                    format!(
                                        "  (origin {:.2}, {:.2} mm)",
                                        self.design_offset.x,
                                        self.design_offset.y,
                                    )
                                };
                                ui.label(
                                    egui::RichText::new(format!(
                                        "Cursor:  X = {:>10.3} {}    Y = {:>10.3} {}{}",
                                        x, unit, y, unit, origin_tag,
                                    ))
                                    .monospace()
                                    .color(egui::Color32::from_rgb(180, 200, 220)),
                                );
                            }
                            None => {
                                ui.label(
                                    egui::RichText::new(
                                        "Cursor: hover the canvas for live coordinates",
                                    )
                                    .small()
                                    .italics()
                                    .color(egui::Color32::from_rgb(140, 150, 170)),
                                );
                            }
                        }
                        ui.with_layout(
                            egui::Layout::right_to_left(egui::Align::Center),
                            |ui| {
                                // 100% = scale that fits the whole board
                                // (base_scale set by ViewState::fit_view).
                                // `view_state.scale` is pixels-per-mm, not
                                // a percentage — ratioing against the fit
                                // scale gives a sensible "zoom from fit".
                                let pct = if self.view_state.base_scale > 0.0 {
                                    (self.view_state.scale
                                        / self.view_state.base_scale)
                                        * 100.0
                                } else {
                                    100.0
                                };
                                ui.label(
                                    egui::RichText::new(format!("Zoom: {:.0}%", pct))
                                        .small()
                                        .color(egui::Color32::from_rgb(140, 150, 170)),
                                );
                            },
                        );
                    });
                });
        }

        // ── About modal ────────────────────────────────────────────
        if self.about_open {
            egui::Window::new("About CopperForge")
                .open(&mut self.about_open)
                .collapsible(false)
                .resizable(false)
                .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
                .default_width(420.0)
                .show(ctx, |ui| {
                    ui.add_space(4.0);
                    ui.heading(
                        egui::RichText::new("CopperForge")
                            .color(egui::Color32::from_rgb(184, 115, 51)),
                    );
                    ui.label(
                        egui::RichText::new("PCB & CAM companion for KiCad")
                            .italics(),
                    );
                    ui.add_space(8.0);
                    ui.separator();
                    ui.add_space(8.0);
                    ui.label(
                        "This is the wasm browser demo. It runs the same \
                         egui app the desktop version does, parsing your \
                         release ZIP entirely in your browser — no upload, \
                         no server.",
                    );
                    ui.add_space(8.0);
                    ui.label(
                        "Drag a CopperForge release zip onto Upload, then \
                         pan with right-mouse drag, zoom with the wheel, \
                         and left-drag a region to zoom in.",
                    );
                    ui.add_space(12.0);
                    ui.horizontal(|ui| {
                        ui.label("Source:");
                        ui.hyperlink_to(
                            "github.com/Atlantix-EDA/CopperForge",
                            "https://github.com/Atlantix-EDA/CopperForge",
                        );
                    });
                    ui.horizontal(|ui| {
                        ui.label("Version:");
                        ui.monospace(env!("CARGO_PKG_VERSION"));
                    });
                });
        }

        // ── Central panel: the gerber canvas ───────────────────────
        egui::CentralPanel::default().show(ctx, |ui| {
            if self.scene.is_none() {
                ui.centered_and_justified(|ui| {
                    ui.label(
                        egui::RichText::new(
                            "Upload a release ZIP from the toolbar to begin.",
                        )
                        .italics()
                        .color(egui::Color32::from_rgb(140, 150, 170)),
                    );
                });
                return;
            }

            // Carve out the full panel as the drawing canvas and grab a
            // drag-sensitive response so right-drag pan + wheel zoom can
            // hook in.
            let (rect, response) = ui.allocate_exact_size(
                ui.available_size(),
                egui::Sense::click_and_drag(),
            );
            ui.painter().rect_filled(
                rect,
                0.0,
                egui::Color32::from_rgb(18, 22, 28),
            );

            // First-paint fit-to-view, and after each new upload.
            if !self.view_initialized {
                if let Some(ref scene) = self.scene {
                    if let Some(bbox) = scene.combined_bbox() {
                        self.view_state.fit_view(rect, &bbox, 1.0);
                        self.view_initialized = true;
                    }
                }
            }

            // Live cursor coordinates for the bottom status bar.
            // None when the pointer leaves the canvas, so the bar
            // shows its idle hint rather than a stale last-position.
            self.cursor_world = response
                .hover_pos()
                .map(|p| self.view_state.screen_to_gerber_coords(p));

            self.handle_canvas_input(ui, &response);

            // Clip the painter to the canvas rect so nothing bleeds
            // into the side panel during pan/zoom.
            let painter = ui.painter_at(rect);

            // Grid first (under everything else).
            draw_grid(&painter, &rect, &self.view_state, &self.grid_settings);

            // Build the image transform — mirroring is pivoted on the
            // scene's bbox center so the board doesn't fly off-canvas
            // when toggled.
            if let Some(ref scene) = self.scene {
                let pivot = scene
                    .combined_bbox()
                    .map(|b| b.center())
                    .unwrap_or_else(|| Point2::new(0.0, 0.0));
                let transform = GerberTransform {
                    mirroring: Mirroring {
                        x: self.mirror_x,
                        y: self.mirror_y,
                    },
                    origin: Vector2::new(pivot.x, pivot.y),
                    ..Default::default()
                };
                paint_canvas(&painter, scene, self.view_state, &transform);
            }

            // Origin marker — sits above the gerbers but below the
            // ruler so a ruler line crossing the origin is still legible.
            self.paint_origin_marker(&painter);

            // Ruler overlay (below the zoom-rect so a zoom drag's
            // rectangle sits on top of any partial measurement).
            self.paint_ruler(&painter, &response);

            // Zoom-to-region rubber band — paints only while a drag is
            // in flight.
            self.paint_zoom_rect(&painter, &response);

            // Cursor hint: crosshair while ruler or origin-setting
            // armed, otherwise the egui default (arrow). Zoom-rect
            // drag inherits whatever's active.
            if self.ruler_active || self.setting_origin {
                ctx.set_cursor_icon(egui::CursorIcon::Crosshair);
            }
        });
    }
}

// ── Layer-visibility presets ────────────────────────────────────────────

#[derive(Debug, Clone, Copy)]
enum Preset {
    All,
    None,
    /// Top-side layers + EdgeCuts; bottom layers off; neutrals (inner
    /// copper, Other) off.
    Top,
    /// Mirror of `Top`.
    Bottom,
}

fn apply_preset(scene: Option<&mut GerberScene>, preset: Preset) {
    use crate::canvas::model::LayerKind;
    let Some(scene) = scene else { return };
    for layer in &mut scene.layers {
        layer.visible = match preset {
            Preset::All => true,
            Preset::None => false,
            Preset::Top => match layer.kind.side() {
                LayerSide::Top => true,
                LayerSide::Bottom => false,
                LayerSide::Neutral => matches!(layer.kind, LayerKind::EdgeCuts),
            },
            Preset::Bottom => match layer.kind.side() {
                LayerSide::Top => false,
                LayerSide::Bottom => true,
                LayerSide::Neutral => matches!(layer.kind, LayerKind::EdgeCuts),
            },
        };
    }
}

// ── File-pick + unzip ───────────────────────────────────────────────────

async fn run_upload() -> Option<Result<LoadedRelease, String>> {
    let handle = rfd::AsyncFileDialog::new()
        .add_filter("Release ZIP", &["zip"])
        .pick_file()
        .await?;
    let source_name = handle.file_name();
    let bytes = handle.read().await;
    Some(unzip_release(source_name, bytes))
}

fn unzip_release(source_name: String, bytes: Vec<u8>) -> Result<LoadedRelease, String> {
    use std::io::{Cursor, Read};
    use zip::ZipArchive;

    let cursor = Cursor::new(bytes);
    let mut archive =
        ZipArchive::new(cursor).map_err(|e| format!("Not a valid ZIP archive: {}", e))?;

    let mut entries = BTreeMap::new();
    let mut had_gerber_or_drill = false;
    for i in 0..archive.len() {
        let mut entry = archive
            .by_index(i)
            .map_err(|e| format!("Reading entry {}: {}", i, e))?;
        if !entry.is_file() {
            continue;
        }
        let name = entry.name().to_string();
        let lower = name.to_lowercase();
        // Skip OS sidecar files that ride along in zips but aren't
        // real content (macOS __MACOSX/, .DS_Store, Windows Thumbs.db).
        if lower.contains("__macosx/")
            || lower.ends_with("/.ds_store")
            || lower == ".ds_store"
            || lower.ends_with("/thumbs.db")
            || lower == "thumbs.db"
        {
            continue;
        }
        if lower.ends_with(".gbr") || lower.ends_with(".drl") {
            had_gerber_or_drill = true;
        }
        let basename = std::path::Path::new(&name)
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or(&name)
            .to_string();

        let mut buf = Vec::with_capacity(entry.size() as usize);
        entry
            .read_to_end(&mut buf)
            .map_err(|e| format!("Decompressing {}: {}", name, e))?;
        entries.insert(basename, buf);
    }

    if !had_gerber_or_drill {
        return Err(
            "No .gbr or .drl files found in the ZIP. \
             Pick a CopperForge release zip or a gerber+drill bundle."
                .to_string(),
        );
    }

    Ok(LoadedRelease {
        source_name,
        entries,
    })
}
