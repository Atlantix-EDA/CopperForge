use std::path::PathBuf;
use image::{ImageBuffer, Rgba, RgbaImage};
use eframe::emath::{Rect, Vec2};
use egui::Pos2;
use gerber_viewer::{ViewState, BoundingBox, GerberTransform};
use crate::{DemoLensApp, ecs::LayerType};
use crate::display::VectorOffset;
use nalgebra::{Vector2, Point2};

#[allow(dead_code)]
pub struct PngExporter;

#[allow(dead_code)]
impl PngExporter {
    /// Export each layer in quadrant view as a separate PNG file
    pub fn export_quadrant_layers(app: &mut DemoLensApp, output_dir: &PathBuf, width: u32, height: u32) -> Result<Vec<PathBuf>, String> {
        if !app.display_manager.quadrant_view_enabled {
            return Err("Quadrant view must be enabled for layer export".to_string());
        }

        std::fs::create_dir_all(output_dir).map_err(|e| format!("Failed to create output directory: {}", e))?;
        
        // Get mechanical outline layer using ECS - this defines the consistent bounding box for all exports
        let (mechanical_outline_gerber, master_bbox) = {
            let mechanical_outline_data = crate::ecs::get_layer_data(&mut app.ecs_world, LayerType::MechanicalOutline)
                .ok_or("Mechanical outline layer is required for consistent PNG export boundaries")?;
            let gerber_layer = mechanical_outline_data.2.0.clone();
            let bbox = Self::calculate_master_bounding_box(app, &gerber_layer)?;
            (gerber_layer, bbox)
        };
        
        let mut exported_files = Vec::new();
        
        // Collect visible layers data first to avoid borrowing conflicts
        let mut layers_to_export = Vec::new();
        for layer_type in LayerType::all() {
            if let Some((_entity, _layer_info, gerber_data, visibility)) = crate::ecs::get_layer_data(&mut app.ecs_world, layer_type) {
                if visibility.visible && layer_type != LayerType::MechanicalOutline {
                    // Skip if layer shouldn't render for current view
                    if layer_type.should_render(app.display_manager.showing_top) {
                        layers_to_export.push((layer_type, gerber_data.0.clone()));
                    }
                }
            }
        }
        
        // Now export each layer without borrowing conflicts
        for (layer_type, gerber_layer) in layers_to_export {
            let filename = format!("{}.png", layer_type.display_name().replace(" ", "_").to_lowercase());
            let output_path = output_dir.join(&filename);
            
            Self::export_single_layer_with_bbox(
                app,
                &gerber_layer,
                &layer_type,
                Some(&mechanical_outline_gerber),
                &master_bbox,
                &output_path,
                width,
                height,
            )?;
            
            exported_files.push(output_path);
        }
        
        Ok(exported_files)
    }
    
    /// Calculate the master bounding box from mechanical outline layer (defines size for all exports)
    fn calculate_master_bounding_box(
        app: &DemoLensApp,
        mechanical_outline: &gerber_viewer::GerberLayer,
    ) -> Result<BoundingBox, String> {
        // The mechanical outline defines the board boundary and should be used as the 
        // consistent bounding box for all layer exports
        let outline_bbox = Self::calculate_transformed_bounding_box(app, mechanical_outline, &LayerType::MechanicalOutline)?;
        
        // Add some padding around the mechanical outline (5% on each side)
        let padding_factor = 0.05;
        let bbox_width = outline_bbox.width();
        let bbox_height = outline_bbox.height();
        let padding_x = bbox_width * padding_factor;
        let padding_y = bbox_height * padding_factor;
        
        let padded_bbox = BoundingBox {
            min: Point2::new(
                outline_bbox.min.x - padding_x,
                outline_bbox.min.y - padding_y,
            ),
            max: Point2::new(
                outline_bbox.max.x + padding_x,
                outline_bbox.max.y + padding_y,
            ),
        };
        
        Ok(padded_bbox)
    }
    
