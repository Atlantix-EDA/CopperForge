use crate::DemoLensApp;
use crate::project::constants::*;
use crate::ui;

use eframe::emath::{Rect, Vec2};
use eframe::epaint::Color32;
use egui::{Painter, Pos2, Stroke};
use egui_dock::{SurfaceIndex, NodeIndex};
use serde::{Serialize, Deserialize};

use egui_lens::ReactiveEventLogger;
use gerber_viewer::{
    draw_arrow, draw_crosshair, GerberRenderer, 
    draw_marker, ViewState, RenderConfiguration, GerberTransform
};
use crate::drc_operations::types::Position;
use crate::display::manager::ToPosition;
use nalgebra::Vector2;

/// Define the tabs for the DockArea
#[derive(Clone, Serialize, Deserialize)]
pub enum TabKind {
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
pub struct Tab {
    pub kind: TabKind,
    #[serde(skip)]
    #[allow(dead_code)]
    pub surface: Option<SurfaceIndex>,
    #[serde(skip)]
    #[allow(dead_code)]
    pub node: Option<NodeIndex>,
}

impl Tab {
    pub fn new(kind: TabKind, surface: SurfaceIndex, node: NodeIndex) -> Self {
        Self {
            kind,
            surface: Some(surface),
            node: Some(node),
        }
    }

    pub fn title(&self) -> String {
        match self.kind {
            TabKind::ViewSettings => "View Settings".to_string(),
            TabKind::DRC => "DRC".to_string(),
            TabKind::GerberView => "Gerber View".to_string(),
            TabKind::EventLog => "Event Log".to_string(),
            TabKind::Project => "Project".to_string(),
            TabKind::Settings => "Settings".to_string(),
        }
    }

