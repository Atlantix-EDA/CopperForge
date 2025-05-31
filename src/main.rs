use std::io::BufReader;
use std::{fs, path::PathBuf};

use eframe::emath::{Rect, Vec2};
use eframe::epaint::Color32;
use egui::ViewportBuilder;
use egui_dock::{DockArea, DockState, NodeIndex, Style, SurfaceIndex};
use serde::{Serialize, Deserialize};

/// egui_lens imports
use egui_lens::{ReactiveEventLogger, ReactiveEventLoggerState, LogColors};

/// Use of prelude for egui_mobius_reactive
use egui_mobius_reactive::Dynamic;  
use std::collections::HashMap;

use gerber_viewer::gerber_parser::parse;
use gerber_viewer::{
    draw_arrow, draw_outline, draw_crosshair, BoundingBox, GerberLayer, GerberRenderer, 
    ViewState, Mirroring, draw_marker, UiState
};
use egui::{Painter, Pos2, Stroke};
use gerber_viewer::position::Vector;

// Import platform modules
mod platform;
use platform::{banner, details};

// Import new modules
mod constants;
mod layers;
mod grid;
mod ui;
mod drc;

use constants::*;
use layers::{LayerType, LayerInfo};
use grid::GridSettings;
use drc::DrcSimple;

/// Simple DRC Rules structure
#[derive(Debug, Clone)]
pub struct DrcRules {
    pub min_trace_width: f32,      // mm
    pub min_via_diameter: f32,     // mm  
    pub min_drill_diameter: f32,   // mm
    pub min_spacing: f32,          // mm
    pub min_annular_ring: f32,     // mm
    pub use_mils: bool,            // true = display in mils, false = mm
}

impl Default for DrcRules {
    fn default() -> Self {
        Self {
            min_trace_width: 0.15,    // 0.15mm = ~6 mil
            min_via_diameter: 0.3,    // 0.3mm = ~12 mil
            min_drill_diameter: 0.2,  // 0.2mm = ~8 mil
            min_spacing: 0.15,        // 0.15mm = ~6 mil
            min_annular_ring: 0.1,    // 0.1mm = ~4 mil
            use_mils: false,          // Default to mm
        }
    }
}

impl DrcRules {
    /// Convert mm to mils (1 mm = 39.3701 mils)
    fn mm_to_mils(mm: f32) -> f32 {
        mm * 39.3701
    }
    
    /// Convert mils to mm (1 mil = 0.0254 mm)
    fn mils_to_mm(mils: f32) -> f32 {
        mils * 0.0254
    }
    
    /// Get display value (convert to mils if use_mils is true)
    fn get_display_value(&self, mm_value: f32) -> f32 {
        if self.use_mils {
            Self::mm_to_mils(mm_value)
        } else {
            mm_value
        }
    }
    
    /// Set value from display (convert from mils if use_mils is true)
    fn set_from_display(&self, display_value: f32) -> f32 {
        if self.use_mils {
            Self::mils_to_mm(display_value)
        } else {
            display_value
        }
    }
    
    /// Get unit suffix
    fn unit_suffix(&self) -> &str {
        if self.use_mils { " mils" } else { " mm" }
    }
}

/// DRC violation result
#[derive(Debug, Clone)]
pub struct DrcViolation {
    pub rule_name: String,
    pub description: String,
    pub layer: String,
    pub measured_value: f32,  // mm
    pub required_value: f32,  // mm
    pub x: f32,              // mm
    pub y: f32,              // mm
}

impl DrcViolation {
    pub fn format_message(&self) -> String {
        format!("{}: {} on {} - measured {:.3}mm, required {:.3}mm at ({:.1}, {:.1})",
            self.rule_name,
            self.description,
            self.layer,
            self.measured_value,
            self.required_value,
            self.x,
            self.y
        )
    }
}

