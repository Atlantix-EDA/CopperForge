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
use copperforge_core::panels::Board3dView;
use gerber_viewer::{GerberTransform, Mirroring, ViewState};
use nalgebra::{Point2, Vector2};

use std::collections::HashMap;

use egui_dock::{DockArea, DockState, NodeIndex};

use crate::board3d::Board3dGeom;
use crate::board_weight::{self, WeightInputs};
use crate::bom::{self, Mount};
use crate::canvas::model::LayerSide;
use crate::canvas::{paint as paint_canvas, GerberScene};
use crate::centroid::{self, CentroidEntry};
use crate::pad_count::{self, SmtPadCount};
use crate::release_pkg;
use crate::state::{self, Logger};
use crate::tabs::{Tab, TabKind, TabViewer};

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

    /// IANA timezone name for the top-bar clock — `None` = browser
    /// local. Set in the Settings tab; consumed by
    /// `browser_local_clock` via JS `Intl.DateTimeFormat`.
    user_timezone: Option<String>,

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
    /// Grouped-part stats (unique vs total placements) for the v1
    /// manufacturability metric. `None` until a BOM with a Value column
    /// is parsed from the upload.
    part_stats: Option<bom::PartStats>,

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

    /// Append-only log buffer rendered by the Logger dock tab.
    /// Bounded at 1000 entries. Placeholder for egui_lens until the
    /// egui_mobius monorepo (egui 0.34) and gerber_viewer (egui 0.33)
    /// align on a common egui version.
    pub logger: Logger,

    /// 3D board view — same renderer the native app uses, driven directly
    /// (no citizen framework). Owns camera + GPU mesh state across frames.
    board3d_view: Board3dView,
    /// Tessellated 3D meshes for the current upload (outline / copper /
    /// mask). Rebuilt once per release; `None` until the first load.
    board3d_geom: Option<Board3dGeom>,
    /// The eframe glow (WebGL2) context, restashed each frame from
    /// `frame.gl()`. `Board3dView::show` needs it to upload/draw meshes;
    /// `render_*_tab` only gets `&mut Ui`, so we thread it through here.
    gl: Option<Arc<eframe::glow::Context>>,

    /// egui_dock layout. Tabs become draggable / splittable / closable;
    /// fresh sessions get the layout from `default_dock_layout()`.
    pub dock_state: DockState<Tab>,
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
            user_timezone: None,
            centroid: Vec::new(),
            bom_mount: HashMap::new(),
            smt_pads: SmtPadCount::default(),
            part_stats: None,
            weight_inputs: WeightInputs::default(),
            design_offset: Point2::new(0.0, 0.0),
            setting_origin: false,
            cursor_world: None,
            logger: Logger::new(),
            board3d_view: Board3dView::new(),
            board3d_geom: None,
            gl: None,
            dock_state: default_dock_layout(),
        }
    }
}