    /// Export a single layer to PNG using the consistent master bounding box
    fn export_single_layer_with_bbox(
        app: &DemoLensApp,
        gerber_layer: &gerber_viewer::GerberLayer,
        layer_type: &LayerType,
        mechanical_outline: Option<&gerber_viewer::GerberLayer>,
        master_bbox: &BoundingBox,
        output_path: &PathBuf,
        width: u32,
        height: u32,
    ) -> Result<(), String> {
        // Create image buffer
        let mut img: RgbaImage = ImageBuffer::new(width, height);
        
        // Fill with black background (PCB standard)
        for pixel in img.pixels_mut() {
            *pixel = Rgba([0, 0, 0, 255]); // Black background
        }
        
        // Create viewport that matches our master bounding box
        let viewport = Rect::from_min_size(
            Pos2::ZERO,
            Vec2::new(width as f32, height as f32)
        );
        
        // Calculate view state to fit the master bounding box (same for all layers)
        let view_state = Self::calculate_bbox_view_state(master_bbox, &viewport);
        
        // Log the export operation
        println!("Exporting {} layer to {:?}", layer_type.display_name(), output_path);
        println!("  Using master bounding box: ({:.2}, {:.2}) to ({:.2}, {:.2}) mm", 
                 master_bbox.min.x, master_bbox.min.y, 
                 master_bbox.max.x, master_bbox.max.y);
        println!("  Board dimensions: {:.2} x {:.2} mm", 
                 master_bbox.width(), master_bbox.height());
        println!("  Image size: {}x{} pixels", width, height);
        println!("  Scale: {:.2} (consistent for all layers)", view_state.scale);
        
        // Render the gerber layer to the image buffer
        Self::render_gerber_to_image(
            app,
            gerber_layer,
            layer_type,
            mechanical_outline,
            &view_state,
            &mut img,
            width,
            height,
        )?;
        
        // Save the image
        img.save(output_path).map_err(|e| format!("Failed to save PNG: {}", e))?;
        
        Ok(())
    }
    
    /// Render gerber layer to image buffer using a simplified approach
    fn render_gerber_to_image(
        app: &DemoLensApp,
        _gerber_layer: &gerber_viewer::GerberLayer,
        layer_type: &LayerType,
        mechanical_outline: Option<&gerber_viewer::GerberLayer>,
        _view_state: &ViewState,
        img: &mut RgbaImage,
        width: u32,
        height: u32,
    ) -> Result<(), String> {
        // Note: Layer positions are managed by the DisplayManager
        
        // Get quadrant offset for this layer type - this is the key positioning info
        let quadrant_offset = app.display_manager.get_quadrant_offset(layer_type);
        
        println!("🎯 Exporting {} with quadrant offset: ({:.1}, {:.1})", 
                layer_type.display_name(), quadrant_offset.x, quadrant_offset.y);
        
        let layer_color = layer_type.color();
        let rgba = [layer_color.r(), layer_color.g(), layer_color.b(), layer_color.a()];
        
        // Draw the layer representation using the quadrant offset directly
        Self::draw_layer_in_quadrant(img, layer_type, &quadrant_offset, &rgba, width, height);
        
        // Draw mechanical outline border at image edge for reference  
        if let Some(_outline_layer) = mechanical_outline {
            let outline_color = LayerType::MechanicalOutline.color();
            let outline_rgba = [outline_color.r(), outline_color.g(), outline_color.b(), outline_color.a()];
            
            // Draw simple border around entire image
            let margin = 20;
            Self::draw_rectangle_border(img, margin, margin, width - margin, height - margin, &outline_rgba, 2);
        }
        
        Ok(())
    }
    