/// Simple DRC checker with OpenCV integration
pub fn run_simple_drc_check(app: &mut DemoLensApp) -> Vec<DrcViolation> {
    let mut violations = Vec::new();
    
    // Clear previous quality issues
    app.trace_quality_issues.clear();
    
    // Get PCB boundary from mechanical outline layer
    let pcb_boundary = if let Some(outline_info) = app.layers.get(&LayerType::MechanicalOutline) {
        outline_info.gerber_layer.as_ref().map(|layer| layer.bounding_box())
    } else {
        None
    };
    
    if pcb_boundary.is_none() {
        println!("Warning: No mechanical outline found - cannot determine PCB boundary for DRC");
        return violations;
    }
    
    let boundary = pcb_boundary.unwrap();
    println!("DRC boundary check: PCB area is {:.1} x {:.1} mm", boundary.width(), boundary.height());
    
    // Check each copper layer for trace width violations
    for (layer_type, layer_info) in &app.layers {
        // Only check copper layers
        if !matches!(layer_type, LayerType::TopCopper | LayerType::BottomCopper) {
            continue;
        }
        
        // Use primitive-based DRC analysis
        if let Some(gerber_layer) = &layer_info.gerber_layer {
            println!("Running primitive-based trace detection on {}", layer_type.display_name());
            
            let drc = DrcSimple {
                min_trace_width: app.drc_rules.min_trace_width,
                lines_only: true,  // Only check Line primitives to avoid copper pour false positives
                min_trace_length: 1.0,  // Only lines >= 1mm are considered traces (not pad connections)
                ..DrcSimple::default()
            };
            
            // Get mechanical outline bounds for filtering
            let pcb_bounds = app.layers.get(&LayerType::MechanicalOutline)
                .and_then(|outline| outline.gerber_layer.as_ref())
                .map(|layer| layer.bounding_box());
                
            let primitive_violations = drc.run_trace_width_drc_with_bounds(gerber_layer, pcb_bounds);
            
            // Also analyze trace quality (corners, jogs, etc.)
            let quality_issues = drc.analyze_trace_quality(gerber_layer);
            println!("Found {} trace quality issues on {}", quality_issues.len(), layer_type.display_name());
            
            // Log corner issues specifically
            for issue in &quality_issues {
                if matches!(issue.issue_type, drc::TraceQualityType::SharpCorner) {
                    println!("Corner issue at ({:.2}, {:.2}): {}", issue.location.0, issue.location.1, issue.description);
                }
            }
            
            // Store quality issues for this layer (extend the existing vector)
            app.trace_quality_issues.extend(quality_issues);
            
            if let Some(bounds) = &pcb_bounds {
                println!("PCB bounds: ({:.2}, {:.2}) to ({:.2}, {:.2})", 
                    bounds.min.x, bounds.min.y, bounds.max.x, bounds.max.y);
            }
            
            // Convert to DrcViolation format
            for (i, violation) in primitive_violations.iter().enumerate() {
                if i < 3 { // Debug first few violations
                    println!("Violation {}: trace at ({:.2}, {:.2}), width {:.3}mm", 
                        i, violation.trace.center_x, violation.trace.center_y, violation.trace.width);
                }
            }
            
            // Convert to DrcViolation format
            for violation in primitive_violations {
                violations.push(DrcViolation {
                    rule_name: "Primitive Trace Width".to_string(),
                    description: format!("Trace width {:.3}mm below minimum", violation.measured_width),
                    layer: layer_type.display_name().to_string(),
                    measured_value: violation.measured_width,
                    required_value: violation.required_width,
                    x: violation.trace.center_x,
                    y: violation.trace.center_y,
                });
            }
            
            println!("Primitive analysis found {} violations on {}", violations.len(), layer_type.display_name());
        } else if let Some(ref raw_gerber) = layer_info.raw_gerber_data {
            // Use gerber parsing if no layer object available
            let layer_violations = check_trace_width_in_gerber_data(
                raw_gerber,
                layer_type.display_name(),
                app.drc_rules.min_trace_width,
                &boundary
            );
            violations.extend(layer_violations);
        }
    }
    
    // Apply one-marker-per-trace clustering to reduce visual clutter
    let clustered_violations = cluster_violations_per_trace(violations.clone(), 10.0); // 10mm clustering radius for trace grouping
    println!("DRC clustering: {} violations reduced to {} markers (1 per trace)", violations.len(), clustered_violations.len());
    
    // Store clustered violations in app state for rendering
    app.drc_violations = clustered_violations.clone();
    
    violations
}