/// Initial dock layout — Canvas in the centre, Layers docked left,
/// Board docked right, Logger across the bottom. Matches zicad's
/// general shape (sidebar + main + bottom log) adapted to PCB review.
///
/// ```text
/// ┌────────┬──────────────────┬────────┐
/// │ Layers │      Canvas      │ Board  │
/// ├────────┴──────────────────┴────────┤
/// │              Logger                │
/// └────────────────────────────────────┘
/// ```
fn default_dock_layout() -> DockState<Tab> {
    // Centre node hosts the 2D Canvas and the 3D Board as sibling tabs —
    // Canvas active on launch, 3D one click away.
    let mut dock = DockState::new(vec![
        Tab::new(TabKind::Canvas),
        Tab::new(TabKind::Board3d),
    ]);
    let surface = dock.main_surface_mut();
    // Right side hosts Board and Settings as sibling tabs — Board is
    // listed first so it's active on launch; Settings is one tab-click
    // away.
    let [_, _right] = surface.split_right(
        NodeIndex::root(),
        0.78,
        vec![
            Tab::new(TabKind::Board),
            Tab::new(TabKind::Settings),
        ],
    );
    let [_, _left] =
        surface.split_left(NodeIndex::root(), 0.22, vec![Tab::new(TabKind::Layers)]);
    let [_, _bottom] =
        surface.split_below(NodeIndex::root(), 0.78, vec![Tab::new(TabKind::Logger)]);
    dock
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
    /// Decompress the bundled example zip + drive the same code path
    /// as a successful upload. Synchronous (no file picker / no
    /// async), so the demo loads instantly on click.
    fn load_example_release(&mut self) {
        const EXAMPLE_ZIP: &[u8] =
            include_bytes!("../../../assets/media/cparti-fpga-dev-board.zip");
        let result = unzip_release(
            "cparti-fpga-dev-board.zip".to_string(),
            EXAMPLE_ZIP.to_vec(),
        );
        // Stuff the result into pending_load so `drain_pending`
        // handles it identically to a user-picked file — log lines,
        // scene build, centroid/BOM parse, everything.
        *self.pending_load.lock().unwrap() = Some(result);
    }

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
                let centroid = centroid::find_and_parse(&loaded.entries)
                    .unwrap_or_default();
                let bom_mount = bom::find_and_parse(&loaded.entries);
                let part_stats = bom::find_and_parse_parts(&loaded.entries);
                let smt_pads = pad_count::count_from_entries(&loaded.entries);

                // Logger entries cover the full upload+parse flow so
                // the Logger tab tells a story instead of being silent.
                self.logger.custom(
                    "upload",
                    format!(
                        "Loaded {} — {} gerber + {} drill ({:.1} KB)",
                        loaded.source_name,
                        loaded.gerber_count(),
                        loaded.drill_count(),
                        loaded.total_bytes() as f64 / 1024.0,
                    ),
                );
                self.logger
                    .custom("parse", format!("Parsed {} gerber layers", total_layers));
                if !centroid.is_empty() {
                    self.logger.custom(
                        "parse",
                        format!("Centroid: {} component placements", centroid.len()),
                    );
                }
                if !bom_mount.is_empty() {
                    self.logger.custom(
                        "parse",
                        format!("BOM: {} designators classified", bom_mount.len()),
                    );
                }
                if let Some(ref ps) = part_stats {
                    self.logger.custom(
                        "parse",
                        format!(
                            "Parts: {} unique / {} total ({:.0}% unique, {:.1}× reuse)",
                            ps.unique_parts,
                            ps.total_parts,
                            ps.unique_ratio() * 100.0,
                            ps.reuse(),
                        ),
                    );
                }
                if smt_pads.any() {
                    self.logger.custom(
                        "parse",
                        format!(
                            "SMT pads: {} top + {} bottom = {} total",
                            smt_pads.top,
                            smt_pads.bottom,
                            smt_pads.total()
                        ),
                    );
                }

                // Tessellate the 3D meshes once, up front. Logged so the
                // Logger tab reflects what the 3D tab will show.
                let board3d_geom = Board3dGeom::build_from_entries(&loaded.entries);
                if board3d_geom.outline.is_some() {
                    let cu = board3d_geom.top_copper.is_some() as u8
                        + board3d_geom.bottom_copper.is_some() as u8;
                    let mask = board3d_geom.top_mask.is_some() as u8
                        + board3d_geom.bottom_mask.is_some() as u8;
                    self.logger.custom(
                        "3d",
                        format!("3D: board outline + {cu} copper + {mask} mask layer(s)"),
                    );
                } else {
                    self.logger
                        .custom("3d", "3D: no edge-cuts layer — board outline unavailable");
                }
                self.board3d_geom = Some(board3d_geom);
                // New geometry → force the 3D view to drop the previous
                // board's meshes and re-upload (its change detection alone
                // treats Some→Some as "unchanged").
                self.board3d_view.mark_dirty();

                self.scene = Some(scene);
                self.view_initialized = false;
                self.centroid = centroid;
                self.bom_mount = bom_mount;
                self.part_stats = part_stats;
                self.smt_pads = smt_pads;
                self.loaded = Some(loaded);
                self.error = None;
            }
            Err(e) if e == "canceled" => {
                self.logger.info("Upload canceled");
            }
            Err(e) => {
                self.logger.error(format!("Upload failed: {}", e));
                self.error = Some(e);
            }
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
                self.logger.custom(
                    "origin",
                    format!("Origin set to ({:.3}, {:.3}) mm", g.x, g.y),
                );
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
            self.logger.error(format!("Export failed: {}", e));
        } else {
            self.logger.custom(
                "export",
                format!(
                    "Exported {} ({:.1} KB)",
                    download_name,
                    bytes.len() as f64 / 1024.0,
                ),
            );
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

// ── Per-tab render methods ──────────────────────────────────────────────
//
// Each method renders one dock tab's body. They live on `WebApp` so the
// existing state stays close to the UI that mutates it; the TabViewer
// in `tabs.rs` is just dispatch.

impl WebApp {
    /// Layers tab — preset row, scrollable per-layer checkboxes with
    /// color swatches. Empty hint when no scene has been parsed yet.
    pub fn render_layer_tab(&mut self, ui: &mut egui::Ui) {
        if self.scene.is_none() {
            ui.label(
                egui::RichText::new("Upload a release ZIP to see layers.")
                    .small()
                    .italics()
                    .color(egui::Color32::from_rgb(140, 150, 170)),
            );
            return;
        }
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
                if let Some(ref mut scene) = self.scene {
                    let mut indices: Vec<usize> = (0..scene.layers.len()).collect();
                    indices.sort_by_key(|&i| {
                        std::cmp::Reverse(scene.layers[i].kind.z_order())
                    });
                    for i in indices {
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
    }

    /// Board tab — dimensions, component counts, SMT pads, weight calc.
    /// Renders an empty hint pre-upload.
    pub fn render_board_tab(&mut self, ui: &mut egui::Ui) {
        if self.scene.is_none() {
            ui.label(
                egui::RichText::new("Upload a release ZIP to see board stats.")
                    .small()
                    .italics()
                    .color(egui::Color32::from_rgb(140, 150, 170)),
            );
            return;
        }
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
        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
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
                    let row_kv = |ui: &mut egui::Ui, k: &str, v: String| {
                        ui.horizontal(|ui| {
                            ui.label(k);
                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    ui.monospace(v);
                                },
                            );
                        });
                    };
                    row_kv(ui, "Width", format!("{:.2} {}", w, unit));
                    row_kv(ui, "Height", format!("{:.2} {}", h, unit));
                    let area = w * h;
                    let suffix = if self.units_mils { "mil²" } else { "mm²" };
                    row_kv(ui, "Area", format!("{:.1} {}", area, suffix));
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
                    let s = ComponentStats::build(&self.centroid, &self.bom_mount);
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
                        }
                    }
                }
                if self.smt_pads.any() {
                    ui.add_space(6.0);
                    ui.label(egui::RichText::new("SMT pads").strong());
                    row(ui, "Top", self.smt_pads.top);
                    row(ui, "Bottom", self.smt_pads.bottom);
                    row(ui, "Total", self.smt_pads.total());
                }

                // ── Manufacturability (v1) ──────────────────────────
                // Part-commonality metrics + a transparent score. The
                // raw ratio is free-tier funnel candy; the `score` body
                // is the planned v2 fuzzy-inference swap point.
                let kv = |ui: &mut egui::Ui, k: &str, v: String| {
                    ui.horizontal(|ui| {
                        ui.label(k);
                        ui.with_layout(
                            egui::Layout::right_to_left(egui::Align::Center),
                            |ui| {
                                ui.monospace(v);
                            },
                        );
                    });
                };
                ui.add_space(6.0);
                ui.separator();
                ui.label(egui::RichText::new("Manufacturability").strong());
                if let Some(ref ps) = self.part_stats {
                    row(ui, "Unique parts", ps.unique_parts);
                    kv(
                        ui,
                        "Unique / total",
                        format!("{:.0} %", ps.unique_ratio() * 100.0),
                    );
                    kv(ui, "Reuse", format!("{:.1}× /part", ps.reuse()));

                    let m = crate::manufacturability::score(
                        ps.unique_ratio(),
                        ps.tht_fraction(),
                        ps.total_parts,
                    );
                    let color = match m.grade {
                        'A' | 'B' => egui::Color32::from_rgb(120, 200, 140),
                        'C' => egui::Color32::from_rgb(220, 200, 120),
                        _ => egui::Color32::from_rgb(220, 140, 120),
                    };
                    ui.horizontal(|ui| {
                        ui.label("Score");
                        ui.with_layout(
                            egui::Layout::right_to_left(egui::Align::Center),
                            |ui| {
                                ui.monospace(
                                    egui::RichText::new(format!(
                                        "{} / 100  ({})",
                                        m.score, m.grade
                                    ))
                                    .color(color),
                                );
                            },
                        );
                    })
                    .response
                    .on_hover_text(
                        "v1 heuristic: part commonality + through-hole \
                         fraction + size. Higher = easier to assemble. \
                         Weights are being calibrated.",
                    );
                    if !ps.keyed_by_mpn {
                        ui.label(
                            egui::RichText::new(
                                "unique keyed by value + package (no MPN column)",
                            )
                            .small()
                            .italics()
                            .color(egui::Color32::from_rgb(140, 150, 170)),
                        );
                    }
                } else {
                    ui.label(
                        egui::RichText::new(
                            "Needs a BOM CSV with a Value column.",
                        )
                        .small()
                        .italics()
                        .color(egui::Color32::from_rgb(140, 150, 170)),
                    );
                }

                ui.add_space(6.0);
                ui.separator();
                ui.label(egui::RichText::new("Weight").strong());
                ui.label(
                    egui::RichText::new(
                        "Bare board, no components. Approximation: bbox × \
                         fill %.",
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
                if let Some(w) = self
                    .scene
                    .as_ref()
                    .and_then(|s| board_weight::compute(s, &self.weight_inputs))
                {
                    ui.add_space(4.0);
                    let row_g = |ui: &mut egui::Ui, label: &str, g: f64| {
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
                    row_g(ui, "Substrate (FR4)", w.substrate_g);
                    row_g(
                        ui,
                        &format!("Outer Cu ×{}", w.outer_copper_layers),
                        w.copper_outer_g,
                    );
                    if w.inner_copper_layers > 0 {
                        row_g(
                            ui,
                            &format!("Inner Cu ×{}", w.inner_copper_layers),
                            w.copper_inner_g,
                        );
                    }
                    ui.horizontal(|ui| {
                        ui.label(egui::RichText::new("Total").strong());
                        ui.with_layout(
                            egui::Layout::right_to_left(egui::Align::Center),
                            |ui| {
                                ui.monospace(
                                    egui::RichText::new(format!("{:.2} g", w.total_g))
                                        .strong(),
                                );
                            },
                        );
                    });
                }
            });
    }

    /// Canvas tab — the gerber viewport. Allocates the full available
    /// rect, paints grid + scene + overlays, handles right-drag pan /
    /// wheel zoom / left-drag rubber-band / ruler / origin.
    /// The 3D board tab — axes/grid always; board outline + copper +
    /// soldermask once a release with an edge-cuts layer is loaded. Shares
    /// `copperforge_core::panels::Board3dView` with the native app.
    pub fn render_board3d_tab(&mut self, ui: &mut egui::Ui) {
        if self.board3d_geom.is_none() {
            ui.centered_and_justified(|ui| {
                ui.label(
                    egui::RichText::new(
                        "Upload a release ZIP from the toolbar to see the board in 3D.",
                    )
                    .italics()
                    .color(egui::Color32::from_rgb(140, 150, 170)),
                );
            });
            return;
        }
        // Disjoint field borrows: `board3d_view` mut, `gl`/`board3d_geom`
        // shared — distinct fields, so the borrow checker allows it.
        let geom = self.board3d_geom.as_ref();
        self.board3d_view.show(
            ui,
            self.gl.as_ref(),
            geom.and_then(|g| g.outline.as_ref()),
            geom.and_then(|g| g.top_copper.as_ref()),
            geom.and_then(|g| g.bottom_copper.as_ref()),
            geom.and_then(|g| g.top_mask.as_ref()),
            geom.and_then(|g| g.bottom_mask.as_ref()),
            geom.and_then(|g| g.drill.as_ref()),
            self.units_mils,
        );
    }

    pub fn render_canvas_tab(&mut self, ui: &mut egui::Ui) {
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

        let (rect, response) = ui.allocate_exact_size(
            ui.available_size(),
            egui::Sense::click_and_drag(),
        );
        ui.painter()
            .rect_filled(rect, 0.0, egui::Color32::from_rgb(18, 22, 28));

        if !self.view_initialized {
            if let Some(ref scene) = self.scene {
                if let Some(bbox) = scene.combined_bbox() {
                    self.view_state.fit_view(rect, &bbox, 1.0);
                    self.view_initialized = true;
                }
            }
        }

        self.cursor_world = response
            .hover_pos()
            .map(|p| self.view_state.screen_to_gerber_coords(p));

        self.handle_canvas_input(ui, &response);

        let painter = ui.painter_at(rect);
        draw_grid(&painter, &rect, &self.view_state, &self.grid_settings);

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

        self.paint_origin_marker(&painter);
        self.paint_ruler(&painter, &response);
        self.paint_zoom_rect(&painter, &response);

        if self.ruler_active || self.setting_origin {
            ui.ctx().set_cursor_icon(egui::CursorIcon::Crosshair);
        }
    }

    /// Logger tab — renders the in-house buffer + toolbar (System,
    /// Clear, per-level filter). Toolbar emits a `LogAction` rather
    /// than mutating the buffer directly so the System dump can read
    /// other `WebApp` state (loaded zip, scene, timezone) before
    /// being inserted.
    pub fn render_logger_tab(&mut self, ui: &mut egui::Ui) {
        let avail = ui.available_size_before_wrap();
        let action = ui
            .allocate_ui_with_layout(
                avail,
                egui::Layout::top_down(egui::Align::Min),
                |ui| state::show_log(ui, &mut self.logger),
            )
            .inner;
        if action.clear_requested {
            self.logger.clear();
            self.logger.info("Log cleared");
        }
        if action.system_info_requested {
            self.log_system_info();
        }
    }

    /// Snapshot of app + browser context, emitted as a block of log
    /// entries. Useful for bug reports and the "what state was the
    /// user in" question.
    fn log_system_info(&mut self) {
        let ua = web_sys::window()
            .and_then(|w| w.navigator().user_agent().ok())
            .unwrap_or_else(|| "unknown".to_string());
        let tz = self
            .user_timezone
            .clone()
            .unwrap_or_else(|| "browser local".to_string());
        self.logger.info("── System snapshot ──");
        self.logger
            .info(format!("CopperForge web {}", env!("CARGO_PKG_VERSION")));
        self.logger.info("Target: wasm32-unknown-unknown");
        self.logger.info(format!("User agent: {}", ua));
        self.logger.info(format!("Clock timezone: {}", tz));
        if let Some(ref loaded) = self.loaded {
            self.logger.info(format!(
                "Loaded: {} ({:.1} KB, {} entries)",
                loaded.source_name,
                loaded.total_bytes() as f64 / 1024.0,
                loaded.entries.len(),
            ));
        } else {
            self.logger.info("Loaded: (nothing — upload a release zip)");
        }
        if let Some(ref scene) = self.scene {
            self.logger
                .info(format!("Parsed gerber layers: {}", scene.layers.len()));
        }
        if !self.centroid.is_empty() {
            self.logger
                .info(format!("Centroid placements: {}", self.centroid.len()));
        }
        if !self.bom_mount.is_empty() {
            self.logger
                .info(format!("BOM-classified designators: {}", self.bom_mount.len()));
        }
        if self.smt_pads.any() {
            self.logger.info(format!(
                "SMT pads: {} top / {} bottom",
                self.smt_pads.top, self.smt_pads.bottom,
            ));
        }
        let origin = if self.design_offset.x == 0.0 && self.design_offset.y == 0.0 {
            "(0.000, 0.000) — default".to_string()
        } else {
            format!(
                "({:.3}, {:.3}) mm",
                self.design_offset.x, self.design_offset.y
            )
        };
        self.logger.info(format!("Design origin: {}", origin));
    }

    /// Settings tab — user preferences. Today just the timezone for
    /// the top-bar clock; future expansion: theme, default grid,
    /// units, layer-preset names.
    pub fn render_settings_tab(&mut self, ui: &mut egui::Ui) {
        ui.add_space(4.0);
        ui.label(egui::RichText::new("Settings").strong());
        ui.label(
            egui::RichText::new("Preferences applied immediately.")
                .small()
                .italics()
                .color(egui::Color32::from_rgb(140, 150, 170)),
        );
        ui.separator();
        ui.add_space(6.0);

        ui.label(egui::RichText::new("Clock timezone").strong());
        ui.label(
            egui::RichText::new(
                "Used by the clock in the upper-right of the ribbon.",
            )
            .small()
            .color(egui::Color32::from_rgb(140, 150, 170)),
        );
        ui.add_space(4.0);

        // Common IANA timezones, in roughly geographic order. Browser
        // local is the default first entry.
        let zones: &[(Option<&str>, &str)] = &[
            (None, "Browser local"),
            (Some("UTC"), "UTC"),
            (Some("America/Los_Angeles"), "America / Los Angeles (Pacific)"),
            (Some("America/Denver"), "America / Denver (Mountain)"),
            (Some("America/Chicago"), "America / Chicago (Central)"),
            (Some("America/New_York"), "America / New York (Eastern)"),
            (Some("America/Toronto"), "America / Toronto"),
            (Some("America/Sao_Paulo"), "America / São Paulo"),
            (Some("Europe/London"), "Europe / London"),
            (Some("Europe/Paris"), "Europe / Paris"),
            (Some("Europe/Berlin"), "Europe / Berlin"),
            (Some("Europe/Helsinki"), "Europe / Helsinki"),
            (Some("Asia/Dubai"), "Asia / Dubai"),
            (Some("Asia/Kolkata"), "Asia / Kolkata"),
            (Some("Asia/Shanghai"), "Asia / Shanghai"),
            (Some("Asia/Tokyo"), "Asia / Tokyo"),
            (Some("Australia/Sydney"), "Australia / Sydney"),
        ];
        let current = self.user_timezone.clone();
        let current_label = zones
            .iter()
            .find(|(tz, _)| tz.map(String::from) == current)
            .map(|(_, label)| *label)
            .unwrap_or("Browser local");
        egui::ComboBox::from_id_salt("settings_timezone_combo")
            .selected_text(current_label)
            .width(280.0)
            .show_ui(ui, |ui| {
                for (tz, label) in zones {
                    let owned = tz.map(String::from);
                    let was_selected = self.user_timezone == owned;
                    if ui.selectable_label(was_selected, *label).clicked() {
                        if !was_selected {
                            self.user_timezone = owned;
                            self.logger.info(format!(
                                "Clock timezone → {}",
                                tz.unwrap_or("browser local"),
                            ));
                        }
                    }
                }
            });

        ui.add_space(6.0);
        ui.label(
            egui::RichText::new(
                "Times rendered via the browser's Intl.DateTimeFormat \
                 with hour12 = false, en-CA locale.",
            )
            .small()
            .italics()
            .color(egui::Color32::from_rgb(140, 150, 170)),
        );
    }

}