    /// Draw layer in its proper quadrant using simple, direct positioning
    fn draw_layer_in_quadrant(
        img: &mut RgbaImage,
        layer_type: &LayerType,
        quadrant_offset: &VectorOffset,
        color: &[u8; 4],
        width: u32,
        height: u32,
    ) {
        println!("  🎨 Drawing {} in quadrant at offset ({:.1}, {:.1})", 
                layer_type.display_name(), quadrant_offset.x, quadrant_offset.y);
        
        // Simple quadrant mapping - divide image into 4 sections
        let half_width = width / 2;
        let half_height = height / 2;
        let quarter_width = width / 4;
        let quarter_height = height / 4;
        
        // Determine quadrant based on offset signs (from DisplayManager::get_quadrant_offset)
        let (quadrant_x, quadrant_y) = if quadrant_offset.x > 0.0 && quadrant_offset.y > 0.0 {
            // Quadrant 1 (top-right): Copper
            (half_width + quarter_width / 2, quarter_height / 2)
        } else if quadrant_offset.x < 0.0 && quadrant_offset.y > 0.0 {
            // Quadrant 2 (top-left): Silkscreen  
            (quarter_width / 2, quarter_height / 2)
        } else if quadrant_offset.x < 0.0 && quadrant_offset.y < 0.0 {
            // Quadrant 3 (bottom-left): Soldermask
            (quarter_width / 2, half_height + quarter_height / 2)
        } else if quadrant_offset.x > 0.0 && quadrant_offset.y < 0.0 {
            // Quadrant 4 (bottom-right): Paste
            (half_width + quarter_width / 2, half_height + quarter_height / 2)
        } else {
            // Center (mechanical outline)
            (half_width, half_height)
        };
        
        // Draw layer representation in the calculated quadrant
        let size = quarter_width.min(quarter_height) as u32;
        let x1 = quadrant_x.saturating_sub(size / 2);
        let y1 = quadrant_y.saturating_sub(size / 2);
        let x2 = (quadrant_x + size / 2).min(width);
        let y2 = (quadrant_y + size / 2).min(height);
        
        println!("  📍 Quadrant position: ({}, {}) -> Rectangle ({}, {}) to ({}, {})", 
                quadrant_x, quadrant_y, x1, y1, x2, y2);
        
        // Draw based on layer type
        match layer_type {
            LayerType::Copper(_) => {
                Self::fill_rectangle(img, x1, y1, x2, y2, color);
            },
            LayerType::Silkscreen(_) => {
                // Draw silkscreen as text-like patterns
                Self::draw_silkscreen_pattern(img, (x1, y1), (x2, y2), color);
            },
            LayerType::Soldermask(_) => {
                // Draw soldermask as mostly filled with some openings
                Self::fill_rectangle_with_openings(img, x1, y1, x2, y2, color);
            },
            LayerType::Paste(_) => {
                // Draw paste as small squares/dots
                Self::draw_paste_pattern(img, (x1, y1), (x2, y2), color);
            },
            LayerType::MechanicalOutline => {
                // This is handled separately
            }
        }
    }
    
    /// Draw mechanical outline as border
    fn draw_outline_representation(
        img: &mut RgbaImage,
        _bbox: &BoundingBox,
        _offset: &VectorOffset,
        _view_state: &ViewState,
        color: &[u8; 4],
        width: u32,
        height: u32,
    ) {
        println!("  Drawing mechanical outline");
        
        // Draw outline as a border around the same area as the layers
        let margin = 200;
        let x1 = margin;
        let y1 = margin;
        let x2 = width - margin;
        let y2 = height - margin;
        
        Self::draw_rectangle_border(img, x1, y1, x2, y2, color, 5);
    }
    
    /// Convert gerber coordinates to screen coordinates
    fn gerber_to_screen_coords(
        gerber_x: f64,
        gerber_y: f64,
        view_state: &ViewState,
        img_width: u32,
        img_height: u32,
    ) -> (u32, u32) {
        let screen_x = view_state.translation.x + (gerber_x as f32 * view_state.scale);
        let screen_y = view_state.translation.y - (gerber_y as f32 * view_state.scale); // Y is flipped
        
        let x = screen_x.clamp(0.0, img_width as f32 - 1.0) as u32;
        let y = screen_y.clamp(0.0, img_height as f32 - 1.0) as u32;
        
        // Debug output to see what coordinates we're getting
        println!("  Gerber ({:.2}, {:.2}) -> Screen ({}, {})", gerber_x, gerber_y, x, y);
        
        (x, y)
    }
    
    /// Calculate the transformed bounding box for a layer including all transformations
    fn calculate_transformed_bounding_box(
        app: &DemoLensApp,
        gerber_layer: &gerber_viewer::GerberLayer,
        layer_type: &LayerType,
    ) -> Result<BoundingBox, String> {
        // Get the original bounding box
        let original_bbox = gerber_layer.bounding_box().clone();
        
        // Get quadrant offset for this layer type
        let quadrant_offset = app.display_manager.get_quadrant_offset(layer_type);
        
        // Calculate combined offset
        let combined_offset = VectorOffset {
            x: app.display_manager.center_offset.x + quadrant_offset.x,
            y: app.display_manager.center_offset.y + quadrant_offset.y,
        };
        
        // Create transform similar to what's used in rendering
        let origin: Vector2<f64> = app.display_manager.center_offset.clone().into();
        let offset: Vector2<f64> = combined_offset.into();
        
        let transform = GerberTransform {
            rotation: app.rotation_degrees.to_radians(),
            mirroring: app.display_manager.mirroring.clone().into(),
            origin: origin - offset,
            offset,
            scale: 1.0,
        };
        
        // Transform all corners of the bounding box
        let corners = original_bbox.vertices();
        let transformed_corners: Vec<Point2<f64>> = corners
            .into_iter()
            .map(|corner| transform.apply_to_position(corner))
            .collect();
        
        // Create new bounding box from transformed corners
        Ok(BoundingBox::from_points(&transformed_corners))
    }
    