/// Check trace width by parsing raw Gerber data
fn check_trace_width_in_gerber_data(gerber_data: &str, layer_name: &str, min_width: f32, pcb_boundary: &BoundingBox) -> Vec<DrcViolation> {
    let mut violations = Vec::new();
    
    println!("Parsing Gerber data for layer: {}", layer_name);
    
    // Parse the raw Gerber file using gerber_viewer's gerber_parser
    let reader = BufReader::new(gerber_data.as_bytes());
    match parse(reader) {
        Ok(doc) => {
            println!("Successfully parsed Gerber file for {}", layer_name);
            
            // Convert to commands like in the working code
            let commands = doc.into_commands();
            println!("Found {} commands in {}", commands.len(), layer_name);
            
            // Now parse for real DRC violations
            println!("Analyzing {} commands for DRC violations in {}", commands.len(), layer_name);
            
            // Build a map of aperture codes to their diameters
            let mut aperture_map = std::collections::HashMap::new();
            let mut current_aperture: Option<i32> = None;
            
            for command in &commands {
                // Extract aperture definitions and classify them
                let command_str = format!("{:?}", command);
                if command_str.contains("ApertureDefinition") {
                    if let Some(code_start) = command_str.find("code: ") {
                        if let Some(code_end) = command_str[code_start + 6..].find(',') {
                            if let Ok(code) = command_str[code_start + 6..code_start + 6 + code_end].parse::<i32>() {
                                // Extract diameter from circle apertures
                                if let Some(diameter_start) = command_str.find("diameter: ") {
                                    if let Some(diameter_end) = command_str[diameter_start + 10..].find(',') {
                                        if let Ok(diameter) = command_str[diameter_start + 10..diameter_start + 10 + diameter_end].parse::<f32>() {
                                            // Only include small apertures likely to be traces (not pads/pours)
                                            if is_trace_aperture(diameter) {
                                                aperture_map.insert(code, diameter);
                                                println!("Found trace aperture {}: diameter {}mm", code, diameter);
                                            } else {
                                                println!("Ignored pad/pour aperture {}: diameter {}mm (too large)", code, diameter);
                                            }
                                        }
                                    }
                                }
                                // Extract width from rectangular apertures
                                if let Some(x_start) = command_str.find("x: ") {
                                    if let Some(x_end) = command_str[x_start + 3..].find(',') {
                                        if let Ok(width) = command_str[x_start + 3..x_start + 3 + x_end].parse::<f32>() {
                                            // Only include small rectangular apertures likely to be traces
                                            if is_trace_aperture(width) {
                                                aperture_map.insert(code, width);
                                                println!("Found trace aperture {}: width {}mm", code, width);
                                            } else {
                                                println!("Ignored pad/pour aperture {}: width {}mm (too large)", code, width);
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
                
                // Track aperture selections
                if command_str.contains("SelectAperture") {
                    if let Some(aperture_start) = command_str.find("SelectAperture(") {
                        if let Some(aperture_end) = command_str[aperture_start + 15..].find(')') {
                            if let Ok(aperture) = command_str[aperture_start + 15..aperture_start + 15 + aperture_end].parse::<i32>() {
                                current_aperture = Some(aperture);
                            }
                        }
                    }
                }
                
                // Check interpolate operations (drawing lines - equivalent to D01)
                // Ignore Flash operations (D03) which are used for pads/vias
                if command_str.contains("Interpolate") && !command_str.contains("Flash") {
                    if let Some(current_aperture_code) = current_aperture {
                        if let Some(&diameter) = aperture_map.get(&current_aperture_code) {
                            if diameter < min_width {
                                // Extract coordinates from command string
                                let (x, y) = extract_coordinates_from_command(&command_str);
                                let x_mm = x / 1_000_000.0; // Convert from nanometers to mm
                                let y_mm = y / 1_000_000.0;
                                
                                // Additional filtering: reject if coordinates suggest this is near a pad/component
                                if is_within_pcb_boundary(x_mm, y_mm, pcb_boundary) && is_likely_trace_location(x_mm, y_mm, diameter) {
                                    violations.push(DrcViolation {
                                        rule_name: "Minimum Trace Width".to_string(),
                                        description: format!("Trace width {:.3}mm below minimum", diameter),
                                        layer: layer_name.to_string(),
                                        measured_value: diameter,
                                        required_value: min_width,
                                        x: x_mm,
                                        y: y_mm,
                                    });
                                    println!("DRC violation on trace at ({:.2}, {:.2})mm: {:.3}mm trace", x_mm, y_mm, diameter);
                                } else {
                                    println!("Rejected violation (likely pad/component) at ({:.2}, {:.2})mm: {:.3}mm", x_mm, y_mm, diameter);
                                }
                            }
                        }
                    }
                }
            }
            
            println!("Found {} apertures and {} violations in {}", 
                     aperture_map.len(), violations.len(), layer_name);
        }
        Err(e) => {
            println!("Failed to parse Gerber data for {}: {:?}", layer_name, e);
        }
    }
    
    violations
}

/// Cluster violations to show only 1 marker per trace (aggressive clustering)
fn cluster_violations_per_trace(violations: Vec<DrcViolation>, cluster_radius_mm: f32) -> Vec<DrcViolation> {
    if violations.is_empty() {
        return violations;
    }
    
    let mut clustered = Vec::new();
    let mut used = vec![false; violations.len()];
    
    for i in 0..violations.len() {
        if used[i] {
            continue;
        }
        
        let center = &violations[i];
        let mut cluster_violations = vec![center.clone()];
        used[i] = true;
        
        // Find ALL violations within clustering radius (aggressive grouping)
        for j in (i + 1)..violations.len() {
            if used[j] {
                continue;
            }
            
            let distance = ((violations[j].x - center.x).powi(2) + (violations[j].y - center.y).powi(2)).sqrt();
            
            if distance <= cluster_radius_mm {
                cluster_violations.push(violations[j].clone());
                used[j] = true;
            }
        }
        
        // Always create one representative violation per trace cluster
        let mut sum_x = 0.0;
        let mut sum_y = 0.0;
        let mut worst_violation = &cluster_violations[0];
        
        for violation in &cluster_violations {
            sum_x += violation.x;
            sum_y += violation.y;
            
            // Find worst violation (smallest measured value = worst)
            if violation.measured_value < worst_violation.measured_value {
                worst_violation = violation;
            }
        }
        
        let centroid_x = sum_x / cluster_violations.len() as f32;
        let centroid_y = sum_y / cluster_violations.len() as f32;
        
        // Create single representative marker for this trace/area
        clustered.push(DrcViolation {
            rule_name: "Trace Width Violation".to_string(),
            description: if cluster_violations.len() == 1 {
                format!("Trace width {:.3}mm below minimum", worst_violation.measured_value)
            } else {
                format!("Trace area with {} violations (worst: {:.3}mm)", cluster_violations.len(), worst_violation.measured_value)
            },
            layer: worst_violation.layer.clone(),
            measured_value: worst_violation.measured_value,
            required_value: worst_violation.required_value,
            x: centroid_x,
            y: centroid_y,
        });
    }
    
    clustered
}

/// Determine if an aperture represents a trace vs pad/via/pour
fn is_trace_aperture(width_mm: f32) -> bool {
    // Be much more aggressive - only very narrow apertures are traces
    // Typical traces: 0.1mm - 0.5mm (4-20 mils)
    // Pads are typically > 0.8mm (30+ mils)
    width_mm < 0.8 && width_mm > 0.05 // Between 0.05mm and 0.8mm only
}

/// Additional check to see if this location is likely a trace vs pad
fn is_likely_trace_location(_x_mm: f32, _y_mm: f32, width_mm: f32) -> bool {
    // For now, just use very conservative width filtering
    // Real traces are typically < 0.5mm (20 mils)
    // Anything larger is likely a pad or connector feature
    width_mm < 0.5
}

/// Check if coordinates are within PCB boundary
fn is_within_pcb_boundary(x_mm: f32, y_mm: f32, boundary: &BoundingBox) -> bool {
    let x_f64 = x_mm as f64;
    let y_f64 = y_mm as f64;
    
    x_f64 >= boundary.min.x && x_f64 <= boundary.max.x &&
    y_f64 >= boundary.min.y && y_f64 <= boundary.max.y
}

/// Extract coordinates from Gerber command debug string
fn extract_coordinates_from_command(command_str: &str) -> (f32, f32) {
    let mut x = 0.0;
    let mut y = 0.0;
    
    // Look for x coordinate pattern: "x: Some(CoordinateNumber { nano: 12345 }"
    if let Some(x_start) = command_str.find("x: Some(CoordinateNumber { nano: ") {
        let x_offset = x_start + 33; // Length of "x: Some(CoordinateNumber { nano: "
        if let Some(x_end) = command_str[x_offset..].find(' ') {
            if let Ok(x_nano) = command_str[x_offset..x_offset + x_end].parse::<f32>() {
                x = x_nano;
            }
        }
    }
    
    // Look for y coordinate pattern: "y: Some(CoordinateNumber { nano: 12345 }"
    if let Some(y_start) = command_str.find("y: Some(CoordinateNumber { nano: ") {
        let y_offset = y_start + 33; // Length of "y: Some(CoordinateNumber { nano: "
        if let Some(y_end) = command_str[y_offset..].find(' ') {
            if let Ok(y_nano) = command_str[y_offset..y_offset + y_end].parse::<f32>() {
                y = y_nano;
            }
        }
    }
    
    (x, y)
}

/// Draw a red X marker for DRC violations
fn draw_violation_marker(painter: &Painter, center: Pos2, size: f32, color: Color32) {
    let half_size = size / 2.0;
    let stroke = Stroke::new(2.0, color);
    
    // Draw X shape
    painter.line_segment([
        Pos2::new(center.x - half_size, center.y - half_size),
        Pos2::new(center.x + half_size, center.y + half_size)
    ], stroke);
    
    painter.line_segment([
        Pos2::new(center.x - half_size, center.y + half_size),
        Pos2::new(center.x + half_size, center.y - half_size)
    ], stroke);
}

/// Extract diameter from aperture definition - TODO: Fix when we understand the API
fn get_aperture_diameter(_aperture: &str) -> Option<f32> {
    // TODO: This needs to be implemented once we understand the correct API
    None
}

/// Check trace width in a single gerber layer - updated to use raw parsing
fn check_trace_width_in_layer(gerber_layer: &GerberLayer, layer_name: &str, _min_width: f32, _pcb_boundary: &BoundingBox) -> Vec<DrcViolation> {
    // For now, we need to get the raw gerber data to parse it
    // This is a placeholder until we can access the original gerber strings
    let violations = Vec::new();
    
    let bbox = gerber_layer.bounding_box();
    println!("Layer {} has bbox: {:.3}x{:.3}mm", layer_name, bbox.width(), bbox.height());
    
    // TODO: We need access to the original gerber file content to parse it
    // The GerberLayer has already processed the data, so we need to either:
    // 1. Store the raw gerber data alongside the GerberLayer
    // 2. Parse the gerber files directly before creating GerberLayer
    
    violations
}

/// Define the tabs for the DockArea
#[derive(Clone, Serialize, Deserialize)]
enum TabKind {
    ViewSettings,
    DRC,
    GerberView,
    EventLog,
}

pub struct TabParams<'a> {
    pub app: &'a mut DemoLensApp,

}

/// Tab container struct for DockArea
#[derive(Clone, Serialize, Deserialize)]
struct Tab {
    kind: TabKind,
    #[serde(skip)]
    surface: Option<SurfaceIndex>,
    #[serde(skip)]
    node: Option<NodeIndex>,
}

/// The main application struct
pub struct DemoLensApp {
    // Multi-layer support
    pub layers: HashMap<LayerType, LayerInfo>,
    pub active_layer: LayerType,
    
    // Legacy single layer support (for compatibility)
    pub gerber_layer: GerberLayer,
    pub view_state: ViewState,
    pub ui_state: UiState,
    pub needs_initial_view: bool,

    pub rotation_degrees: f32,
    
    // Logger state, colors, banner, details
    pub logger_state : Dynamic<ReactiveEventLoggerState>,
    pub log_colors   : Dynamic<LogColors>,
    pub banner       : banner::Banner,
    pub details      : details::Details,
    
    // Properties
    pub enable_unique_colors: bool,
    pub enable_polygon_numbering: bool,
    pub mirroring: Mirroring,
    pub center_offset: Vector,
    pub design_offset: Vector,
    pub showing_top: bool,  // true = top layers, false = bottom layers
    
    // DRC Properties
    pub current_drc_ruleset: Option<String>,
    pub drc_rules: DrcRules,
    pub drc_violations: Vec<DrcViolation>,
    pub trace_quality_issues: Vec<drc::TraceQualityIssue>,
    pub rounded_corner_primitives: Vec<gerber_viewer::GerberPrimitive>,
    pub corner_overlay_shapes: Vec<drc::CornerOverlayShape>,
    
    // Mouse position tracking
    pub mouse_pos_display_units: bool, // true = mils, false = mm
    
    // Grid Settings
    pub grid_settings: GridSettings,

    // Dock state
    dock_state: DockState<Tab>,
    config_path: PathBuf,
}

impl Tab {
    fn new(kind: TabKind, surface: SurfaceIndex, node: NodeIndex) -> Self {
        Self {
            kind,
            surface: Some(surface),
            node: Some(node),
        }
    }

    fn title(&self) -> String {
        match self.kind {
            TabKind::ViewSettings => "View Settings".to_string(),
            TabKind::DRC => "DRC".to_string(),
            TabKind::GerberView => "Gerber View".to_string(),
            TabKind::EventLog => "Event Log".to_string(),
        }
    }


    fn content(&self, ui: &mut egui::Ui, params: &mut TabParams<'_>) {
        match self.kind {
            TabKind::ViewSettings => {
                // Use vertical layout like diskforge
                ui.vertical(|ui| {
                    let logger_state_clone = params.app.logger_state.clone();
                    let log_colors_clone = params.app.log_colors.clone();
                    
                    // Layer Controls Section
                    ui.heading("Layer Controls");
                    ui.separator();
                    ui::show_layers_panel(ui, params.app, &logger_state_clone, &log_colors_clone);
                    
                    ui.add_space(20.0);
                    
                    // Orientation Section
                    ui.heading("Orientation");
                    ui.separator();
                    ui::show_orientation_panel(ui, params.app, &logger_state_clone, &log_colors_clone);
                    
                    ui.add_space(20.0);
                    
                    // Mouse Cursor Section
                    ui.heading("Mouse Cursor");
                    ui.separator();
                    ui.horizontal(|ui| {
                        ui.label("Coordinate Units:");
                        ui.selectable_value(&mut params.app.mouse_pos_display_units, false, "mm");
                        ui.selectable_value(&mut params.app.mouse_pos_display_units, true, "mils");
                    });
                    
                    ui.add_space(20.0);
                    
                    // Grid Settings Section
                    ui.heading("Grid Settings");
                    ui.separator();
                    ui::show_grid_panel(ui, params.app, &logger_state_clone, &log_colors_clone);
                });
            }
            TabKind::DRC => {
                let logger_state_clone = params.app.logger_state.clone();
                let log_colors_clone = params.app.log_colors.clone();
                ui::show_drc_panel(ui, params.app, &logger_state_clone, &log_colors_clone);
            }
            TabKind::GerberView => {
                self.render_gerber_view(ui, params.app);
            }
            TabKind::EventLog => {
                let logger = ReactiveEventLogger::with_colors(&params.app.logger_state, &params.app.log_colors);
                logger.show(ui);
            }
        }
    }

    fn render_gerber_view(&self, ui: &mut egui::Ui, app: &mut DemoLensApp) {
        // Fill all available space in the panel
        ui.ctx().request_repaint(); // Ensure continuous updates
        
        // Use allocate_response to ensure we fill the entire available area
        let available_size = ui.available_size();
        // Ensure minimum size to avoid zero-sized allocations
        let size = egui::Vec2::new(
            available_size.x.max(100.0),
            available_size.y.max(100.0)
        );
        let response = ui.allocate_response(size, egui::Sense::click_and_drag());
        let viewport = response.rect;
        
        // Handle double-click to center (same as Center button)
        if response.double_clicked() {
            // Apply the same logic as the "Center" button in orientation panel
            app.center_offset = gerber_viewer::position::Vector::new(0.0, 0.0);
            app.design_offset = gerber_viewer::position::Vector::new(0.0, 0.0);
            app.needs_initial_view = true;
        }
        
        // Get mouse position for cursor tracking
        let mouse_pos_screen = ui.input(|i| i.pointer.hover_pos());

        // Fill the background with the panel color to ensure no black gaps
        let painter = ui.painter_at(viewport);
        painter.rect_filled(viewport, 0.0, ui.visuals().extreme_bg_color);

        if app.needs_initial_view {
            app.reset_view(viewport)
        }
        
        app.ui_state.update(ui, &viewport, &response, &mut app.view_state);

        let painter = ui.painter().with_clip_rect(viewport);
        
        // Draw grid if enabled (before other elements so it appears underneath)
        grid::draw_grid(&painter, &viewport, &app.view_state, &app.grid_settings);
        
        draw_crosshair(&painter, app.ui_state.origin_screen_pos, Color32::BLUE);
        draw_crosshair(&painter, app.ui_state.center_screen_pos, Color32::LIGHT_GRAY);

        // Render all visible layers based on showing_top
        for layer_type in LayerType::all() {
            if let Some(layer_info) = app.layers.get(&layer_type) {
                if layer_info.visible {
                    // Filter based on showing_top
                    let should_render = layer_type.should_render(app.showing_top);
                    
                    if should_render {
                        // Use the layer's specific gerber data if available, otherwise fall back to demo
                        let gerber_to_render = layer_info.gerber_layer.as_ref()
                            .unwrap_or(&app.gerber_layer);
                        
                        GerberRenderer::default().paint_layer(
                            &painter,
                            app.view_state,
                            gerber_to_render,
                            layer_type.color(),
                            false, // Don't use unique colors for multi-layer view
                            false, // Don't show polygon numbering
                            app.rotation_degrees.to_radians(),
                            app.mirroring,
                            app.center_offset.into(),
                            app.design_offset.into(),
                        );
                    }
                }
            }
        }

        // Get bounding box and outline vertices
        let bbox = app.gerber_layer.bounding_box();
        let origin = app.center_offset - app.design_offset;
        let bbox_vertices = bbox.vertices();  
        let outline_vertices = bbox.vertices();  
        
        // Transform vertices after getting them
        let bbox_vertices_screen = bbox_vertices.iter()
            .map(|v| app.view_state.gerber_to_screen_coords(*v + origin.to_position()))
            .collect::<Vec<_>>();
            
        let outline_vertices_screen = outline_vertices.iter()
            .map(|v| app.view_state.gerber_to_screen_coords(*v + origin.to_position()))
            .collect::<Vec<_>>();

        draw_outline(&painter, bbox_vertices_screen, Color32::RED);
        draw_outline(&painter, outline_vertices_screen, Color32::GREEN);

        let screen_radius = MARKER_RADIUS * app.view_state.scale;

        let design_offset_screen_position = app.view_state.gerber_to_screen_coords(app.design_offset.to_position());
        draw_arrow(&painter, design_offset_screen_position, app.ui_state.origin_screen_pos, Color32::ORANGE);
        draw_marker(&painter, design_offset_screen_position, Color32::ORANGE, Color32::YELLOW, screen_radius);

        let design_origin_screen_position = app.view_state.gerber_to_screen_coords((app.center_offset - app.design_offset).to_position());
        draw_marker(&painter, design_origin_screen_position, Color32::PURPLE, Color32::MAGENTA, screen_radius);
        
        // Render corner overlay shapes (rounded corners)
        if !app.corner_overlay_shapes.is_empty() {
            // Use a different color for the overlay (bright green for visibility)
            let overlay_color = Color32::from_rgb(0, 255, 0); // Bright green
            
            for shape in &app.corner_overlay_shapes {
                // Transform all polygon vertices
                let mut transformed_vertices = Vec::new();
                
                for point in &shape.points {
                    let mut vertex_pos = *point;
                    
                    // Apply rotation if any
                    if app.rotation_degrees != 0.0 {
                        let rotation_radians = app.rotation_degrees.to_radians();
                        let (sin_theta, cos_theta) = (rotation_radians.sin(), rotation_radians.cos());
                        
                        let rotated_x = vertex_pos.x * cos_theta as f64 - vertex_pos.y * sin_theta as f64;
                        let rotated_y = vertex_pos.x * sin_theta as f64 + vertex_pos.y * cos_theta as f64;
                        vertex_pos = gerber_viewer::position::Position::new(rotated_x, rotated_y);
                    }
                    
                    // Apply mirroring if any
                    if app.mirroring.x {
                        vertex_pos = vertex_pos.invert_x();
                    }
                    if app.mirroring.y {
                        vertex_pos = vertex_pos.invert_y();
                    }
                    
                    // Apply center and design offsets
                    let origin = app.center_offset - app.design_offset;
                    vertex_pos = vertex_pos + origin.to_position();
                    
                    let vertex_screen = app.view_state.gerber_to_screen_coords(vertex_pos);
                    transformed_vertices.push(vertex_screen);
                }
                
                // Draw filled polygon for the entire rounded corner
                if transformed_vertices.len() >= 3 {
                    painter.add(egui::Shape::convex_polygon(
                        transformed_vertices,
                        overlay_color,
                        Stroke::NONE
                    ));
                }
            }
        }
        
        // Draw DRC violation markers
        for violation in &app.drc_violations {
            let violation_pos = gerber_viewer::position::Position::new(violation.x as f64, violation.y as f64);
            
            // Apply the same transformation pipeline as GerberRenderer::paint_layer()
            let mut transformed_pos = violation_pos;
            
            // Apply rotation if any
            if app.rotation_degrees != 0.0 {
                let rotation_radians = app.rotation_degrees.to_radians();
                let (sin_theta, cos_theta) = (rotation_radians.sin(), rotation_radians.cos());
                let rotated_x = transformed_pos.x * cos_theta as f64 - transformed_pos.y * sin_theta as f64;
                let rotated_y = transformed_pos.x * sin_theta as f64 + transformed_pos.y * cos_theta as f64;
                transformed_pos = gerber_viewer::position::Position::new(rotated_x, rotated_y);
            }
            
            // Apply mirroring if any
            if app.mirroring.x { // X mirroring
                transformed_pos = transformed_pos.invert_x();
            }
            if app.mirroring.y { // Y mirroring
                transformed_pos = transformed_pos.invert_y();
            }
            
            // Apply center and design offsets
            transformed_pos = transformed_pos + origin.to_position();
            
            let screen_pos = app.view_state.gerber_to_screen_coords(transformed_pos);
            
            // All markers now represent trace areas (1 per trace)
            let base_size = 3.0; // Small but visible markers
            let marker_size = base_size * app.view_state.scale.max(0.5); // Scale with zoom but not too small
            let color = Color32::RED;
            
            draw_violation_marker(&painter, screen_pos, marker_size, color);
        }
        
        // Draw board dimensions in mils at the bottom
        if let Some(layer_info) = app.layers.get(&LayerType::MechanicalOutline) {
            if let Some(ref outline_layer) = layer_info.gerber_layer {
                let bbox = outline_layer.bounding_box();
                let width_mm = bbox.width();
                let height_mm = bbox.height();
                let width_mils = width_mm / 0.0254;
                let height_mils = height_mm / 0.0254;
                
                let dimension_text = format!("{:.0} x {:.0} mils", width_mils, height_mils);
                let text_pos = viewport.max - Vec2::new(10.0, 50.0);
                painter.text(
                    text_pos,
                    egui::Align2::RIGHT_BOTTOM,
                    dimension_text,
                    egui::FontId::default(),
                    Color32::from_rgb(200, 200, 200),
                );
            }
        }
        
        // Draw mouse position cursor indicator
        if let Some(mouse_screen_pos) = mouse_pos_screen {
            if viewport.contains(mouse_screen_pos) {
                // Convert screen position to gerber coordinates
                let gerber_pos = app.view_state.screen_to_gerber_coords(mouse_screen_pos);
                
                // Apply the same transformation as other elements for consistency
                let origin = app.center_offset - app.design_offset;
                let adjusted_pos = gerber_viewer::position::Position::new(
                    gerber_pos.x - origin.to_position().x,
                    gerber_pos.y - origin.to_position().y
                );
                
                // Format coordinates based on user preference
                let cursor_text = if app.mouse_pos_display_units {
                    // Display in mils
                    let x_mils = adjusted_pos.x / 0.0254;
                    let y_mils = adjusted_pos.y / 0.0254;
                    format!("({:.0}, {:.0}) mils", x_mils, y_mils)
                } else {
                    // Display in mm
                    format!("({:.2}, {:.2}) mm", adjusted_pos.x, adjusted_pos.y)
                };
                
                // Position cursor text near mouse
                let text_offset = Vec2::new(15.0, -15.0);
                let cursor_text_pos = mouse_screen_pos + text_offset;
                
                // Draw background box for better readability
                let text_size = painter.text(
                    cursor_text_pos,
                    egui::Align2::LEFT_TOP,
                    "",
                    egui::FontId::monospace(12.0),
                    Color32::WHITE,
                ).size();
                
                let background_rect = egui::Rect::from_min_size(
                    cursor_text_pos - Vec2::new(2.0, 2.0),
                    text_size + Vec2::new(4.0, 4.0)
                );
                
                painter.rect_filled(
                    background_rect,
                    3.0,
                    Color32::from_rgba_unmultiplied(0, 0, 0, 180)
                );
                
                // Draw the actual cursor text
                painter.text(
                    cursor_text_pos,
                    egui::Align2::LEFT_TOP,
                    cursor_text,
                    egui::FontId::monospace(12.0),
                    Color32::WHITE,
                );
                
                // Draw crosshair at mouse position
                let crosshair_size = 8.0;
                painter.line_segment(
                    [
                        mouse_screen_pos - Vec2::new(crosshair_size, 0.0),
                        mouse_screen_pos + Vec2::new(crosshair_size, 0.0)
                    ],
                    Stroke::new(1.0, Color32::WHITE)
                );
                painter.line_segment(
                    [
                        mouse_screen_pos - Vec2::new(0.0, crosshair_size),
                        mouse_screen_pos + Vec2::new(0.0, crosshair_size)
                    ],
                    Stroke::new(1.0, Color32::WHITE)
                );
            }
        }
        
        // Add unit toggle in top-right corner
        let unit_toggle_pos = viewport.max - Vec2::new(10.0, 30.0);
        let unit_text = if app.mouse_pos_display_units { "mils" } else { "mm" };
        painter.text(
            unit_toggle_pos,
            egui::Align2::RIGHT_BOTTOM,
            format!("Mouse: {}", unit_text),
            egui::FontId::default(),
            Color32::from_rgb(150, 150, 150),
        );
    }
}

struct TabViewer<'a> {
    app: &'a mut DemoLensApp,
}

impl<'a> egui_dock::TabViewer for TabViewer<'a> {
    type Tab = Tab;

    fn title(&mut self, tab: &mut Self::Tab) -> egui::WidgetText {
        tab.title().into()
    }

    fn ui(&mut self, ui: &mut egui::Ui, tab: &mut Self::Tab) {
        let mut params = TabParams {
            app: self.app,
            // ...other fields as needed
        };
        tab.content(ui, &mut params);
    }
}

impl Drop for DemoLensApp {
    fn drop(&mut self) {
        // Save dock state when application closes
        self.save_dock_state();
    }
}

impl DemoLensApp {
    /// **Create a new instance of the DemoLensApp**
    ///
    /// This function initializes the application state, including loading the Gerber layer,
    /// setting up the logger, and configuring the UI properties. It also sets up the initial view
    /// and adds platform details to the app. The function returns a new instance of the DemoLensApp.
    ///
    pub fn new() -> Self {
        // Load the demo gerber for legacy compatibility
        let demo_str = include_str!("../assets/demo.gbr").as_bytes();
        let reader = BufReader::new(demo_str);
        let doc = parse(reader).unwrap();
        let commands = doc.into_commands();
        let gerber_layer = GerberLayer::new(commands);
        
        // Initialize layers HashMap
        let mut layers = HashMap::new();
        
        // Map layer types to their corresponding gerber files
        let layer_files = [
            (LayerType::TopCopper, "cmod_s7-F_Cu.gbr"),
            (LayerType::BottomCopper, "cmod_s7-B_Cu.gbr"),
            (LayerType::TopSilk, "cmod_s7-F_SilkS.gbr"),
            (LayerType::BottomSilk, "cmod_s7-B_SilkS.gbr"),
            (LayerType::TopSoldermask, "cmod_s7-F_Mask.gbr"),
            (LayerType::BottomSoldermask, "cmod_s7-B_Mask.gbr"),
            (LayerType::MechanicalOutline, "cmod_s7-Edge_Cuts.gbr"),
        ];
        
        // Load each layer's gerber file
        for (layer_type, filename) in layer_files {
            let gerber_data = match filename {
                "cmod_s7-F_Cu.gbr" => include_str!("../assets/cmod_s7-F_Cu.gbr"),
                "cmod_s7-B_Cu.gbr" => include_str!("../assets/cmod_s7-B_Cu.gbr"),
                "cmod_s7-F_SilkS.gbr" => include_str!("../assets/cmod_s7-F_SilkS.gbr"),
                "cmod_s7-B_SilkS.gbr" => include_str!("../assets/cmod_s7-B_SilkS.gbr"),
                "cmod_s7-F_Mask.gbr" => include_str!("../assets/cmod_s7-F_Mask.gbr"),
                "cmod_s7-B_Mask.gbr" => include_str!("../assets/cmod_s7-B_Mask.gbr"),
                "cmod_s7-Edge_Cuts.gbr" => include_str!("../assets/cmod_s7-Edge_Cuts.gbr"),
                _ => include_str!("../assets/demo.gbr"), // Fallback
            };
            
            let reader = BufReader::new(gerber_data.as_bytes());
            let layer_gerber = match parse(reader) {
                Ok(doc) => {
                    let commands = doc.into_commands();
                    Some(GerberLayer::new(commands))
                }
                Err(e) => {
                    eprintln!("Failed to parse {}: {:?}", filename, e);
                    None
                }
            };
            
            let layer_info = LayerInfo::new(
                layer_type,
                layer_gerber,
                Some(gerber_data.to_string()),  // Store raw Gerber data for DRC
                matches!(layer_type, LayerType::TopCopper | LayerType::MechanicalOutline),
            );
            layers.insert(layer_type, layer_info);
        }
        
        // Create logger state, colors, banner, and details
        let logger_state = Dynamic::new(ReactiveEventLoggerState::new());
        let log_colors = Dynamic::new(LogColors::default());
        let mut banner = banner::Banner::new(); 
        banner.format(); 
        let mut details = details::Details::new(); 
        details.get_os();
        

        // Initialize dock state with gerber view as the main content
        let view_settings_tab = Tab::new(TabKind::ViewSettings, SurfaceIndex::main(), NodeIndex(0));
        let drc_tab = Tab::new(TabKind::DRC, SurfaceIndex::main(), NodeIndex(1));
        let gerber_tab = Tab::new(TabKind::GerberView, SurfaceIndex::main(), NodeIndex(2));
        let log_tab = Tab::new(TabKind::EventLog, SurfaceIndex::main(), NodeIndex(3));
        
        // Create dock state with gerber view as the root
        let mut dock_state = DockState::new(vec![gerber_tab]);
        let surface = dock_state.main_surface_mut();
        
        // Split left for control panels
        let [left, _right] = surface.split_left(
            NodeIndex::root(),
            0.3, // Left panel takes 30% of width
            vec![view_settings_tab, drc_tab],
        );
        
        // Add event log to bottom of left panel
        surface.split_below(
            left,
            0.7, // Top takes 70% of height
            vec![log_tab],
        );

        let app = Self {
            layers,
            active_layer: LayerType::TopCopper,
            gerber_layer,
            view_state: ViewState::default(),
            ui_state: UiState::default(),
            needs_initial_view: true,
            rotation_degrees: 0.0,
            logger_state,
            log_colors,
            banner,
            details,
            enable_unique_colors: ENABLE_UNIQUE_SHAPE_COLORS,
            enable_polygon_numbering: ENABLE_POLYGON_NUMBERING,
            mirroring: MIRRORING.into(),
            center_offset: CENTER_OFFSET,
            design_offset: DESIGN_OFFSET,
            showing_top: true,
            current_drc_ruleset: None,
            drc_rules: DrcRules::default(),
            drc_violations: Vec::new(),
            trace_quality_issues: Vec::new(),
            rounded_corner_primitives: Vec::new(),
            corner_overlay_shapes: Vec::new(),
            mouse_pos_display_units: false, // Default to mm
            grid_settings: GridSettings::default(),
            dock_state,
            config_path: dirs::config_dir()
                .map(|d| d.join("kiforge"))
                .unwrap_or_default(),
        };
        
        // Add platform details
        app.add_banner_platform_details();
        
        app
    }

    /// **Add platform details to the app**
    /// 
    /// These functions are customizable via the `platform` module.
    /// The `add_banner_platform_details` function is responsible for logging the banner message
    /// and system details. It creates a logger using the `ReactiveEventLogger` and logs the banner
    /// and operating system details.
     fn add_banner_platform_details(&self) {
        // Create a logger using references to our logger state
        let logger = ReactiveEventLogger::with_colors(&self.logger_state, &self.log_colors);
        
        // Log banner message (welcome message)
        logger.log_info(&self.banner.message);
        
        // Log system details
        let details_text = self.details.clone().format_os();
        logger.log_info(&details_text);
     }

    fn reset_view(&mut self, viewport: Rect) {
        // Find bounding box from all loaded layers
        let mut combined_bbox: Option<BoundingBox> = None;
        
        for layer_info in self.layers.values() {
            if let Some(ref layer_gerber) = layer_info.gerber_layer {
                let layer_bbox = layer_gerber.bounding_box();
                combined_bbox = Some(match combined_bbox {
                    None => layer_bbox.clone(),
                    Some(existing) => BoundingBox {
                        min: gerber_viewer::position::Position::new(
                            existing.min.x.min(layer_bbox.min.x),
                            existing.min.y.min(layer_bbox.min.y),
                        ),
                        max: gerber_viewer::position::Position::new(
                            existing.max.x.max(layer_bbox.max.x),
                            existing.max.y.max(layer_bbox.max.y),
                        ),
                    },
                });
            }
        }
        
        // Fall back to demo gerber if no layers loaded
        let bbox = combined_bbox.unwrap_or_else(|| self.gerber_layer.bounding_box().clone());
        let content_width = bbox.width();
        let content_height = bbox.height();

        // Calculate scale to fit the content (100% zoom)
        let scale = f32::min(
            viewport.width() / (content_width as f32),
            viewport.height() / (content_height as f32),
        );
        // adjust slightly to add a margin
        let scale = scale * 0.95;

        let center = bbox.center();

        // Offset from viewport center to place content in the center
        self.view_state.translation = Vec2::new(
            viewport.center().x - (center.x as f32 * scale),
            viewport.center().y + (center.y as f32 * scale), // Note the + here since we flip Y
        );

        self.view_state.scale = scale;
        self.needs_initial_view = false;
    }
}

impl DemoLensApp {
    fn save_dock_state(&self) {
        if let Some(config_dir) = dirs::config_dir() {
            let kiforge_dir = config_dir.join("kiforge");
            fs::create_dir_all(&kiforge_dir).ok();
            let config_path = kiforge_dir.join("dock_state.json");
            if let Ok(json) = serde_json::to_string_pretty(&self.dock_state) {
                fs::write(config_path, json).ok();
            }
        }
    }
}

/// Implement the eframe::App trait for DemoLensApp
///
/// This implementation contains the main event loop for the application, including
/// handling user input, updating the UI, and rendering the Gerber layer. It also contains
/// the logic for handling the logger and displaying system information.
/// The `update` method is called every frame and is responsible for updating the UI
/// and rendering the Gerber layer. It also handles user input and updates the logger
/// state. The `update` method is where most of the application logic resides.
/// 
impl eframe::App for DemoLensApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Clone the dock state
        let mut dock_state = self.dock_state.clone();
        
        // Create the dock layout and tab viewer
        let mut tab_viewer = TabViewer { app: self };
        
        // Create custom style to match panel colors
        let mut style = Style::from_egui(ctx.style().as_ref());
        style.dock_area_padding = None;
        style.tab_bar.fill_tab_bar = true;
        
        // Show the dock area directly on the context
        DockArea::new(&mut dock_state)
            .style(style)
            .show_add_buttons(false)
            .show_close_buttons(true)
            .show(ctx, &mut tab_viewer);
            
        // Save the updated dock state back to the app
        self.dock_state = dock_state;
        
        // Save dock state to disk periodically
        if ctx.input(|i| i.time) % 30.0 < 0.1 {
            self.save_dock_state();
        }
    }
}

/// The main function is the entry point of the application.
/// 
/// It initializes the logger, sets up the native window options,
/// and runs the application using the `eframe` framework.
fn main() -> eframe::Result<()> {
    env_logger::init(); // Log to stderr (optional).
    eframe::run_native(
        "Gerber Viewer Lens Demo (egui)",
        eframe::NativeOptions {
            viewport: ViewportBuilder::default().with_inner_size([1280.0, 768.0]),
            ..Default::default()
        },
        Box::new(|_cc| Ok(Box::new(DemoLensApp::new()))),
    )
}