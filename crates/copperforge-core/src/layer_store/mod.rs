//! PCB layer management — plain structs, no ECS.
//!
//! A PCB has at most ~20 layers. This module stores them in a `Vec<PcbLayer>`
//! with simple methods for lookup, visibility, rendering, and gerber assignment.

pub mod types;
pub mod detection;
mod layer;
pub mod units;

pub use types::*;
pub use detection::*;
pub use units::*;

use std::collections::HashMap;
use std::path::Path;

use gerber_viewer::{BoundingBox, GerberLayer, GerberImageTransform, RenderConfiguration, GerberRenderer, GerberTransform};
use egui::{Color32, Painter};

use crate::display::{DisplayManager, VectorOffset, manager::MirroringSettings};

/// Everything about one PCB layer in one place.
#[derive(Clone)]
pub struct PcbLayer {
    pub layer_type: LayerType,
    pub name: String,
    pub file_path: Option<std::path::PathBuf>,
    pub gerber: GerberLayer,
    pub image_transform: GerberImageTransform,
    pub visible: bool,
    pub color: Color32,
    pub z_order: i32,
}

/// The store. Holds all layers plus supporting state.
pub struct LayerStore {
    pub layers: Vec<PcbLayer>,
    pub active_layer: LayerType,
    pub assignments: HashMap<String, LayerType>,
    pub unassigned: Vec<UnassignedGerber>,
    pub detector: LayerDetector,
    pub coordinates_dirty: bool,
    pub zoom: ZoomState,
    pub units: UnitsState,
}

/// Zoom/view tracking (was ZoomResource).
#[derive(Clone, Debug)]
pub struct ZoomState {
    pub scale: f32,
    pub center_x: f32,
    pub center_y: f32,
    pub min_scale: f32,
    pub max_scale: f32,
    pub fit_to_view_scale: f32,
}

impl Default for ZoomState {
    fn default() -> Self {
        Self { scale: 1.0, center_x: 0.0, center_y: 0.0, min_scale: 0.001, max_scale: 1000.0, fit_to_view_scale: 1.0 }
    }
}

impl ZoomState {
    pub fn set_scale(&mut self, scale: f32) { self.scale = scale.clamp(self.min_scale, self.max_scale); }
    pub fn set_fit_to_view_scale(&mut self, scale: f32) { self.fit_to_view_scale = scale.clamp(self.min_scale, self.max_scale); }
    pub fn zoom_percentage(&self) -> f32 { (self.scale / self.fit_to_view_scale) * 100.0 }
}

/// Display unit tracking (was UnitsResource).
#[derive(Clone, Debug)]
pub struct UnitsState {
    pub display_unit: DisplayUnit,
}

impl Default for UnitsState {
    fn default() -> Self { Self { display_unit: DisplayUnit::Millimeters } }
}

impl UnitsState {
    pub fn is_mils(&self) -> bool { self.display_unit == DisplayUnit::Mils }
    pub fn is_mm(&self) -> bool { self.display_unit == DisplayUnit::Millimeters }
    pub fn toggle(&mut self) {
        self.display_unit = match self.display_unit {
            DisplayUnit::Mils => DisplayUnit::Millimeters,
            _ => DisplayUnit::Mils,
        };
    }
    pub fn unit_suffix(&self) -> &str {
        match self.display_unit {
            DisplayUnit::Millimeters => "mm",
            DisplayUnit::Mils => "mils",
            _ => "mm",
        }
    }
}

// ── Construction ─────────────────────────────────────────────────────────

impl Default for LayerStore {
    fn default() -> Self {
        Self {
            layers: Vec::new(),
            active_layer: LayerType::Copper(1),
            assignments: HashMap::new(),
            unassigned: Vec::new(),
            detector: LayerDetector::new(),
            coordinates_dirty: false,
            zoom: ZoomState::default(),
            units: UnitsState::default(),
        }
    }
}

// ── Layer queries ────────────────────────────────────────────────────────

impl LayerStore {
    pub fn layer_count(&self) -> usize { self.layers.len() }

    pub fn find(&self, layer_type: LayerType) -> Option<&PcbLayer> {
        self.layers.iter().find(|l| l.layer_type == layer_type)
    }

    pub fn find_mut(&mut self, layer_type: LayerType) -> Option<&mut PcbLayer> {
        self.layers.iter_mut().find(|l| l.layer_type == layer_type)
    }

    pub fn visible_layers(&self) -> impl Iterator<Item = &PcbLayer> {
        self.layers.iter().filter(|l| l.visible)
    }

    pub fn set_visibility(&mut self, layer_type: LayerType, visible: bool) {
        if let Some(layer) = self.find_mut(layer_type) { layer.visible = visible; }
    }

    pub fn get_visibility(&self, layer_type: LayerType) -> bool {
        self.find(layer_type).map_or(false, |l| l.visible)
    }