    pub fn content(&self, ui: &mut egui::Ui, params: &mut TabParams<'_>) {
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
        // Controls at the top
        ui.horizontal(|ui| {
            // Quadrant View controls
            if ui.checkbox(&mut app.display_manager.quadrant_view_enabled, "Quadrant View").clicked() {
                // Update layer positions when quadrant view is toggled
                app.display_manager.update_layer_positions(&mut app.layer_manager);
                app.layer_manager.mark_coordinates_dirty();
                app.needs_initial_view = true;
            }
            
            if app.display_manager.quadrant_view_enabled {
                ui.separator();
                ui.label("Offset:");
                
                // Quadrant offset control
                let (mut offset_value, units_suffix, conversion_factor) = if app.global_units_mils {
                    (app.display_manager.quadrant_offset_magnitude / 0.0254, "mils", 0.0254)
                } else {
                    (app.display_manager.quadrant_offset_magnitude, "mm", 1.0)
                };
                
                let speed = if app.global_units_mils { 10.0 } else { 1.0 };
                let max_range = if app.global_units_mils { 20000.0 } else { 500.0 };
                
                if ui.add(egui::DragValue::new(&mut offset_value)
                    .suffix(units_suffix)
                    .speed(speed)
                    .range(0.0..=max_range))
                    .changed() 
                {
                    let offset_mm = offset_value * conversion_factor;
                    app.display_manager.set_quadrant_offset_magnitude(offset_mm);
                    app.display_manager.update_layer_positions(&mut app.layer_manager);
                }
                
                ui.separator();
                
                // PNG Export section for quadrant view
                if ui.button("📷 Export Layers as PNG").clicked() {
                    let logger_state = app.logger_state.clone();
                    let log_colors = app.log_colors.clone();
                    let logger = ReactiveEventLogger::with_colors(&logger_state, &log_colors);
                    crate::ui::orientation_panel::export_quadrant_layers_to_png(app, &logger);
                }
            }
            
            ui.separator();
            
            // Flip Top/Bottom button - toggles layer visibility
            let flip_text = if app.display_manager.showing_top { "🔄 Flip to Bottom (F)" } else { "🔄 Flip to Top (F)" };
            if ui.button(flip_text).clicked() {
                app.display_manager.showing_top = !app.display_manager.showing_top;
                
                // Auto-toggle layer visibility based on flip state
                for layer_type in crate::layer_operations::LayerType::all() {
                    if let Some(layer_info) = app.layer_manager.layers.get_mut(&layer_type) {
                        match layer_type {
                            crate::layer_operations::LayerType::TopCopper |
                            crate::layer_operations::LayerType::TopSilk |
                            crate::layer_operations::LayerType::TopSoldermask |
                            crate::layer_operations::LayerType::TopPaste => {
                                layer_info.visible = app.display_manager.showing_top;
                            },
                            crate::layer_operations::LayerType::BottomCopper |
                            crate::layer_operations::LayerType::BottomSilk |
                            crate::layer_operations::LayerType::BottomSoldermask |
                            crate::layer_operations::LayerType::BottomPaste => {
                                layer_info.visible = !app.display_manager.showing_top;
                            },
                            crate::layer_operations::LayerType::MechanicalOutline => {
                                // Leave outline visibility unchanged
                            }
                        }
                    }
                }
                
                app.layer_manager.mark_coordinates_dirty();
            }
            
            // Rotate button
            if ui.button("🔄 Rotate (R)").clicked() {
                // Rotate 90 degrees clockwise
                app.rotation_degrees = (app.rotation_degrees + 90.0) % 360.0;
                app.needs_initial_view = true;
                
                let logger_state = app.logger_state.clone();
                let log_colors = app.log_colors.clone();
                let logger = ReactiveEventLogger::with_colors(&logger_state, &log_colors);
                logger.log_custom(
                    crate::project::constants::LOG_TYPE_ROTATION, 
                    &format!("Rotated to {:.0}°", app.rotation_degrees)
                );
            }
            
            // X Mirror button
            let x_mirror_text = if app.display_manager.mirroring.x { "↔️ X Mirror ✓" } else { "↔️ X Mirror" };
            if ui.button(x_mirror_text).clicked() {
                app.display_manager.mirroring.x = !app.display_manager.mirroring.x;
                
                // Recenter view after mirror
                app.display_manager.center_offset = crate::display::VectorOffset { x: 0.0, y: 0.0 };
                app.display_manager.design_offset = crate::display::VectorOffset { x: 0.0, y: 0.0 };
                app.needs_initial_view = true;
                
                let logger_state = app.logger_state.clone();
                let log_colors = app.log_colors.clone();
                let logger = ReactiveEventLogger::with_colors(&logger_state, &log_colors);
                logger.log_custom(
                    crate::project::constants::LOG_TYPE_MIRROR,
                    &format!("X mirroring {}", if app.display_manager.mirroring.x { "enabled" } else { "disabled" })
                );
            }
            
            // Y Mirror button
            let y_mirror_text = if app.display_manager.mirroring.y { "↕️ Y Mirror ✓" } else { "↕️ Y Mirror" };
            if ui.button(y_mirror_text).clicked() {
                app.display_manager.mirroring.y = !app.display_manager.mirroring.y;
                
                // Recenter view after mirror
                app.display_manager.center_offset = crate::display::VectorOffset { x: 0.0, y: 0.0 };
                app.display_manager.design_offset = crate::display::VectorOffset { x: 0.0, y: 0.0 };
                app.needs_initial_view = true;
                
                let logger_state = app.logger_state.clone();
                let log_colors = app.log_colors.clone();
                let logger = ReactiveEventLogger::with_colors(&logger_state, &log_colors);
                logger.log_custom(
                    crate::project::constants::LOG_TYPE_MIRROR,
                    &format!("Y mirroring {}", if app.display_manager.mirroring.y { "enabled" } else { "disabled" })
                );
            }
            
            ui.separator();
            
            // Grid spacing dropdown
            ui.label("Grid:");
            let grid_spacings_mils = [100.0, 50.0, 25.0, 10.0, 5.0, 2.0, 1.0];
            let grid_spacings_mm = [2.54, 1.27, 0.635, 0.254, 0.127, 0.0508, 0.0254];
            
            let (spacings, _unit_name) = if app.global_units_mils {
                (&grid_spacings_mils[..], "mils")
            } else {
                (&grid_spacings_mm[..], "mm")
            };
            
            // Find current selection
            let mut current_spacing_display = "Custom".to_string();
            for &spacing in spacings {
                let spacing_mm = if app.global_units_mils { spacing * 0.0254 } else { spacing };
                if (app.grid_settings.spacing_mm - spacing_mm).abs() < 0.001 {
                    current_spacing_display = if app.global_units_mils {
                        format!("{} mils", spacing as i32)
                    } else {
                        format!("{:.3} mm", spacing)
                    };
                    break;
                }
            }
            
            egui::ComboBox::from_label("")
                .selected_text(current_spacing_display)
                .show_ui(ui, |ui| {
                    for &spacing in spacings {
                        let spacing_mm = if app.global_units_mils { spacing * 0.0254 } else { spacing };
                        let label = if app.global_units_mils {
                            format!("{} mils", spacing as i32)
                        } else {
                            format!("{:.3} mm", spacing)
                        };
                        if ui.selectable_label(false, label).clicked() {
                            app.grid_settings.spacing_mm = spacing_mm;
                        }
                    }
                });
            
            ui.separator();
            
            // Grid dot size slider
            ui.label("Dot Size:");
            ui.add(egui::Slider::new(&mut app.grid_settings.dot_size, 0.5..=5.0).suffix("px"));
        });
        ui.separator();
        
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
            app.display_manager.center_offset = crate::display::VectorOffset { x: 0.0, y: 0.0 };
            app.display_manager.design_offset = crate::display::VectorOffset { x: 0.0, y: 0.0 };
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
            
            // Force everything to use viewport center as (0,0)
            let viewport_center = viewport.center();
            app.ui_state.origin_screen_pos = viewport_center;
            app.ui_state.center_screen_pos = viewport_center;
            
            // Override cursor coordinates to be relative to viewport center
            if let Some(cursor_pos) = ui.input(|i| i.pointer.hover_pos()) {
                let relative_x = (cursor_pos.x - viewport_center.x) / app.view_state.scale;
                let relative_y = -(cursor_pos.y - viewport_center.y) / app.view_state.scale; // Flip Y
                app.ui_state.cursor_gerber_coords = Some(nalgebra::Point2::new(relative_x as f64, relative_y as f64));
            }
            
            // Handle origin setting click
            if app.setting_origin_mode && response.clicked() {
                if let Some(gerber_coords) = app.ui_state.cursor_gerber_coords {
                    // Set the design offset to make this point the new (0,0)
                    app.display_manager.design_offset = crate::display::VectorOffset {
                        x: gerber_coords.x,
                        y: gerber_coords.y,
                    };
                    app.setting_origin_mode = false; // Exit origin setting mode
                    app.needs_initial_view = true;
                }
            }
        }