    /// Combine two bounding boxes into one that contains both
    fn combine_bounding_boxes(bbox1: &BoundingBox, bbox2: &BoundingBox) -> BoundingBox {
        BoundingBox {
            min: Point2::new(
                bbox1.min.x.min(bbox2.min.x),
                bbox1.min.y.min(bbox2.min.y),
            ),
            max: Point2::new(
                bbox1.max.x.max(bbox2.max.x),
                bbox1.max.y.max(bbox2.max.y),
            ),
        }
    }
    
    /// Calculate view state to fit a specific bounding box in the viewport
    fn calculate_bbox_view_state(bbox: &BoundingBox, viewport: &Rect) -> ViewState {
        let bbox_width = bbox.width() as f32;
        let bbox_height = bbox.height() as f32;
        
        // Calculate scale to fit the bounding box with some padding
        let scale = f32::min(
            viewport.width() / bbox_width,
            viewport.height() / bbox_height,
        ) * 0.9; // 90% to add some padding
        
        // Calculate translation to center the bounding box in the viewport
        let bbox_center = bbox.center();
        let viewport_center = viewport.center();
        
        println!("  Bbox center: ({:.2}, {:.2})", bbox_center.x, bbox_center.y);
        println!("  Viewport center: ({:.2}, {:.2})", viewport_center.x, viewport_center.y);
        println!("  Scale: {:.2}", scale);
        
        let translation_x = viewport_center.x - (bbox_center.x as f32 * scale);
        let translation_y = viewport_center.y + (bbox_center.y as f32 * scale); // Y is flipped in screen coords
        
        println!("  Translation: ({:.2}, {:.2})", translation_x, translation_y);
        
        ViewState {
            scale,
            base_scale: scale,
            translation: Vec2::new(translation_x, translation_y),
        }
    }
    
    /// Fill a rectangle in the image
    fn fill_rectangle(img: &mut RgbaImage, x1: u32, y1: u32, x2: u32, y2: u32, color: &[u8; 4]) {
        let min_x = x1.min(x2);
        let max_x = x1.max(x2);
        let min_y = y1.min(y2);
        let max_y = y1.max(y2);
        
        for y in min_y..=max_y {
            for x in min_x..=max_x {
                if let Some(pixel) = img.get_pixel_mut_checked(x, y) {
                    *pixel = Rgba(*color);
                }
            }
        }
    }
    
    /// Draw rectangle border
    fn draw_rectangle_border(img: &mut RgbaImage, x1: u32, y1: u32, x2: u32, y2: u32, color: &[u8; 4], thickness: u32) {
        let min_x = x1.min(x2);
        let max_x = x1.max(x2);
        let min_y = y1.min(y2);
        let max_y = y1.max(y2);
        
        // Draw border lines
        for t in 0..thickness {
            // Top and bottom borders
            for x in min_x..=max_x {
                if let Some(pixel) = img.get_pixel_mut_checked(x, min_y + t) {
                    *pixel = Rgba(*color);
                }
                if max_y >= t {
                    if let Some(pixel) = img.get_pixel_mut_checked(x, max_y - t) {
                        *pixel = Rgba(*color);
                    }
                }
            }
            
            // Left and right borders
            for y in min_y..=max_y {
                if let Some(pixel) = img.get_pixel_mut_checked(min_x + t, y) {
                    *pixel = Rgba(*color);
                }
                if max_x >= t {
                    if let Some(pixel) = img.get_pixel_mut_checked(max_x - t, y) {
                        *pixel = Rgba(*color);
                    }
                }
            }
        }
    }
    