    pub fn set_color(&mut self, layer_type: LayerType, color: Color32) -> bool {
        if let Some(layer) = self.find_mut(layer_type) { layer.color = color; true } else { false }
    }

    pub fn combined_bounding_box(&self) -> Option<BoundingBox> {
        let mut combined: Option<BoundingBox> = None;
        for layer in self.visible_layers() {
            let bbox = layer.gerber.bounding_box();
            combined = Some(match combined {
                Some(mut existing) => { existing.expand(bbox); existing }
                None => bbox.clone(),
            });
        }
        combined
    }
}

// ── Layer creation ───────────────────────────────────────────────────────

impl LayerStore {
    pub fn add_layer(
        &mut self,
        layer_type: LayerType,
        gerber: GerberLayer,
        file_path: Option<std::path::PathBuf>,
        visible: bool,
    ) {
        let layer = PcbLayer {
            layer_type,
            name: layer_type.display_name(),
            file_path,
            image_transform: gerber.image_transform().clone(),
            gerber,
            visible,
            color: layer_type.color(),
            z_order: z_order_for(layer_type),
        };
        self.layers.push(layer);
    }

    pub fn clear_all(&mut self) {
        self.layers.clear();
        self.unassigned.clear();
        self.assignments.clear();
    }
}

// ── Gerber assignment ────────────────────────────────────────────────────

impl LayerStore {
    pub fn has_unassigned(&self) -> bool { !self.unassigned.is_empty() }

    pub fn add_assignment(&mut self, filename: String, layer_type: LayerType) {
        self.assignments.insert(filename, layer_type);
    }

    pub fn get_assignment(&self, filename: &str) -> Option<LayerType> {
        self.assignments.get(filename).copied()
    }

    pub fn detect_layer_type(&self, filename: &str) -> Option<LayerType> {
        self.detector.detect_layer_type(filename)
    }

    /// Assign an unassigned gerber to a layer type.
    pub fn assign_gerber(&mut self, filename: &str, layer_type: LayerType) -> Result<(), String> {
        let idx = self.unassigned.iter().position(|u| u.filename == filename)
            .ok_or_else(|| format!("Unassigned gerber '{}' not found", filename))?;

        if self.find(layer_type).is_some() {
            return Err(format!("Layer type {:?} is already assigned", layer_type));
        }

        let ug = self.unassigned.remove(idx);
        self.add_layer(layer_type, ug.parsed_layer, Some(filename.into()), true);
        self.add_assignment(filename.to_string(), layer_type);
        Ok(())
    }

    /// Auto-detect and assign all unassigned gerbers that can be identified.
    pub fn auto_assign(&mut self) -> Vec<(String, LayerType)> {
        let mut assigned = Vec::new();
        let candidates: Vec<(String, LayerType)> = self.unassigned.iter()
            .filter_map(|ug| {
                let detected = self.detector.detect_layer_type(&ug.filename)?;
                if self.find(detected).is_none() { Some((ug.filename.clone(), detected)) } else { None }
            })
            .collect();

        for (filename, layer_type) in candidates {
            if self.assign_gerber(&filename, layer_type).is_ok() {
                assigned.push((filename, layer_type));
            }
        }
        assigned
    }

    /// Load all .gbr files from a directory, auto-detect where possible.
    pub fn load_from_directory(&mut self, dir: &Path) -> Result<(usize, usize), String> {
        use std::io::BufReader;
        use gerber_viewer::gerber_parser::parse;

        let entries = std::fs::read_dir(dir)
            .map_err(|e| format!("Failed to read directory: {}", e))?;

        let mut loaded = 0usize;
        let mut unassigned_count = 0usize;

        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|s| s.to_str()) != Some("gbr") { continue; }

            let filename = path.file_name().and_then(|n| n.to_str()).unwrap_or("").to_string();
            let content = match std::fs::read_to_string(&path) { Ok(c) => c, Err(_) => continue };
            let doc = match parse(BufReader::new(content.as_bytes())) { Ok(d) => d, Err(_) => continue };
            let gerber_layer = GerberLayer::new(doc.into_commands());

            if let Some(detected) = self.detector.detect_layer_type(&filename) {
                if self.find(detected).is_none() && !self.assignments.values().any(|t| *t == detected) {
                    self.add_layer(detected, gerber_layer, Some(path.clone()), true);
                    self.add_assignment(filename, detected);
                    loaded += 1;
                    continue;
                }
            }

            self.unassigned.push(UnassignedGerber { filename, content, parsed_layer: gerber_layer });
            unassigned_count += 1;
        }

        Ok((loaded, unassigned_count))
    }
}

// ── Coordinate / transform updates ──────────────────────────────────────

impl LayerStore {
    pub fn mark_dirty(&mut self) { self.coordinates_dirty = true; }
    pub fn is_dirty(&self) -> bool { self.coordinates_dirty }
    pub fn mark_clean(&mut self) { self.coordinates_dirty = false; }