        let painter = ui.painter().with_clip_rect(viewport);
        
        // Draw grid if enabled (before other elements so it appears underneath)
        crate::display::draw_grid(&painter, &viewport, &app.view_state, &app.grid_settings);
        
        // Draw quadrant axes if quadrant view is enabled
        if app.display_manager.quadrant_view_enabled {
            draw_quadrant_axes(&painter, &viewport, &app.view_state, app.ui_state.center_screen_pos);
        }
        
        // In quadrant view, show blue crosshair at viewport center instead of origin
        if app.display_manager.quadrant_view_enabled {
            draw_crosshair(&painter, app.ui_state.center_screen_pos, Color32::BLUE);
        } else {
            draw_crosshair(&painter, app.ui_state.origin_screen_pos, Color32::BLUE);
            draw_crosshair(&painter, app.ui_state.center_screen_pos, Color32::LIGHT_GRAY);
        }

        // Get mechanical outline layer for quadrant view
        let mechanical_outline_layer = if app.display_manager.quadrant_view_enabled {
            app.layer_manager.layers.get(&crate::layer_operations::LayerType::MechanicalOutline)
                .and_then(|info| if info.visible { info.gerber_layer.as_ref() } else { None })
        } else {
            None
        };

        // Render all visible layers based on showing_top
        for layer_type in crate::layer_operations::LayerType::all() {
            if let Some(layer_info) = app.layer_manager.layers.get(&layer_type) {
                if layer_info.visible {
                    // Skip mechanical outline in quadrant view (it will be rendered with each layer)
                    if app.display_manager.quadrant_view_enabled && layer_type == crate::layer_operations::LayerType::MechanicalOutline {
                        continue;
                    }
                    
                    // Always render visible layers - manual control overrides flip state
                    
                    // Use the layer's specific gerber data if available, otherwise fall back to demo
                    let gerber_to_render = layer_info.gerber_layer.as_ref()
                        .unwrap_or(&app.gerber_layer);
                    
                    // Get quadrant offset for this layer type
                    let quadrant_offset = app.display_manager.get_quadrant_offset(&layer_type);
                    
                    // The key is to offset from center_offset (which positions the viewport center)
                    // rather than from design_offset
                    let combined_offset = if app.display_manager.quadrant_view_enabled {
                        // When quadrant view is enabled, position relative to center_offset
                        crate::display::VectorOffset {
                            x: app.display_manager.center_offset.x + quadrant_offset.x,
                            y: app.display_manager.center_offset.y + quadrant_offset.y,
                        }
                    } else {
                        // Normal mode: use design offset
                        app.display_manager.design_offset.clone()
                    };
                    
                    // Create render configuration
                    let config = RenderConfiguration::default();
                    
                    // Create transform
                    let transform = GerberTransform {
                        rotation: app.rotation_degrees.to_radians(),
                        mirroring: app.display_manager.mirroring.clone().into(),
                        origin: app.display_manager.center_offset.clone().into(),
                        offset: combined_offset.clone().into(),
                        scale: 1.0,
                    };
                    
                    // Render the main layer
                    GerberRenderer::default().paint_layer(
                        &painter,
                        app.view_state,
                        gerber_to_render,
                        layer_info.color,
                        &config,
                        &transform,
                    );
                    
                    // In quadrant view, also render mechanical outline with this layer
                    if app.display_manager.quadrant_view_enabled {
                        if let Some(mechanical_layer) = mechanical_outline_layer {
                            // Create transform for mechanical outline
                            let mechanical_transform = GerberTransform {
                                rotation: app.rotation_degrees.to_radians(),
                                mirroring: app.display_manager.mirroring.clone().into(),
                                origin: app.display_manager.center_offset.clone().into(),
                                offset: combined_offset.into(),
                                scale: 1.0,
                            };
                            
                            GerberRenderer::default().paint_layer(
                                &painter,
                                app.view_state,
                                mechanical_layer,
                                crate::layer_operations::LayerType::MechanicalOutline.color(),
                                &config,
                                &mechanical_transform,
                            );
                        }
                    }
                }
            }
        }

