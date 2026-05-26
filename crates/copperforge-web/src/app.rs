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
use gerber_viewer::ViewState;
use nalgebra::Point2;

use crate::canvas::model::LayerSide;
use crate::canvas::{paint as paint_canvas, GerberScene};

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
        }
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
                self.scene = Some(scene);
                self.view_initialized = false;
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

        // ── Ruler tool: left-click places points ────────────────────
        if self.ruler_active && response.clicked_by(egui::PointerButton::Primary) {
            if let Some(pos) = response.interact_pointer_pos() {
                let gerber_pos = self.view_state.screen_to_gerber_coords(pos);
                match (self.ruler_start, self.ruler_end) {
                    (None, _) => {
                        // First click — set start.
                        self.ruler_start = Some(gerber_pos);
                        self.ruler_end = None;
                    }
                    (Some(_), None) => {
                        // Second click — set end (measurement complete).
                        self.ruler_end = Some(gerber_pos);
                    }
                    (Some(_), Some(_)) => {
                        // Third click — start a new measurement.
                        self.ruler_start = Some(gerber_pos);
                        self.ruler_end = None;
                    }
                }
            }
        }

        // ESC cancels the ruler tool.
        if self.ruler_active && ui.input(|i| i.key_pressed(egui::Key::Escape)) {
            self.ruler_active = false;
            self.ruler_start = None;
            self.ruler_end = None;
        }
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
                    ui.label(
                        egui::RichText::new("CopperForge")
                            .strong()
                            .size(16.0)
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
                        // Grid on/off + spacing slider live in the top bar
                        // so they're discoverable without opening anything.
                        ui.checkbox(&mut self.grid_settings.enabled, "Grid");
                        ui.add_enabled_ui(self.grid_settings.enabled, |ui| {
                            ui.add(
                                egui::DragValue::new(&mut self.grid_settings.spacing_mm)
                                    .speed(0.1)
                                    .range(0.1..=50.0)
                                    .suffix(" mm"),
                            );
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
                                    // Compute the label + color via
                                    // immutable read first, then take a
                                    // disjoint mutable borrow for the
                                    // checkbox bool — borrowck doesn't
                                    // see through field-level disjoint
                                    // access inside a method call.
                                    let label = scene.layers[i].display_label();
                                    let color = scene.layers[i].color;
                                    let visible = &mut scene.layers[i].visible;
                                    ui.checkbox(
                                        visible,
                                        egui::RichText::new(label).color(color),
                                    );
                                }
                            }
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

            self.handle_canvas_input(ui, &response);

            // Clip the painter to the canvas rect so nothing bleeds
            // into the side panel during pan/zoom.
            let painter = ui.painter_at(rect);

            // Grid first (under everything else).
            draw_grid(&painter, &rect, &self.view_state, &self.grid_settings);

            // Gerber layers next.
            if let Some(ref scene) = self.scene {
                paint_canvas(&painter, scene, self.view_state);
            }

            // Ruler overlay last so the line + label sit on top.
            self.paint_ruler(&painter, &response);

            // Crosshair cursor while ruler is armed.
            if self.ruler_active {
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
    for i in 0..archive.len() {
        let mut entry = archive
            .by_index(i)
            .map_err(|e| format!("Reading entry {}: {}", i, e))?;
        if !entry.is_file() {
            continue;
        }
        let name = entry.name().to_string();
        let lower = name.to_lowercase();
        if !(lower.ends_with(".gbr") || lower.ends_with(".drl")) {
            continue;
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

    if entries.is_empty() {
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