    /// Apply display settings (mirroring, rotation, quadrant offsets) before rendering.
    pub fn update_transforms(&mut self, display_manager: &DisplayManager, rotation_degrees: f32) {
        let pcb_center = self.combined_bounding_box()
            .map(|bbox| bbox.center())
            .unwrap_or_else(|| nalgebra::Point2::new(0.0, 0.0));

        // Nothing to store per-layer — transforms are computed at render time.
        // We just cache the PCB center for the render pass.
        let _ = (pcb_center, display_manager, rotation_degrees);
        self.mark_clean();
    }
}

// ── Rendering ────────────────────────────────────────────────────────────

impl LayerStore {
    /// Render all visible layers. This is the main entry point.
    pub fn render(
        &self,
        painter: &Painter,
        view_state: gerber_viewer::ViewState,
        display_manager: &DisplayManager,
        rotation_degrees: f32,
    ) {
        let config = RenderConfiguration::default();

        let pcb_center = self.combined_bounding_box()
            .map(|bbox| bbox.center())
            .unwrap_or_else(|| nalgebra::Point2::new(0.0, 0.0));

        let mechanical_outline = if display_manager.quadrant_view_enabled {
            self.find(LayerType::MechanicalOutline)
        } else {
            None
        };

        let mut sorted: Vec<&PcbLayer> = self.layers.iter().filter(|l| l.visible).collect();
        sorted.sort_by_key(|l| l.z_order);

        for layer in &sorted {
            // Skip outline and paste in quadrant view (outline rendered per-layer below)
            if display_manager.quadrant_view_enabled {
                if layer.layer_type == LayerType::MechanicalOutline { continue; }
                if matches!(layer.layer_type, LayerType::Paste(_)) { continue; }
            }

            let offset = if display_manager.quadrant_view_enabled {
                quadrant_offset_for(layer.layer_type, display_manager.quadrant_offset_magnitude.max(1.0))
            } else {
                VectorOffset { x: 0.0, y: 0.0 }
            };

            let transform = build_transform(&layer.image_transform, &display_manager.mirroring, rotation_degrees, offset.clone(), pcb_center);
            let renderer = GerberRenderer::new(&config, view_state, &transform, &layer.gerber);
            renderer.paint_layer(painter, layer.color);

            // Overlay mechanical outline in quadrant view
            if display_manager.quadrant_view_enabled {
                if let Some(mech) = mechanical_outline {
                    let mech_transform = build_transform(&mech.image_transform, &display_manager.mirroring, rotation_degrees, offset, pcb_center);
                    let mech_renderer = GerberRenderer::new(&config, view_state, &mech_transform, &mech.gerber);
                    mech_renderer.paint_layer(painter, mech.color);
                }
            }
        }
    }
}

// ── Helpers ──────────────────────────────────────────────────────────────

fn z_order_for(layer_type: LayerType) -> i32 {
    match layer_type {
        // Drill on top so holes punch through everything else visually.
        LayerType::Drill => 100,
        LayerType::ViaPlugging(Side::Top) => 95,
        LayerType::Paste(Side::Top) => 90,
        LayerType::Silkscreen(Side::Top) => 80,
        LayerType::Soldermask(Side::Top) => 70,
        LayerType::Copper(1) => 60,
        LayerType::Copper(n) => 50 - (n as i32),
        LayerType::Soldermask(Side::Bottom) => 40,
        LayerType::Silkscreen(Side::Bottom) => 30,
        LayerType::Paste(Side::Bottom) => 20,
        LayerType::ViaPlugging(Side::Bottom) => 15,
        LayerType::MechanicalOutline => 10,
    }
}

fn quadrant_offset_for(layer_type: LayerType, spacing: f64) -> VectorOffset {
    let x = match layer_type {
        LayerType::Copper(_) => 0.0,
        LayerType::Silkscreen(_) => spacing,
        LayerType::Soldermask(_) => spacing * 2.0,
        LayerType::Paste(_) => -9999.0,
        LayerType::MechanicalOutline => 0.0,
        // Drill + plugging stay co-located with the board in quadrant view.
        LayerType::Drill => 0.0,
        LayerType::ViaPlugging(_) => -9999.0,
    };
    VectorOffset { x, y: 0.0 }
}

fn build_transform(
    image_tf: &GerberImageTransform,
    mirroring: &MirroringSettings,
    rotation_degrees: f32,
    offset: VectorOffset,
    pcb_center: nalgebra::Point2<f64>,
) -> GerberTransform {
    let render = GerberTransform {
        rotation: rotation_degrees.to_radians(),
        mirroring: mirroring.clone().into(),
        origin: VectorOffset { x: pcb_center.x, y: pcb_center.y }.into(),
        offset: offset.into(),
        scale: 1.0,
    };
    let composed = image_tf.to_matrix() * render.to_matrix();
    GerberTransform::from_matrix(&composed)
}
