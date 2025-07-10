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

const MARKER_RADIUS: f32 = 6.0;

/// Define the tabs for the DockArea
#[derive(Clone, Serialize, Deserialize)]
pub enum TabKind {
    ViewSettings,
    DRC,
    GerberView,
    EventLog,
    Project,
    Settings,
    BOM,
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
            TabKind::BOM => "BOM".to_string(),
        }
    }

    pub fn content(&self, ui: &mut egui::Ui, params: &mut TabParams<'_>) {
        match self.kind {
            TabKind::ViewSettings => {
                ui.vertical(|ui| {
                    let logger_state_clone = params.app.logger_state.clone();
                    let log_colors_clone = params.app.log_colors.clone();
                    
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
            TabKind::BOM => {
                let logger_state_clone = params.app.logger_state.clone();
                let log_colors_clone = params.app.log_colors.clone();
                ui::show_bom_panel(ui, params.app, &logger_state_clone, &log_colors_clone);
            }
        }
    }

    fn render_gerber_view(&self, ui: &mut egui::Ui, app: &mut DemoLensApp) {
        // Render top controls
        render_controls(ui, app);
        ui.separator();
        
        // Set up viewport and handle interactions
        let (viewport, response) = setup_viewport(ui, app);
        handle_viewport_interactions(ui, app, &viewport, &response);
        
        // Render the gerber layers and overlays
        render_gerber_content(ui, app, &viewport);
    }
}

fn render_controls(ui: &mut egui::Ui, app: &mut DemoLensApp) {
    ui.horizontal(|ui| {
        render_quadrant_controls(ui, app);
        ui.separator();
        render_layer_controls(ui, app);
        ui.separator();
        render_transform_controls(ui, app);
        ui.separator();
        render_grid_controls(ui, app);
    });
}

fn render_quadrant_controls(ui: &mut egui::Ui, app: &mut DemoLensApp) {
    if ui.checkbox(&mut app.display_manager.quadrant_view_enabled, "Quadrant View").clicked() {
        app.display_manager.update_layer_positions(&mut app.layer_manager);
        app.layer_manager.mark_coordinates_dirty();
        app.needs_initial_view = true;
    }
    
    if app.display_manager.quadrant_view_enabled {
        ui.separator();
        ui.label("Offset:");
        
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
        
        if ui.button("📷 Export Layers as PNG").clicked() {
            let logger_state = app.logger_state.clone();
            let log_colors = app.log_colors.clone();
            let logger = ReactiveEventLogger::with_colors(&logger_state, &log_colors);
            crate::ui::orientation_panel::export_quadrant_layers_to_png(app, &logger);
        }
    }
}

fn render_layer_controls(ui: &mut egui::Ui, app: &mut DemoLensApp) {
    let flip_text = if app.display_manager.showing_top { "🔄 Flip to Bottom (F)" } else { "🔄 Flip to Top (F)" };
    if ui.button(flip_text).clicked() {
        app.display_manager.showing_top = !app.display_manager.showing_top;
        
        // Auto-toggle layer visibility based on flip state (using ECS)
        for layer_type in crate::layer_operations::LayerType::all() {
            let visible = match layer_type {
                crate::layer_operations::LayerType::TopCopper |
                crate::layer_operations::LayerType::TopSilk |
                crate::layer_operations::LayerType::TopSoldermask |
                crate::layer_operations::LayerType::TopPaste => {
                    app.display_manager.showing_top
                },
                crate::layer_operations::LayerType::BottomCopper |
                crate::layer_operations::LayerType::BottomSilk |
                crate::layer_operations::LayerType::BottomSoldermask |
                crate::layer_operations::LayerType::BottomPaste => {
                    !app.display_manager.showing_top
                },
                crate::layer_operations::LayerType::MechanicalOutline => {
                    // Leave outline visibility unchanged, get current state from ECS
                    app.layer_manager.get_layer_visibility(&app.ecs_world, &layer_type)
                }
            };
            app.layer_manager.set_layer_visibility_ecs(&mut app.ecs_world, &layer_type, visible);
        }
        
        app.layer_manager.mark_coordinates_dirty();
    }
}

fn render_transform_controls(ui: &mut egui::Ui, app: &mut DemoLensApp) {
    // Rotate button
    if ui.button("🔄 Rotate (R)").clicked() {
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
    
    // ECS Rendering toggle button
    if ui.button(if app.use_ecs_rendering { "🔥 ECS Mode" } else { "🔧 Legacy Mode" }).clicked() {
        app.use_ecs_rendering = !app.use_ecs_rendering;
        
        let logger_state = app.logger_state.clone();
        let log_colors = app.log_colors.clone();
        let logger = ReactiveEventLogger::with_colors(&logger_state, &log_colors);
        logger.log_custom(
            crate::project::constants::LOG_TYPE_ROTATION, 
            &format!("Switched to {} rendering", if app.use_ecs_rendering { "ECS" } else { "Legacy" })
        );
    }
    
    // Mirror buttons
    let x_mirror_text = if app.display_manager.mirroring.x { "↔️ X Mirror ✓" } else { "↔️ X Mirror" };
    if ui.button(x_mirror_text).clicked() {
        app.display_manager.mirroring.x = !app.display_manager.mirroring.x;
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
    
    let y_mirror_text = if app.display_manager.mirroring.y { "↕️ Y Mirror ✓" } else { "↕️ Y Mirror" };
    if ui.button(y_mirror_text).clicked() {
        app.display_manager.mirroring.y = !app.display_manager.mirroring.y;
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
    
    // Origin setting button
    let origin_set = app.display_manager.design_offset.x != 0.0 || app.display_manager.design_offset.y != 0.0;
    if origin_set {
        if ui.button("🎯 Reset Origin").clicked() {
            app.display_manager.design_offset = crate::display::VectorOffset { x: 0.0, y: 0.0 };
            
            let logger_state = app.logger_state.clone();
            let log_colors = app.log_colors.clone();
            let logger = ReactiveEventLogger::with_colors(&logger_state, &log_colors);
            logger.log_info("Reset origin to (0, 0)");
        }
    } else {
        if ui.button("🎯 Set Origin").clicked() {
            app.setting_origin_mode = true;
            
            let logger_state = app.logger_state.clone();
            let log_colors = app.log_colors.clone();
            let logger = ReactiveEventLogger::with_colors(&logger_state, &log_colors);
            logger.log_info("Click on the PCB to set the origin");
        }
    }
}

fn render_grid_controls(ui: &mut egui::Ui, app: &mut DemoLensApp) {
    ui.label("Grid:");
    let grid_spacings_mils = [100.0, 50.0, 25.0, 10.0, 5.0, 2.0, 1.0];
    let grid_spacings_mm = [2.54, 1.27, 0.635, 0.254, 0.127, 0.0508, 0.0254];
    
    let spacings = if app.global_units_mils {
        &grid_spacings_mils[..]
    } else {
        &grid_spacings_mm[..]
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
}

fn setup_viewport(ui: &mut egui::Ui, app: &mut DemoLensApp) -> (Rect, egui::Response) {
    ui.ctx().request_repaint();
    
    let available_size = ui.available_size();
    let size = egui::Vec2::new(
        available_size.x.max(100.0),
        available_size.y.max(100.0)
    );
    
    let response = ui.allocate_response(size, egui::Sense::click_and_drag());
    let viewport = response.rect;
    
    // Handle double-click to center
    if response.double_clicked() {
        app.display_manager.center_offset = crate::display::VectorOffset { x: 0.0, y: 0.0 };
        app.display_manager.design_offset = crate::display::VectorOffset { x: 0.0, y: 0.0 };
        app.needs_initial_view = true;
    }
    
    (viewport, response)
}

fn handle_viewport_interactions(ui: &mut egui::Ui, app: &mut DemoLensApp, viewport: &Rect, response: &egui::Response) {
    let mouse_pos_screen = ui.input(|i| i.pointer.hover_pos());
    
    // Handle zoom window
    handle_zoom_window(ui, app, viewport, mouse_pos_screen, response);
    
    // Update UI state if not dragging zoom window
    if !app.zoom_window_dragging {
        app.ui_state.update(ui, viewport, response, &mut app.view_state);
        
        let viewport_center = viewport.center();
        app.ui_state.origin_screen_pos = viewport_center;
        app.ui_state.center_screen_pos = viewport_center;
        
        // Update cursor coordinates using raw transform (not affected by design_offset)
        if let Some(cursor_pos) = ui.input(|i| i.pointer.hover_pos()) {
            // Use the original gerber coordinate system for origin setting
            let raw_gerber_pos = app.view_state.screen_to_gerber_coords(cursor_pos);
            app.ui_state.cursor_gerber_coords = Some(raw_gerber_pos);
        }
        
        // Show visual feedback when in origin setting mode
        if app.setting_origin_mode {
            ui.ctx().set_cursor_icon(egui::CursorIcon::Crosshair);
            
            // Draw preview text at cursor
            if let Some(mouse_pos) = ui.input(|i| i.pointer.hover_pos()) {
                let painter = ui.painter();
                painter.text(
                    mouse_pos + Vec2::new(20.0, -20.0),
                    egui::Align2::LEFT_BOTTOM,
                    "Click to set origin",
                    egui::FontId::default(),
                    Color32::YELLOW,
                );
            }
        }
        
        // Handle origin setting
        if app.setting_origin_mode && response.clicked() {
            if let Some(gerber_coords) = app.ui_state.cursor_gerber_coords {
                app.display_manager.design_offset = crate::display::VectorOffset {
                    x: gerber_coords.x,
                    y: gerber_coords.y,
                };
                app.setting_origin_mode = false;
                
                let logger_state = app.logger_state.clone();
                let log_colors = app.log_colors.clone();
                let logger = ReactiveEventLogger::with_colors(&logger_state, &log_colors);
                logger.log_info(&format!("Set origin to ({:.2}, {:.2}) mm", gerber_coords.x, gerber_coords.y));
            }
        }
    }
}

fn handle_zoom_window(ui: &mut egui::Ui, app: &mut DemoLensApp, viewport: &Rect, mouse_pos_screen: Option<Pos2>, response: &egui::Response) {
    let right_button = egui::PointerButton::Secondary;
    
    // Start zoom window
    if response.contains_pointer() {
        if ui.input(|i| i.pointer.button_pressed(right_button)) {
            if let Some(pos) = mouse_pos_screen {
                app.zoom_window_start = Some(pos);
                app.zoom_window_dragging = true;
            }
        }
    }
    
    // Complete zoom window
    if app.zoom_window_dragging && ui.input(|i| i.pointer.button_released(right_button)) {
        if let (Some(start), Some(end)) = (app.zoom_window_start, ui.input(|i| i.pointer.hover_pos())) {
            let zoom_rect = Rect::from_two_pos(start, end);
            
            if zoom_rect.width() > 10.0 && zoom_rect.height() > 10.0 {
                let gerber_start = app.view_state.screen_to_gerber_coords(zoom_rect.min);
                let gerber_end = app.view_state.screen_to_gerber_coords(zoom_rect.max);
                
                let gerber_width = (gerber_end.x - gerber_start.x).abs() as f32;
                let gerber_height = (gerber_end.y - gerber_start.y).abs() as f32;
                
                let scale_x = viewport.width() / gerber_width;
                let scale_y = viewport.height() / gerber_height;
                let new_scale = scale_x.min(scale_y) * 0.9;
                
                let gerber_center_x = (gerber_start.x + gerber_end.x) / 2.0;
                let gerber_center_y = (gerber_start.y + gerber_end.y) / 2.0;
                
                app.view_state.scale = new_scale;
                
                let viewport_center = viewport.center();
                app.view_state.translation = Vec2::new(
                    viewport_center.x - (gerber_center_x * new_scale as f64) as f32,
                    viewport_center.y + (gerber_center_y * new_scale as f64) as f32
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
}

fn render_gerber_content(ui: &mut egui::Ui, app: &mut DemoLensApp, viewport: &Rect) {
    let painter = ui.painter_at(*viewport);
    painter.rect_filled(*viewport, 0.0, ui.visuals().extreme_bg_color);
    
    if app.needs_initial_view {
        app.reset_view(*viewport);
    }
    
    let painter = ui.painter().with_clip_rect(*viewport);
    
    // Draw grid
    crate::display::draw_grid(&painter, viewport, &app.view_state, &app.grid_settings);
    
    // Draw quadrant axes
    if app.display_manager.quadrant_view_enabled {
        draw_quadrant_axes(&painter, viewport, &app.view_state, app.ui_state.center_screen_pos);
    }
    
    // Draw crosshairs
    if app.display_manager.quadrant_view_enabled {
        draw_crosshair(&painter, app.ui_state.center_screen_pos, Color32::BLUE);
    } else {
        draw_crosshair(&painter, app.ui_state.origin_screen_pos, Color32::BLUE);
        draw_crosshair(&painter, app.ui_state.center_screen_pos, Color32::LIGHT_GRAY);
    }
    
    // Render layers
    if app.use_ecs_rendering {
        // ECS rendering path
        app.render_layers_ecs(&painter);
    } else {
        // Legacy rendering path
        render_layers_legacy(app, &painter);
    }
    
    // Render overlays
    render_overlays(app, &painter, viewport);
    
    // Render cursor info
    render_cursor_info(ui, app, &painter, viewport);
}

fn render_layers_legacy(app: &mut DemoLensApp, painter: &Painter) {
    // Get mechanical outline layer for quadrant view (using ECS)
    let mechanical_outline_layer = if app.display_manager.quadrant_view_enabled {
        app.layer_manager.get_layer_ecs(&app.ecs_world, &crate::layer_operations::LayerType::MechanicalOutline)
            .and_then(|(_entity, _layer_info, gerber_data, visibility)| {
                if visibility.visible { Some(&gerber_data.0) } else { None }
            })
    } else {
        None
    };
    
    // Render all visible layers (using ECS)
    for layer_type in crate::layer_operations::LayerType::all() {
        if let Some((_entity, _layer_info, gerber_data, visibility)) = app.layer_manager.get_layer_ecs(&app.ecs_world, &layer_type) {
            if visibility.visible {
                // Skip mechanical outline in quadrant view (it will be rendered with each layer)
                if app.display_manager.quadrant_view_enabled && layer_type == crate::layer_operations::LayerType::MechanicalOutline {
                    continue;
                }
                
                let gerber_to_render = &gerber_data.0;
                
                let quadrant_offset = app.display_manager.get_quadrant_offset(&layer_type);
                
                let combined_offset = if app.display_manager.quadrant_view_enabled {
                    crate::display::VectorOffset {
                        x: app.display_manager.center_offset.x + quadrant_offset.x,
                        y: app.display_manager.center_offset.y + quadrant_offset.y,
                    }
                } else {
                    app.display_manager.design_offset.clone()
                };
                
                let config = RenderConfiguration::default();
                let transform = GerberTransform {
                    rotation: app.rotation_degrees.to_radians(),
                    mirroring: app.display_manager.mirroring.clone().into(),
                    origin: app.display_manager.center_offset.clone().into(),
                    offset: combined_offset.clone().into(),
                    scale: 1.0,
                };
                
                // Get layer color from ECS render properties
                let layer_color = app.layer_manager.get_layer_render_properties_ecs(&app.ecs_world, &layer_type)
                    .map(|props| props.color)
                    .unwrap_or(layer_type.color());
                
                // Render main layer
                GerberRenderer::default().paint_layer(
                    painter,
                    app.view_state,
                    gerber_to_render,
                    layer_color,
                    &config,
                    &transform,
                );
                
                // Render mechanical outline in quadrant view
                if app.display_manager.quadrant_view_enabled {
                    if let Some(mechanical_layer) = mechanical_outline_layer {
                        let mechanical_transform = GerberTransform {
                            rotation: app.rotation_degrees.to_radians(),
                            mirroring: app.display_manager.mirroring.clone().into(),
                            origin: app.display_manager.center_offset.clone().into(),
                            offset: combined_offset.into(),
                            scale: 1.0,
                        };
                        
                        GerberRenderer::default().paint_layer(
                            painter,
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
}

fn render_overlays(app: &mut DemoLensApp, painter: &Painter, viewport: &Rect) {
    let screen_radius = MARKER_RADIUS * app.view_state.scale;
    
    // Design offset marker
    let design_offset = &app.display_manager.design_offset;
    if design_offset.x != 0.0 || design_offset.y != 0.0 {
        let design_offset_screen_position = app.view_state.gerber_to_screen_coords(Vector2::from(design_offset.clone()).to_position().to_point2());
        draw_arrow(painter, design_offset_screen_position, app.ui_state.origin_screen_pos, Color32::ORANGE);
        draw_marker(painter, design_offset_screen_position, Color32::ORANGE, Color32::YELLOW, screen_radius);
    }
    
    // Purple dot
    let purple_dot_pos = if app.display_manager.quadrant_view_enabled {
        app.ui_state.center_screen_pos
    } else {
        app.ui_state.origin_screen_pos
    };
    draw_marker(painter, purple_dot_pos, Color32::PURPLE, Color32::MAGENTA, screen_radius);
    
    // Corner overlay shapes
    render_corner_overlays(app, painter);
    
    // DRC violations
    render_drc_violations(app, painter);
    
    // Board dimensions
    render_board_dimensions(app, painter, viewport);
    
    // Zoom window
    render_zoom_window(app, painter);
}

fn render_corner_overlays(app: &mut DemoLensApp, painter: &Painter) {
    if !app.drc_manager.corner_overlay_shapes.is_empty() {
        let overlay_color = Color32::from_rgb(0, 255, 0);
        
        for shape in &app.drc_manager.corner_overlay_shapes {
            let mut transformed_vertices = Vec::new();
            
            for point in &shape.points {
                let mut vertex_pos = *point;
                
                // Apply rotation
                if app.rotation_degrees != 0.0 {
                    let rotation_radians = app.rotation_degrees.to_radians();
                    let (sin_theta, cos_theta) = (rotation_radians.sin(), rotation_radians.cos());
                    
                    let rotated_x = vertex_pos.x * cos_theta as f64 - vertex_pos.y * sin_theta as f64;
                    let rotated_y = vertex_pos.x * sin_theta as f64 + vertex_pos.y * cos_theta as f64;
                    vertex_pos = Position { x: rotated_x, y: rotated_y };
                }
                
                // Apply mirroring
                if app.display_manager.mirroring.x {
                    vertex_pos = vertex_pos.invert_x();
                }
                if app.display_manager.mirroring.y {
                    vertex_pos = vertex_pos.invert_y();
                }
                
                // Apply offsets
                let origin = Vector2::from(app.display_manager.center_offset.clone()) - Vector2::from(app.display_manager.design_offset.clone());
                vertex_pos = vertex_pos + origin.to_position();
                
                let vertex_screen = app.view_state.gerber_to_screen_coords(vertex_pos.to_point2());
                transformed_vertices.push(vertex_screen);
            }
            
            if transformed_vertices.len() >= 3 {
                painter.add(egui::Shape::convex_polygon(
                    transformed_vertices,
                    overlay_color,
                    Stroke::NONE
                ));
            }
        }
    }
}

fn render_drc_violations(app: &mut DemoLensApp, painter: &Painter) {
    for violation in &app.drc_manager.violations {
        let violation_pos = Position::new(violation.x as f64, violation.y as f64);
        let mut transformed_pos = violation_pos;
        
        // Apply rotation
        if app.rotation_degrees != 0.0 {
            let rotation_radians = app.rotation_degrees.to_radians();
            let (sin_theta, cos_theta) = (rotation_radians.sin(), rotation_radians.cos());
            let rotated_x = transformed_pos.x * cos_theta as f64 - transformed_pos.y * sin_theta as f64;
            let rotated_y = transformed_pos.x * sin_theta as f64 + transformed_pos.y * cos_theta as f64;
            transformed_pos = Position::new(rotated_x, rotated_y);
        }
        
        // Apply mirroring
        if app.display_manager.mirroring.x {
            transformed_pos = transformed_pos.invert_x();
        }
        if app.display_manager.mirroring.y {
            transformed_pos = transformed_pos.invert_y();
        }
        
        // Apply offsets
        let origin = Vector2::from(app.display_manager.center_offset.clone()) - Vector2::from(app.display_manager.design_offset.clone());
        transformed_pos = transformed_pos + origin.to_position();
        
        let screen_pos = app.view_state.gerber_to_screen_coords(transformed_pos.to_point2());
        
        let base_size = 3.0;
        let marker_size = base_size * app.view_state.scale.max(0.5);
        let color = Color32::RED;
        
        draw_violation_marker(painter, screen_pos, marker_size, color);
    }
}

fn render_board_dimensions(app: &mut DemoLensApp, painter: &Painter, viewport: &Rect) {
    if let Some((_entity, _layer_info, gerber_data, _visibility)) = app.layer_manager.get_layer_ecs(&app.ecs_world, &crate::layer_operations::LayerType::MechanicalOutline) {
        let bbox = gerber_data.0.bounding_box();
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

fn render_zoom_window(app: &mut DemoLensApp, painter: &Painter) {
    if app.zoom_window_dragging {
        if let (Some(start), Some(current)) = (app.zoom_window_start, painter.ctx().input(|i| i.pointer.hover_pos())) {
            let zoom_rect = Rect::from_two_pos(start, current);
            
            // Draw semi-transparent fill
            painter.rect_filled(
                zoom_rect,
                0.0,
                Color32::from_rgba_unmultiplied(100, 150, 255, 50)
            );
            
            // Draw border
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
        }
    }
}

fn render_cursor_info(ui: &mut egui::Ui, app: &mut DemoLensApp, painter: &Painter, viewport: &Rect) {
    let mouse_pos_screen = ui.input(|i| i.pointer.hover_pos());
    
    if let Some(mouse_screen_pos) = mouse_pos_screen {
        if viewport.contains(mouse_screen_pos) {
            let gerber_pos = app.view_state.screen_to_gerber_coords(mouse_screen_pos);
            
            // Apply the design_offset as a simple coordinate offset for display
            // The design_offset is where we want (0,0) to be, so we subtract it from current position
            let adjusted_pos = Position::new(
                gerber_pos.x - app.display_manager.design_offset.x,
                gerber_pos.y - app.display_manager.design_offset.y
            );
            
            let cursor_text = if app.global_units_mils {
                let x_mils = adjusted_pos.x / 0.0254;
                let y_mils = adjusted_pos.y / 0.0254;
                format!("({:.0}, {:.0}) mils", x_mils, y_mils)
            } else {
                format!("({:.2}, {:.2}) mm", adjusted_pos.x, adjusted_pos.y)
            };
            
            
            let text_offset = Vec2::new(15.0, -15.0);
            let cursor_text_pos = mouse_screen_pos + text_offset;
            
            // Draw background
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
            
            // Draw text
            painter.text(
                cursor_text_pos,
                egui::Align2::LEFT_TOP,
                cursor_text,
                egui::FontId::monospace(12.0),
                Color32::WHITE,
            );
            
            // Draw crosshair
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
    
    // Unit display
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
    
    // Draw vertical axis
    if center_screen_pos.x >= viewport.min.x && center_screen_pos.x <= viewport.max.x {
        painter.line_segment(
            [
                Pos2::new(center_screen_pos.x, viewport.min.y),
                Pos2::new(center_screen_pos.x, viewport.max.y)
            ],
            stroke
        );
    }
    
    // Draw horizontal axis
    if center_screen_pos.y >= viewport.min.y && center_screen_pos.y <= viewport.max.y {
        painter.line_segment(
            [
                Pos2::new(viewport.min.x, center_screen_pos.y),
                Pos2::new(viewport.max.x, center_screen_pos.y)
            ],
            stroke
        );
    }
    
    // Add quadrant labels
    let label_offset = 20.0;
    let font_id = egui::FontId::default();
    let label_color = Color32::from_rgba_unmultiplied(150, 150, 150, 200);
    
    if center_screen_pos.x > viewport.min.x + label_offset * 2.0 && 
       center_screen_pos.x < viewport.max.x - label_offset * 2.0 &&
       center_screen_pos.y > viewport.min.y + label_offset * 2.0 &&
       center_screen_pos.y < viewport.max.y - label_offset * 2.0 {
        
        painter.text(
            center_screen_pos + Vec2::new(label_offset, -label_offset),
            egui::Align2::LEFT_BOTTOM,
            "Copper",
            font_id.clone(),
            label_color,
        );
        
        painter.text(
            center_screen_pos + Vec2::new(-label_offset, -label_offset),
            egui::Align2::RIGHT_BOTTOM,
            "Silkscreen",
            font_id.clone(),
            label_color,
        );
        
        painter.text(
            center_screen_pos + Vec2::new(-label_offset, label_offset),
            egui::Align2::RIGHT_TOP,
            "Soldermask",
            font_id.clone(),
            label_color,
        );
        
        painter.text(
            center_screen_pos + Vec2::new(label_offset, label_offset),
            egui::Align2::LEFT_TOP,
            "Paste",
            font_id,
            label_color,
        );
    }
}