        let screen_radius = MARKER_RADIUS * app.view_state.scale;

        // Only show design offset marker if it's not at (0,0) to avoid visual clutter
        let design_offset = &app.display_manager.design_offset;
        if design_offset.x != 0.0 || design_offset.y != 0.0 {
            let design_offset_screen_position = app.view_state.gerber_to_screen_coords(Vector2::from(design_offset.clone()).to_position().to_point2());
            draw_arrow(&painter, design_offset_screen_position, app.ui_state.origin_screen_pos, Color32::ORANGE);
            draw_marker(&painter, design_offset_screen_position, Color32::ORANGE, Color32::YELLOW, screen_radius);
        }

        // Purple dot should match the blue crosshair position
        let purple_dot_pos = if app.display_manager.quadrant_view_enabled {
            app.ui_state.center_screen_pos  // In quadrant view, use center
        } else {
            app.ui_state.origin_screen_pos  // In normal view, use origin
        };
        draw_marker(&painter, purple_dot_pos, Color32::PURPLE, Color32::MAGENTA, screen_radius);
        
        // Render corner overlay shapes (rounded corners)
        if !app.drc_manager.corner_overlay_shapes.is_empty() {
            // Use a different color for the overlay (bright green for visibility)
            let overlay_color = Color32::from_rgb(0, 255, 0); // Bright green
            
            for shape in &app.drc_manager.corner_overlay_shapes {
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
                        vertex_pos = Position { x: rotated_x, y: rotated_y };
                    }
                    
                    // Apply mirroring if any
                    if app.display_manager.mirroring.x {
                        vertex_pos = vertex_pos.invert_x();
                    }
                    if app.display_manager.mirroring.y {
                        vertex_pos = vertex_pos.invert_y();
                    }
                    
                    // Apply center and design offsets
                    let origin = Vector2::from(app.display_manager.center_offset.clone()) - Vector2::from(app.display_manager.design_offset.clone());
                    vertex_pos = vertex_pos + origin.to_position();
                    
                    let vertex_screen = app.view_state.gerber_to_screen_coords(vertex_pos.to_point2());
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
        for violation in &app.drc_manager.violations {
            let violation_pos = Position::new(violation.x as f64, violation.y as f64);
            
            // Apply the same transformation pipeline as GerberRenderer::paint_layer()
            let mut transformed_pos = violation_pos;
            
            // Apply rotation if any
            if app.rotation_degrees != 0.0 {
                let rotation_radians = app.rotation_degrees.to_radians();
                let (sin_theta, cos_theta) = (rotation_radians.sin(), rotation_radians.cos());
                let rotated_x = transformed_pos.x * cos_theta as f64 - transformed_pos.y * sin_theta as f64;
                let rotated_y = transformed_pos.x * sin_theta as f64 + transformed_pos.y * cos_theta as f64;
                transformed_pos = Position::new(rotated_x, rotated_y);
            }
            
            // Apply mirroring if any
            if app.display_manager.mirroring.x { // X mirroring
                transformed_pos = transformed_pos.invert_x();
            }
            if app.display_manager.mirroring.y { // Y mirroring
                transformed_pos = transformed_pos.invert_y();
            }
            
            // Apply center and design offsets
            let origin = Vector2::from(app.display_manager.center_offset.clone()) - Vector2::from(app.display_manager.design_offset.clone());
            transformed_pos = transformed_pos + origin.to_position();
            
            let screen_pos = app.view_state.gerber_to_screen_coords(transformed_pos.to_point2());
            
            // All markers now represent trace areas (1 per trace)
            let base_size = 3.0; // Small but visible markers
            let marker_size = base_size * app.view_state.scale.max(0.5); // Scale with zoom but not too small
            let color = Color32::RED;
            
            draw_violation_marker(&painter, screen_pos, marker_size, color);
        }
        
        // Draw board dimensions at the bottom
        if let Some(layer_info) = app.layer_manager.layers.get(&crate::layer_operations::LayerType::MechanicalOutline) {
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
                let origin = Vector2::from(app.display_manager.center_offset.clone()) - Vector2::from(app.display_manager.design_offset.clone());
                let adjusted_pos = Position::new(
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

pub struct TabViewer<'a> {
    pub app: &'a mut DemoLensApp,
}

impl<'a> egui_dock::TabViewer for TabViewer<'a> {
    type Tab = Tab;

    fn title(&mut self, tab: &mut Self::Tab) -> egui::WidgetText {
        tab.title().into()
    }

    fn ui(&mut self, ui: &mut egui::Ui, tab: &mut Self::Tab) {
        let mut params = TabParams {
            app: self.app,
        };
        tab.content(ui, &mut params);
    }
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

/// Draw quadrant axes when quadrant view is enabled
fn draw_quadrant_axes(painter: &Painter, viewport: &Rect, _view_state: &ViewState, center_screen_pos: Pos2) {
    let stroke = Stroke::new(2.0, Color32::from_rgba_unmultiplied(100, 100, 100, 150));
    
    // Use the viewport center (white crosshair) as the origin for quadrant axes
    let origin_screen = center_screen_pos;
    
    // Draw vertical axis
    if origin_screen.x >= viewport.min.x && origin_screen.x <= viewport.max.x {
        painter.line_segment(
            [
                Pos2::new(origin_screen.x, viewport.min.y),
                Pos2::new(origin_screen.x, viewport.max.y)
            ],
            stroke
        );
    }
    
    // Draw horizontal axis
    if origin_screen.y >= viewport.min.y && origin_screen.y <= viewport.max.y {
        painter.line_segment(
            [
                Pos2::new(viewport.min.x, origin_screen.y),
                Pos2::new(viewport.max.x, origin_screen.y)
            ],
            stroke
        );
    }
    
    // Add quadrant labels
    let label_offset = 20.0;
    let font_id = egui::FontId::default();
    let label_color = Color32::from_rgba_unmultiplied(150, 150, 150, 200);
    
    // Only draw labels if axes are visible
    if origin_screen.x > viewport.min.x + label_offset * 2.0 && 
       origin_screen.x < viewport.max.x - label_offset * 2.0 &&
       origin_screen.y > viewport.min.y + label_offset * 2.0 &&
       origin_screen.y < viewport.max.y - label_offset * 2.0 {
        
        // Quadrant 1 (top-right): Copper
        painter.text(
            origin_screen + Vec2::new(label_offset, -label_offset),
            egui::Align2::LEFT_BOTTOM,
            "Copper",
            font_id.clone(),
            label_color,
        );
        
        // Quadrant 2 (top-left): Silkscreen
        painter.text(
            origin_screen + Vec2::new(-label_offset, -label_offset),
            egui::Align2::RIGHT_BOTTOM,
            "Silkscreen",
            font_id.clone(),
            label_color,
        );
        
        // Quadrant 3 (bottom-left): Soldermask
        painter.text(
            origin_screen + Vec2::new(-label_offset, label_offset),
            egui::Align2::RIGHT_TOP,
            "Soldermask",
            font_id.clone(),
            label_color,
        );
        
        // Quadrant 4 (bottom-right): Paste
        painter.text(
            origin_screen + Vec2::new(label_offset, label_offset),
            egui::Align2::LEFT_TOP,
            "Paste",
            font_id,
            label_color,
        );
    }
}