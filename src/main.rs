use std::io::BufReader;
use std::{fs, path::PathBuf};

use eframe::emath::{Rect, Vec2};
use eframe::epaint::Color32;
use egui::ViewportBuilder;
use egui_dock::{DockArea, DockState, NodeIndex, Style, SurfaceIndex};
use serde::{Serialize, Deserialize};

mod managers;
use managers::{ProjectManager, ProjectState, DisplayManager};

/// egui_lens imports
use egui_lens::{ReactiveEventLogger, ReactiveEventLoggerState, LogColors};

/// Use of prelude for egui_mobius_reactive
use egui_mobius_reactive::Dynamic;  
use std::collections::HashMap;

use gerber_viewer::gerber_parser::parse;
use log;
use gerber_viewer::{
   draw_arrow, draw_outline, draw_crosshair, BoundingBox, GerberLayer, GerberRenderer, 
   ViewState, draw_marker, UiState
};
use egui::{Painter, Pos2, Stroke};


// Import platform modules
mod platform;
use platform::{banner, details};

// Import new modules
mod constants;
mod layers;
mod grid;
mod ui;
mod drc;
mod layer_detection;

use constants::*;
use layers::{LayerType, LayerInfo};
use grid::GridSettings;
use drc::{DrcRules, DrcViolation};
use layer_detection::{LayerDetector, UnassignedGerber};

// DRC structures are now imported from the drc module

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


/// Define the tabs for the DockArea
#[derive(Clone, Serialize, Deserialize)]
enum TabKind {
    ViewSettings,
    DRC,
    GerberView,
    EventLog,
    Project,
    Settings,
}

pub struct TabParams<'a> {
    pub app: &'a mut DemoLensApp,

}

/// Tab container struct for DockArea
#[derive(Clone, Serialize, Deserialize)]
struct Tab {
    kind: TabKind,
    #[serde(skip)]
    #[allow(dead_code)]
    surface: Option<SurfaceIndex>,
    #[serde(skip)]
    #[allow(dead_code)]
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
    
    // Display settings
    pub display_manager: DisplayManager,
    
    // DRC Properties
    pub current_drc_ruleset: Option<String>,
    pub drc_rules: DrcRules,
    pub drc_violations: Vec<DrcViolation>,
    pub trace_quality_issues: Vec<drc::TraceQualityIssue>,
    pub rounded_corner_primitives: Vec<gerber_viewer::GerberPrimitive>,
    pub corner_overlay_shapes: Vec<drc::CornerOverlayShape>,
    
    // Global units setting
    pub global_units_mils: bool, // true = mils, false = mm
    
    // Grid Settings
    pub grid_settings: GridSettings,
    
    // Project management
    pub project_manager: ProjectManager,
    
    // Legacy fields for compatibility (will be removed later)
    pub selected_pcb_file: Option<PathBuf>,
    pub generated_gerber_dir: Option<PathBuf>,
    pub generating_gerbers: bool,
    pub loading_gerbers: bool,

    // Dock state
    dock_state: DockState<Tab>,
    config_path: PathBuf,
    
    // Layer detection and unassigned gerbers
    pub layer_detector: LayerDetector,
    pub unassigned_gerbers: Vec<UnassignedGerber>,
    pub layer_assignments: HashMap<String, LayerType>, // filename -> assigned layer type
    
    // Zoom window state
    pub zoom_window_start: Option<Pos2>,
    pub zoom_window_dragging: bool,
    
    // User preferences
    pub user_timezone: Option<String>,
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
            TabKind::Project => "Project".to_string(),
            TabKind::Settings => "Settings".to_string(),
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
            TabKind::Project => {
                let logger_state_clone = params.app.logger_state.clone();
                let log_colors_clone = params.app.log_colors.clone();
                ui::show_project_panel(ui, params.app, &logger_state_clone, &log_colors_clone);
            }
            TabKind::Settings => {
                let logger_state_clone = params.app.logger_state.clone();
                let log_colors_clone = params.app.log_colors.clone();
                ui::show_settings_panel(ui, params.app, &logger_state_clone, &log_colors_clone);
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
            app.display_manager.center_offset = managers::display::VectorOffset { x: 0.0, y: 0.0 };
            app.display_manager.design_offset = managers::display::VectorOffset { x: 0.0, y: 0.0 };
            app.needs_initial_view = true;
        }
        