    /// Draw trace pattern for copper layers
    fn draw_trace_pattern(img: &mut RgbaImage, min_screen: (u32, u32), max_screen: (u32, u32), color: &[u8; 4]) {
        let (min_x, min_y) = min_screen;
        let (max_x, max_y) = max_screen;
        
        // Draw some horizontal and vertical lines to simulate traces
        let step = 20;
        
        for i in (min_y..max_y).step_by(step) {
            for x in min_x..max_x {
                if let Some(pixel) = img.get_pixel_mut_checked(x, i) {
                    *pixel = Rgba(*color);
                }
            }
        }
        
        for i in (min_x..max_x).step_by(step) {
            for y in min_y..max_y {
                if let Some(pixel) = img.get_pixel_mut_checked(i, y) {
                    *pixel = Rgba(*color);
                }
            }
        }
    }
    
    /// Draw silkscreen pattern
    fn draw_silkscreen_pattern(img: &mut RgbaImage, min_screen: (u32, u32), max_screen: (u32, u32), color: &[u8; 4]) {
        let (min_x, min_y) = min_screen;
        let (max_x, max_y) = max_screen;
        
        // Draw some text-like patterns
        for y in (min_y..max_y).step_by(10) {
            for x in (min_x..max_x).step_by(15) {
                // Draw small rectangles to simulate text
                for dy in 0..3 {
                    for dx in 0..8 {
                        if let Some(pixel) = img.get_pixel_mut_checked(x + dx, y + dy) {
                            *pixel = Rgba(*color);
                        }
                    }
                }
            }
        }
    }
    
    /// Draw soldermask with openings
    fn fill_rectangle_with_openings(img: &mut RgbaImage, x1: u32, y1: u32, x2: u32, y2: u32, color: &[u8; 4]) {
        let min_x = x1.min(x2);
        let max_x = x1.max(x2);
        let min_y = y1.min(y2);
        let max_y = y1.max(y2);
        
        for y in min_y..=max_y {
            for x in min_x..=max_x {
                // Create some openings (skip every 25th pixel in a pattern)
                if (x + y) % 25 != 0 {
                    if let Some(pixel) = img.get_pixel_mut_checked(x, y) {
                        *pixel = Rgba(*color);
                    }
                }
            }
        }
    }
    
    /// Draw paste pattern as dots
    fn draw_paste_pattern(img: &mut RgbaImage, min_screen: (u32, u32), max_screen: (u32, u32), color: &[u8; 4]) {
        let (min_x, min_y) = min_screen;
        let (max_x, max_y) = max_screen;
        
        // Draw dots in a grid pattern
        for y in (min_y..max_y).step_by(8) {
            for x in (min_x..max_x).step_by(8) {
                // Draw 3x3 squares
                for dy in 0..3 {
                    for dx in 0..3 {
                        if let Some(pixel) = img.get_pixel_mut_checked(x + dx, y + dy) {
                            *pixel = Rgba(*color);
                        }
                    }
                }
            }
        }
    }
    
    /// Calculate the appropriate view state for a single layer
    fn calculate_layer_view_state(
        app: &DemoLensApp,
        gerber_layer: &gerber_viewer::GerberLayer,
        viewport: &Rect,
        layer_type: &LayerType,
    ) -> ViewState {
        let bbox = gerber_layer.bounding_box();
        let content_width = bbox.width();
        let content_height = bbox.height();

        // Calculate scale to fit the content
        let scale = f32::min(
            viewport.width() / (content_width as f32),
            viewport.height() / (content_height as f32),
        ) * 0.95; // Add margin

        // Get quadrant offset for centering
        let quadrant_offset = app.display_manager.get_quadrant_offset(layer_type);
        let center_x = bbox.center().x + quadrant_offset.x;
        let center_y = bbox.center().y + quadrant_offset.y;

        ViewState {
            scale,
            base_scale: scale,
            translation: Vec2::new(
                viewport.center().x - (center_x as f32 * scale),
                viewport.center().y + (center_y as f32 * scale), // Y is flipped
            ),
        }
    }
    
    /// Alternative approach: Export visible viewport area as PNG
    pub fn export_current_view(_app: &DemoLensApp, _output_path: &PathBuf, _viewport: &Rect) -> Result<(), String> {
        // This would require integration with egui's rendering system
        // For now, we'll suggest using the built-in screenshot functionality
        Err("Use your OS screenshot tool to capture the current view. Full PNG export will be implemented in a future version.".to_string())
    }
}