impl eframe::App for WebApp {
    fn update(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        self.drain_pending();

        // Stash the WebGL2 context for the 3D tab. `render_*_tab` only
        // receives `&mut Ui`, so we capture the gl handle here (cheap Arc
        // clone) where `frame` is in scope.
        self.gl = frame.gl().cloned();

        // ── Top bar ─────────────────────────────────────────────────
        egui::TopBottomPanel::top("top_bar")
            .exact_height(36.0)
            .show(ctx, |ui| {
                ui.horizontal_centered(|ui| {
                    // Brand label — display-only. The About modal is
                    // opened via the dedicated ℹ About button in the
                    // right-side cluster, so the brand stays a label
                    // and avoids the dual-affordance UX problem.
                    ui.label(
                        egui::RichText::new("CopperForge")
                            .strong()
                            .size(18.0)
                            .color(egui::Color32::from_rgb(184, 115, 51)),
                    );
                    ui.label(
                        egui::RichText::new("· Browser Demo")
                            .small()
                            .color(egui::Color32::from_rgb(140, 150, 170)),
                    );
                    ui.separator();
                    let btn_label = if self.loading {
                        "⏳ Loading…"
                    } else {
                        "📦 Upload"
                    };
                    if ui
                        .add_enabled(!self.loading, egui::Button::new(btn_label))
                        .on_hover_text("Upload a release ZIP (gerbers + drill files)")
                        .clicked()
                    {
                        self.pick_release_zip(ctx);
                    }
                    // Example release — bundled as raw bytes so first-time
                    // visitors see a real board without having to find +
                    // upload their own gerbers. Same code path as a
                    // successful upload from there on.
                    if ui
                        .add_enabled(!self.loading, egui::Button::new("📂 Load Example"))
                        .on_hover_text(
                            "Load a bundled example release \
                             (CPArti FPGA dev board, 4-layer).",
                        )
                        .clicked()
                    {
                        self.load_example_release();
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
                            .button("🏭 PCBWay")
                            .on_hover_text(
                                "Release zip + PCBWAY_FAB_SPECS.md \
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

                    // ── Right-aligned cluster ───────────────────────
                    // Right-to-left layout: first call lands RIGHTMOST.
                    // Order on screen (left → right):
                    //   [file status]  ℹ About  ·  HH:MM:SS  YYYY-MM-DD
                    // Sized at 14pt (egui default is ~12) so the
                    // status / clock / button match the visual weight
                    // of the left-side toolbar.
                    ui.with_layout(
                        egui::Layout::right_to_left(egui::Align::Center),
                        |ui| {
                            let clock_text =
                                browser_local_clock(self.user_timezone.as_deref());
                            let tz_label = self
                                .user_timezone
                                .as_deref()
                                .unwrap_or("browser local");
                            ui.label(
                                egui::RichText::new(clock_text)
                                    .monospace()
                                    .size(14.0)
                                    .color(egui::Color32::from_rgb(200, 220, 240)),
                            )
                            .on_hover_text(format!(
                                "Timezone: {}\nChange in the Settings tab.",
                                tz_label
                            ));
                            ui.separator();
                            // User Guide — links out to the deployed
                            // copperforge-web docs site (Astro), hosted on
                            // Cloudflare Pages. The app itself lives at the
                            // apex (copperforge.dev), so the guide gets its
                            // own subdomain. One-line const in case it moves.
                            const USER_GUIDE_URL: &str = "https://docs.copperforge.dev";
                            if ui
                                .add(
                                    egui::Button::new(
                                        egui::RichText::new("📖 Guide")
                                            .size(14.0)
                                            .color(egui::Color32::from_rgb(180, 200, 220)),
                                    ),
                                )
                                .on_hover_text(format!(
                                    "Open the User Guide ({USER_GUIDE_URL})",
                                ))
                                .clicked()
                            {
                                ctx.open_url(egui::OpenUrl::new_tab(USER_GUIDE_URL));
                            }
                            ui.separator();
                            if ui
                                .add(
                                    egui::Button::new(
                                        egui::RichText::new("ℹ About")
                                            .size(14.0)
                                            .color(egui::Color32::from_rgb(184, 115, 51)),
                                    ),
                                )
                                .on_hover_text("About CopperForge")
                                .clicked()
                            {
                                self.about_open = true;
                            }
                            if let Some(ref loaded) = self.loaded {
                                ui.separator();
                                // One compact label (was two — the bold
                                // full filename overflowed into the clock).
                                // Truncate the name so the right cluster
                                // can't overrun the left toolbar; the full
                                // name + the same stats are in the Logger
                                // line and this hover.
                                let name = &loaded.source_name;
                                let short = if name.chars().count() > 20 {
                                    let head: String =
                                        name.chars().take(19).collect();
                                    format!("{head}…")
                                } else {
                                    name.clone()
                                };
                                ui.label(
                                    egui::RichText::new(format!(
                                        "{}  ·  {}g · {}drl · {:.0} KB",
                                        short,
                                        loaded.gerber_count(),
                                        loaded.drill_count(),
                                        loaded.total_bytes() as f64 / 1024.0,
                                    ))
                                    .size(14.0)
                                    .color(egui::Color32::from_rgb(180, 200, 220)),
                                )
                                .on_hover_text(format!(
                                    "{} — {} gerber · {} drill · {:.1} KB",
                                    loaded.source_name,
                                    loaded.gerber_count(),
                                    loaded.drill_count(),
                                    loaded.total_bytes() as f64 / 1024.0,
                                ));
                            }
                        },
                    );
                });
            });

        // Keep the clock ticking without external input. egui only
        // repaints on input or explicit requests, so without this the
        // clock would freeze between user actions. One repaint per
        // second is enough for HH:MM:SS precision.
        ctx.request_repaint_after(std::time::Duration::from_secs(1));

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
        // Movable (no anchor), softer palette than the previous pass:
        // background is a neutral dark-warm grey, copper accent reads
        // as an accent rather than dominating, body text is muted-warm
        // off-white. Title bar drag works as usual on egui::Window —
        // no special handling needed.
        if self.about_open {
            // Softer than the first cut: less saturated copper for body
            // accents, plain Frame::window stroke so the modal feels
            // like a window not a poster.
            let bg = egui::Color32::from_rgb(34, 30, 28);
            let copper_strong = egui::Color32::from_rgb(199, 123, 60); // matches mark fill
            let copper_soft = egui::Color32::from_rgb(170, 140, 110);
            let body = egui::Color32::from_rgb(210, 205, 195);
            let muted = egui::Color32::from_rgb(150, 145, 135);
            let frame = egui::Frame::window(&ctx.style())
                .fill(bg)
                .stroke(egui::Stroke::new(1.0, egui::Color32::from_rgb(70, 60, 50)))
                .corner_radius(8.0)
                .inner_margin(egui::Margin::same(18));
            egui::Window::new(
                egui::RichText::new("About CopperForge").color(copper_soft),
            )
            .open(&mut self.about_open)
            .collapsible(false)
            .resizable(true)
            .default_size(egui::vec2(520.0, 360.0))
            // No `.anchor(...)` — the window is fully draggable. First
            // open lands a little inset from the top-right so it
            // doesn't cover the centre canvas immediately.
            .default_pos(egui::pos2(120.0, 80.0))
            .frame(frame)
            .show(ctx, |ui| {
                // Hero banner across the top of the modal — embedded
                // via `include_bytes!`, decoded by egui_extras's
                // image loader registered in main.rs. The 800×535
                // palette-quantized PNG is ~195 KB; smaller than the
                // 1.6 MB original while still crisp at the modal's
                // typical render size.
                let hero_bytes = include_bytes!(
                    "../../../assets/media/copperforge-hero-800.png"
                );
                ui.add(
                    egui::Image::from_bytes(
                        "bytes://copperforge-hero-800.png",
                        hero_bytes.as_slice(),
                    )
                    .corner_radius(6.0)
                    .max_width(ui.available_width()),
                );
                ui.add_space(8.0);
                ui.heading(
                    egui::RichText::new("CopperForge")
                        .size(28.0)
                        .color(copper_strong),
                );
                ui.label(
                    egui::RichText::new("PCB & CAM companion for KiCad")
                        .italics()
                        .color(muted),
                );
                ui.add_space(8.0);
                ui.separator();
                ui.add_space(8.0);
                ui.label(
                    egui::RichText::new(
                        "PCB release viewer running entirely client-side — \
                         no upload, no server. Loads CopperForge release \
                         zips (gerbers, drill, BOM, centroid) and \
                         re-exports them for fab, including a \
                         PCBWay-target variant with fab-specs sheet.",
                    )
                    .color(body),
                );
                ui.add_space(6.0);
                ui.label(
                    egui::RichText::new(
                        "Click Upload Release ZIP and pick a file, or \
                         Load Example for a bundled 4-layer FPGA dev \
                         board. Right-mouse drag pans, wheel zooms, \
                         left-drag rubber-bands a region.",
                    )
                    .color(body),
                );
                ui.add_space(12.0);
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new("Source").color(muted));
                    ui.add_space(8.0);
                    ui.hyperlink_to(
                        egui::RichText::new("github.com/Atlantix-EDA/CopperForge")
                            .color(copper_soft),
                        "https://github.com/Atlantix-EDA/CopperForge",
                    );
                });
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new("Version").color(muted));
                    ui.add_space(8.0);
                    ui.monospace(
                        egui::RichText::new(env!("CARGO_PKG_VERSION")).color(body),
                    );
                });
            });
        }

        // ── Central panel: egui_dock area with Canvas / Layers /
        //    Board / Logger tabs. All four are draggable, splittable,
        //    and closable. Initial layout in `default_dock_layout()`.
        egui::CentralPanel::default().show(ctx, |ui| {
            // Clone the layout so the TabViewer can hold `&mut self`
            // exclusively for the duration of the dock render — same
            // pattern zicad/src/main.rs uses. Cheap (it's a small
            // tree of indices), and writes back at the end so user
            // drags/splits persist across frames.
            let mut dock_state = self.dock_state.clone();
            {
                let style = egui_dock::Style::from_egui(ctx.style().as_ref());
                let mut viewer = TabViewer { app: self };
                DockArea::new(&mut dock_state)
                    .style(style)
                    .show_inside(ui, &mut viewer);
            }
            self.dock_state = dock_state;
        });
        // Cursor hint moved into `render_canvas_tab`; the dock area
        // hosts the canvas now.
    }
}