        // Get mouse position for cursor tracking
        let mouse_pos_screen = ui.input(|i| i.pointer.hover_pos());
        
        // Handle right-click drag zoom window
        let right_button = egui::PointerButton::Secondary;
        if response.contains_pointer() {
            if ui.input(|i| i.pointer.button_pressed(right_button)) {
                // Start zoom window
                if let Some(pos) = mouse_pos_screen {
                    app.zoom_window_start = Some(pos);
                    app.zoom_window_dragging = true;
                }
            }
        }
        
        if app.zoom_window_dragging && ui.input(|i| i.pointer.button_released(right_button)) {
            // Complete zoom window
            if let (Some(start), Some(end)) = (app.zoom_window_start, mouse_pos_screen) {
                // Calculate zoom rectangle
                let zoom_rect = Rect::from_two_pos(start, end);
                
                // Only zoom if the rectangle is large enough
                if zoom_rect.width() > 10.0 && zoom_rect.height() > 10.0 {
                    // Convert screen coordinates to gerber coordinates
                    let gerber_start = app.view_state.screen_to_gerber_coords(zoom_rect.min);
                    let gerber_end = app.view_state.screen_to_gerber_coords(zoom_rect.max);
                    
                    // Calculate new scale to fit the selected region
                    let gerber_width = (gerber_end.x - gerber_start.x).abs() as f32;
                    let gerber_height = (gerber_end.y - gerber_start.y).abs() as f32;
                    
                    let scale_x = viewport.width() / gerber_width;
                    let scale_y = viewport.height() / gerber_height;
                    let new_scale = scale_x.min(scale_y) * 0.9; // 90% to add some padding
                    
                    // Calculate center of zoom rectangle in gerber coordinates
                    let gerber_center_x = (gerber_start.x + gerber_end.x) / 2.0;
                    let gerber_center_y = (gerber_start.y + gerber_end.y) / 2.0;
                    
                    // Update view state
                    app.view_state.scale = new_scale;
                    
                    // Calculate translation to center the zoomed region
                    let viewport_center = viewport.center();
                    app.view_state.translation = Vec2::new(
                        viewport_center.x - (gerber_center_x * new_scale as f64) as f32,
                        viewport_center.y + (gerber_center_y * new_scale as f64) as f32 // Y is flipped
                    );
                }
            }
            
            app.zoom_window_dragging = false;
            app.zoom_window_start = None;
        }
        
        // Cancel zoom window on escape
        if app.zoom_window_dragging && ui.input(|i| i.key_pressed(egui::Key::Escape)) {
            app.zoom_window_dragging = false;
            app.zoom_window_start = None;
        }

        // Fill the background with the panel color to ensure no black gaps
        let painter = ui.painter_at(viewport);
        painter.rect_filled(viewport, 0.0, ui.visuals().extreme_bg_color);

        if app.needs_initial_view {
            app.reset_view(viewport)
        }
        
        // Only update normal mouse handling if not doing zoom window
        if !app.zoom_window_dragging {
            app.ui_state.update(ui, &viewport, &response, &mut app.view_state);
        }

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
                    let should_render = layer_type.should_render(app.display_manager.showing_top);
                    
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
                            app.display_manager.mirroring.clone().into(),
                            app.display_manager.center_offset.clone().into(),
                            app.display_manager.design_offset.clone().into(),
                        );
                    }
                }
            }
        }

        // Get bounding box and outline vertices
        let bbox = app.gerber_layer.bounding_box();
        let center_vec: gerber_viewer::position::Vector = app.display_manager.center_offset.clone().into();
        let design_vec: gerber_viewer::position::Vector = app.display_manager.design_offset.clone().into();
        let origin = center_vec - design_vec;
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

        let design_offset_vec: gerber_viewer::position::Vector = app.display_manager.design_offset.clone().into();
        let design_offset_screen_position = app.view_state.gerber_to_screen_coords(design_offset_vec.to_position());
        draw_arrow(&painter, design_offset_screen_position, app.ui_state.origin_screen_pos, Color32::ORANGE);
        draw_marker(&painter, design_offset_screen_position, Color32::ORANGE, Color32::YELLOW, screen_radius);

        let center_offset_vec: gerber_viewer::position::Vector = app.display_manager.center_offset.clone().into();
        let design_offset_vec2: gerber_viewer::position::Vector = app.display_manager.design_offset.clone().into();
        let design_origin_screen_position = app.view_state.gerber_to_screen_coords((center_offset_vec - design_offset_vec2).to_position());
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
                    if app.display_manager.mirroring.x {
                        vertex_pos = vertex_pos.invert_x();
                    }
                    if app.display_manager.mirroring.y {
                        vertex_pos = vertex_pos.invert_y();
                    }
                    
                    // Apply center and design offsets
                    let center_vec: gerber_viewer::position::Vector = app.display_manager.center_offset.clone().into();
                    let design_vec: gerber_viewer::position::Vector = app.display_manager.design_offset.clone().into();
                    let origin = center_vec - design_vec;
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
            if app.display_manager.mirroring.x { // X mirroring
                transformed_pos = transformed_pos.invert_x();
            }
            if app.display_manager.mirroring.y { // Y mirroring
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
        
        // Draw board dimensions at the bottom
        if let Some(layer_info) = app.layers.get(&LayerType::MechanicalOutline) {
            if let Some(ref outline_layer) = layer_info.gerber_layer {
                let bbox = outline_layer.bounding_box();
                let width_mm = bbox.width();
                let height_mm = bbox.height();
                
                let dimension_text = if app.global_units_mils {
                    let width_mils = width_mm / 0.0254;
                    let height_mils = height_mm / 0.0254;
                    format!("{:.0} x {:.0} mils", width_mils, height_mils)
                } else {
                    format!("{:.1} x {:.1} mm", width_mm, height_mm)
                };
                
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
        
        // Draw zoom window rectangle if dragging
        if app.zoom_window_dragging {
            if let (Some(start), Some(current)) = (app.zoom_window_start, mouse_pos_screen) {
                let zoom_rect = Rect::from_two_pos(start, current);
                
                // Draw semi-transparent fill
                painter.rect_filled(
                    zoom_rect,
                    0.0,
                    Color32::from_rgba_unmultiplied(100, 150, 255, 50)
                );
                
                // Draw border with lines instead of rect_stroke
                let stroke = Stroke::new(2.0, Color32::from_rgb(100, 150, 255));
                painter.line_segment([zoom_rect.min, Pos2::new(zoom_rect.max.x, zoom_rect.min.y)], stroke);
                painter.line_segment([Pos2::new(zoom_rect.max.x, zoom_rect.min.y), zoom_rect.max], stroke);
                painter.line_segment([zoom_rect.max, Pos2::new(zoom_rect.min.x, zoom_rect.max.y)], stroke);
                painter.line_segment([Pos2::new(zoom_rect.min.x, zoom_rect.max.y), zoom_rect.min], stroke);
                
                // Draw corner markers
                let corner_size = 5.0;
                let corners = [zoom_rect.min, 
                              Pos2::new(zoom_rect.max.x, zoom_rect.min.y),
                              zoom_rect.max,
                              Pos2::new(zoom_rect.min.x, zoom_rect.max.y)];
                
                for corner in &corners {
                    painter.circle_filled(*corner, corner_size, Color32::from_rgb(100, 150, 255));
                }
                
                // Show dimensions of selection
                if zoom_rect.width() > 50.0 && zoom_rect.height() > 30.0 {
                    let gerber_start = app.view_state.screen_to_gerber_coords(zoom_rect.min);
                    let gerber_end = app.view_state.screen_to_gerber_coords(zoom_rect.max);
                    let width_mm = (gerber_end.x - gerber_start.x).abs() as f32;
                    let height_mm = (gerber_end.y - gerber_start.y).abs() as f32;
                    
                    let dimension_text = if app.global_units_mils {
                        let width_mils = width_mm / 0.0254;
                        let height_mils = height_mm / 0.0254;
                        format!("{:.0} x {:.0} mils", width_mils, height_mils)
                    } else {
                        format!("{:.1} x {:.1} mm", width_mm, height_mm)
                    };
                    
                    let text_pos = zoom_rect.center() + Vec2::new(0.0, -20.0);
                    
                    // Background for text
                    let text_galley = painter.layout_no_wrap(
                        dimension_text.clone(),
                        egui::FontId::default(),
                        Color32::WHITE
                    );
                    let text_rect = egui::Rect::from_min_size(
                        text_pos - text_galley.size() / 2.0 - Vec2::new(4.0, 2.0),
                        text_galley.size() + Vec2::new(8.0, 4.0)
                    );
                    painter.rect_filled(
                        text_rect,
                        3.0,
                        Color32::from_rgba_unmultiplied(0, 0, 0, 200)
                    );
                    
                    // Draw text
                    painter.text(
                        text_pos,
                        egui::Align2::CENTER_CENTER,
                        dimension_text,
                        egui::FontId::default(),
                        Color32::WHITE
                    );
                }
            }
        }
        
        // Draw mouse position cursor indicator
        if let Some(mouse_screen_pos) = mouse_pos_screen {
            if viewport.contains(mouse_screen_pos) {
                // Convert screen position to gerber coordinates
                let gerber_pos = app.view_state.screen_to_gerber_coords(mouse_screen_pos);
                
                // Apply the same transformation as other elements for consistency
                let center_vec: gerber_viewer::position::Vector = app.display_manager.center_offset.clone().into();
                let design_vec: gerber_viewer::position::Vector = app.display_manager.design_offset.clone().into();
                let origin = center_vec - design_vec;
                let adjusted_pos = gerber_viewer::position::Position::new(
                    gerber_pos.x - origin.to_position().x,
                    gerber_pos.y - origin.to_position().y
                );
                
                // Format coordinates based on user preference
                let cursor_text = if app.global_units_mils {
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
        let unit_text = if app.global_units_mils { "mils" } else { "mm" };
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
        // Save project config
        if let Err(e) = self.project_manager.save_to_file(&self.config_path) {
            eprintln!("Failed to save project config: {}", e);
        }
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
        

        // Initialize dock state - force fresh layout for now to include Project tab
        let dock_state = {
            // Temporarily force fresh layout
            // let dock_state = if let Some(saved_dock_state) = Self::load_dock_state() {
            //     saved_dock_state
            // } else {
            // Create default dock layout if no saved state exists
            let view_settings_tab = Tab::new(TabKind::ViewSettings, SurfaceIndex::main(), NodeIndex(0));
            let drc_tab = Tab::new(TabKind::DRC, SurfaceIndex::main(), NodeIndex(1));
            let project_tab = Tab::new(TabKind::Project, SurfaceIndex::main(), NodeIndex(2));
            let settings_tab = Tab::new(TabKind::Settings, SurfaceIndex::main(), NodeIndex(3));
            let gerber_tab = Tab::new(TabKind::GerberView, SurfaceIndex::main(), NodeIndex(4));
            let log_tab = Tab::new(TabKind::EventLog, SurfaceIndex::main(), NodeIndex(5));
            
            // Create dock state with gerber view as the root
            let mut dock_state = DockState::new(vec![gerber_tab]);
            let surface = dock_state.main_surface_mut();
            
            // Split left for control panels
            let [left, _right] = surface.split_left(
                NodeIndex::root(),
                0.3, // Left panel takes 30% of width
                vec![view_settings_tab, drc_tab, project_tab, settings_tab],
            );
            
            // Add event log to bottom of left panel
            surface.split_below(
                left,
                0.7, // Top takes 70% of height
                vec![log_tab],
            );
            
            dock_state
            // }
        };

        let mut app = Self {
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
            display_manager: DisplayManager::new(),
            current_drc_ruleset: None,
            drc_rules: DrcRules::default(),
            drc_violations: Vec::new(),
            trace_quality_issues: Vec::new(),
            rounded_corner_primitives: Vec::new(),
            corner_overlay_shapes: Vec::new(),
            global_units_mils: false, // Default to mm
            grid_settings: GridSettings::default(),
            project_manager: ProjectManager::new(),
            selected_pcb_file: None,
            generated_gerber_dir: None,
            generating_gerbers: false,
            loading_gerbers: false,
            dock_state,
            config_path: dirs::config_dir()
                .map(|d| d.join("kiforge"))
                .unwrap_or_default(),
            layer_detector: LayerDetector::new(),
            unassigned_gerbers: Vec::new(),
            layer_assignments: HashMap::new(),
            zoom_window_start: None,
            zoom_window_dragging: false,
            user_timezone: None,
        };
        
        // Load project config from disk
        if let Ok(project_manager) = ProjectManager::load_from_file(&app.config_path) {
            app.project_manager = project_manager;
            
            // Sync legacy fields with project state
            match &app.project_manager.state {
                ProjectState::NoProject => {},
                ProjectState::PcbSelected { pcb_path } |
                ProjectState::GeneratingGerbers { pcb_path } => {
                    app.selected_pcb_file = Some(pcb_path.clone());
                },
                ProjectState::GerbersGenerated { pcb_path, gerber_dir } |
                ProjectState::LoadingGerbers { pcb_path, gerber_dir } |
                ProjectState::Ready { pcb_path, gerber_dir, .. } => {
                    app.selected_pcb_file = Some(pcb_path.clone());
                    app.generated_gerber_dir = Some(gerber_dir.clone());
                },
            }
        }
        
        // Add platform details
        app.add_banner_platform_details();
        
        // Initialize project based on saved state
        app.initialize_project();
        
        app
    }
    
    fn initialize_project(&mut self) {
        let logger = ReactiveEventLogger::with_colors(&self.logger_state, &self.log_colors);
        
        match &self.project_manager.state.clone() {
            ProjectState::NoProject => {
                logger.log_info("No previous project found. Please select a PCB file.");
            },
            ProjectState::PcbSelected { pcb_path } => {
                if pcb_path.exists() {
                    logger.log_info(&format!("Restored PCB file: {}", pcb_path.display()));
                    if self.project_manager.auto_generate_on_startup {
                        logger.log_info("Auto-generating gerbers...");
                        self.generating_gerbers = true;
                    }
                } else {
                    logger.log_error(&format!("PCB file not found: {}", pcb_path.display()));
                    self.project_manager.state = ProjectState::NoProject;
                }
            },
            ProjectState::GeneratingGerbers { pcb_path } => {
                // Resume generation if interrupted
                if pcb_path.exists() {
                    logger.log_info("Resuming gerber generation...");
                    self.generating_gerbers = true;
                } else {
                    self.project_manager.state = ProjectState::NoProject;
                }
            },
            ProjectState::GerbersGenerated { pcb_path, gerber_dir } => {
                if pcb_path.exists() && gerber_dir.exists() {
                    logger.log_info(&format!("Found generated gerbers at: {}", gerber_dir.display()));
                    if self.project_manager.auto_generate_on_startup {
                        logger.log_info("Auto-loading gerbers...");
                        self.loading_gerbers = true;
                    }
                } else {
                    logger.log_error("PCB or gerber files not found");
                    self.project_manager.state = ProjectState::NoProject;
                }
            },
            ProjectState::LoadingGerbers { pcb_path, gerber_dir } => {
                // Resume loading if interrupted
                if pcb_path.exists() && gerber_dir.exists() {
                    logger.log_info("Resuming gerber loading...");
                    self.loading_gerbers = true;
                } else {
                    self.project_manager.state = ProjectState::NoProject;
                }
            },
            ProjectState::Ready { pcb_path, gerber_dir, .. } => {
                if pcb_path.exists() && gerber_dir.exists() {
                    logger.log_info(&format!("Project ready: {}", pcb_path.file_name().unwrap_or_default().to_string_lossy()));
                    // Auto-load the gerbers
                    self.loading_gerbers = true;
                } else {
                    logger.log_error("Project files not found");
                    self.project_manager.state = ProjectState::NoProject;
                }
            },
        }
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
    
    
    /// Show clock display in the upper right corner
    fn show_clock_display(&self, ui: &mut egui::Ui) {
        use chrono::{Local, Utc};
        use chrono_tz::Tz;
        
        let clock_text = if let Some(tz_name) = &self.user_timezone {
            if let Ok(tz) = tz_name.parse::<Tz>() {
                let now = Utc::now().with_timezone(&tz);
                format!("🕐 {} {}", now.format("%H:%M:%S"), tz.name())
            } else {
                let now = Local::now();
                format!("🕐 {}", now.format("%H:%M:%S"))
            }
        } else {
            let now = Local::now();
            format!("🕐 {}", now.format("%H:%M:%S"))
        };
        
        ui.label(egui::RichText::new(clock_text).color(egui::Color32::from_rgb(150, 150, 150)));
    }
    
    /// Show the main content area (dock layout without Project tab)
    #[allow(dead_code)]
    fn show_main_content(&mut self, ui: &mut egui::Ui) {
        // Clone the dock state but filter out the Project tab
        let mut dock_state = self.dock_state.clone();
        
        // Create the dock layout and tab viewer
        let mut tab_viewer = TabViewer { app: self };
        
        // Create custom style to match panel colors
        let mut style = Style::from_egui(ui.ctx().style().as_ref());
        style.dock_area_padding = None;
        style.tab_bar.fill_tab_bar = true;
        
        // Show the dock area but filtered to exclude Project tab
        DockArea::new(&mut dock_state)
            .style(style)
            .show_add_buttons(false)
            .show_close_buttons(true)
            .show(ui.ctx(), &mut tab_viewer);
            
        // Save the updated dock state back to the app
        self.dock_state = dock_state;
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

    #[allow(dead_code)]
    fn load_dock_state() -> Option<DockState<Tab>> {
        if let Some(config_dir) = dirs::config_dir() {
            let config_path = config_dir.join("kiforge").join("dock_state.json");
            if let Ok(json) = fs::read_to_string(&config_path) {
                if let Ok(dock_state) = serde_json::from_str::<DockState<Tab>>(&json) {
                    // Check if Settings tab exists
                    let mut has_settings = false;
                    for (_, tab) in dock_state.iter_all_tabs() {
                        if matches!(tab.kind, TabKind::Settings) {
                            has_settings = true;
                            break;
                        }
                    }
                    
                    // If Settings tab doesn't exist, delete the saved state to force a fresh layout
                    if !has_settings {
                        fs::remove_file(config_path).ok();
                        return None;
                    }
                    
                    return Some(dock_state);
                }
            }
        }
        None
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
        // Handle system info button clicked
        let show_system_info = ctx.memory(|mem| {
            mem.data.get_temp::<bool>(egui::Id::new("show_system_info")).unwrap_or(false)
        });
        
        if show_system_info {
            // Clear the flag
            ctx.memory_mut(|mem| {
                mem.data.remove::<bool>(egui::Id::new("show_system_info"));
            });
            
            // Create a temporary logger for system info output
            let logger = ReactiveEventLogger::with_colors(&self.logger_state, &self.log_colors);
            
            // Display system details first
            let details_text = self.details.format_os();
            logger.log_info(&details_text);
            
            // Then display banner (so it appears above the details in the log)
            logger.log_info(&self.banner.message);
        }
        
        // Handle hotkeys first
        ctx.input(|i| {
            // F key - flip board view (top/bottom)
            if i.key_pressed(egui::Key::F) {
                self.display_manager.showing_top = !self.display_manager.showing_top;
                let view_name = if self.display_manager.showing_top { "top" } else { "bottom" };
                let logger = ReactiveEventLogger::with_colors(&self.logger_state, &self.log_colors);
                logger.log_info(&format!("Flipped to {} view (F key)", view_name));
            }
            
            // U key - toggle units (mm/mils)
            if i.key_pressed(egui::Key::U) {
                self.global_units_mils = !self.global_units_mils;
                let units_name = if self.global_units_mils { "mils" } else { "mm" };
                let logger = ReactiveEventLogger::with_colors(&self.logger_state, &self.log_colors);
                logger.log_info(&format!("Toggled units to {} (U key)", units_name));
            }
            
            // R key - rotate board 90 degrees clockwise around PCB centroid
            if i.key_pressed(egui::Key::R) {
                // Calculate the centroid of all visible gerber layers
                let mut combined_bbox: Option<gerber_viewer::BoundingBox> = None;
                
                for (_layer_type, layer_info) in &self.layers {
                    if layer_info.visible {
                        if let Some(ref gerber_layer) = layer_info.gerber_layer {
                            let layer_bbox = gerber_layer.bounding_box();
                            combined_bbox = Some(match combined_bbox {
                                None => layer_bbox.clone(),
                                Some(existing) => gerber_viewer::BoundingBox {
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
                }
                
                // Get the current center point that we're rotating around
                let rotation_center = if let Some(bbox) = combined_bbox {
                    bbox.center()
                } else {
                    // Fallback to current design offset if no layers
                    {
                        let design_vec: gerber_viewer::position::Vector = self.display_manager.design_offset.clone().into();
                        design_vec.to_position()
                    }
                };
                
                // To rotate around a specific point, we need to:
                // 1. Translate so the rotation center is at origin (subtract center)
                // 2. Rotate 90 degrees
                // 3. Translate back (add rotated center)
                
                // Calculate what the rotation center will be after rotation
                let angle_rad = 90.0_f32.to_radians();
                let cos_a = angle_rad.cos() as f64;
                let sin_a = angle_rad.sin() as f64;
                
                // Rotate the center point itself
                let rotated_center_x = rotation_center.x * cos_a - rotation_center.y * sin_a;
                let rotated_center_y = rotation_center.x * sin_a + rotation_center.y * cos_a;
                
                // Update rotation
                self.rotation_degrees = (self.rotation_degrees + 90.0) % 360.0;
                
                // Adjust the design offset to account for the rotation around the centroid
                // The offset difference keeps the same point at the center of rotation
                let offset_adjustment = gerber_viewer::position::Vector::new(
                    rotation_center.x - rotated_center_x,
                    rotation_center.y - rotated_center_y
                );
                
                // Apply the offset adjustment
                {
                    let current_offset: gerber_viewer::position::Vector = self.display_manager.design_offset.clone().into();
                    let new_offset = current_offset + offset_adjustment;
                    self.display_manager.design_offset = managers::display::VectorOffset { x: new_offset.x, y: new_offset.y };
                }
                
                let logger = ReactiveEventLogger::with_colors(&self.logger_state, &self.log_colors);
                logger.log_custom(
                    constants::LOG_TYPE_ROTATION,
                    &format!("Rotated board to {:.0}° around PCB centroid (R key)", self.rotation_degrees)
                );
            }
        });
        
        // Project Ribbon at the top
        egui::TopBottomPanel::top("project_ribbon").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 10.0;
                
                // Project Ribbon with file selection
                ui.group(|ui| {
                    ui.horizontal(|ui| {
                        ui.label("📁 KiCad PCB File:");
                        
                        // Show current file or placeholder
                        let current_file_text = match &self.project_manager.state {
                            ProjectState::NoProject => "No file selected".to_string(),
                            ProjectState::Ready { pcb_path, .. } |
                            ProjectState::PcbSelected { pcb_path } |
                            ProjectState::GeneratingGerbers { pcb_path } |
                            ProjectState::GerbersGenerated { pcb_path, .. } |
                            ProjectState::LoadingGerbers { pcb_path, .. } => {
                                pcb_path.file_name()
                                    .map(|n| n.to_string_lossy().to_string())
                                    .unwrap_or_else(|| "Unknown file".to_string())
                            }
                        };
                        
                        ui.label(egui::RichText::new(current_file_text).strong());
                        
                        if ui.button("Browse...").clicked() {
                            self.project_manager.open_file_dialog();
                        }
                        
                        // Handle file dialog
                        if let Some(path_buf) = self.project_manager.update_file_dialog(ui.ctx()) {
                            self.selected_pcb_file = Some(path_buf.clone());
                            let logger = ReactiveEventLogger::with_colors(&self.logger_state, &self.log_colors);
                            logger.log_info(&format!("Selected PCB file: {}", path_buf.display()));
                        }
                    });
                });
                
                // Clock in the upper right
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    self.show_clock_display(ui);
                });
            });
        });
        
        // Main dock area below the ribbon
        let mut dock_state = self.dock_state.clone();
        let mut tab_viewer = TabViewer { app: self };
        let mut style = Style::from_egui(ctx.style().as_ref());
        style.dock_area_padding = None;
        style.tab_bar.fill_tab_bar = true;
        
        DockArea::new(&mut dock_state)
            .style(style)
            .show_add_buttons(false)
            .show_close_buttons(true)
            .show(ctx, &mut tab_viewer);
            
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
    // Configure env_logger to filter out gerber_parser warnings
    env_logger::Builder::from_default_env()
        .filter_module("gerber_parser::parser", log::LevelFilter::Off)
        .init();
    eframe::run_native(
        "KiForge - PCB & CAM for KiCad",
        eframe::NativeOptions {
            viewport: ViewportBuilder::default().with_inner_size([1280.0, 768.0]),
            ..Default::default()
        },
        Box::new(|_cc| Ok(Box::new(DemoLensApp::new()))),
    )
}