// ── Clock with optional timezone override ───────────────────────────────

/// `HH:MM:SS  YYYY-MM-DD` formatted via the browser's
/// `Intl.DateTimeFormat`. `tz = None` means browser-local; otherwise
/// pass any IANA name (e.g. `"Europe/Berlin"`). Falls back to direct
/// `Date` field reads if the Intl call fails for any reason (very
/// old browser or unknown timezone string).
fn browser_local_clock(tz: Option<&str>) -> String {
    use wasm_bindgen::JsValue;
    let date = js_sys::Date::new_0();
    // Build the options object: { hour12: false, year, month, day,
    // hour, minute, second [, timeZone] }. en-CA gives ISO-style
    // YYYY-MM-DD, HH:MM:SS which is closest to what we want without
    // post-processing.
    let opts = js_sys::Object::new();
    let two_digit = JsValue::from_str("2-digit");
    let numeric = JsValue::from_str("numeric");
    let set = |key: &str, val: &JsValue| -> bool {
        js_sys::Reflect::set(&opts, &JsValue::from_str(key), val).is_ok()
    };
    set("hour12", &JsValue::from_bool(false));
    set("year", &numeric);
    set("month", &two_digit);
    set("day", &two_digit);
    set("hour", &two_digit);
    set("minute", &two_digit);
    set("second", &two_digit);
    if let Some(tz) = tz {
        set("timeZone", &JsValue::from_str(tz));
    }
    // `Date.prototype.toLocaleString(locales, options)` — locales
    // accepts a string or array; we pass the en-CA tag wrapped in
    // an Array.
    let locales = js_sys::Array::new();
    locales.push(&JsValue::from_str("en-CA"));
    let formatted = date.to_locale_string("en-CA", &opts);
    let raw: String = formatted.into();
    // en-CA emits "YYYY-MM-DD, HH:MM:SS"; reorder to
    // "HH:MM:SS  YYYY-MM-DD" to match the previous look.
    match raw.split_once(", ") {
        Some((date_part, time_part)) => format!("{}  {}", time_part, date_part),
        None => raw,
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
