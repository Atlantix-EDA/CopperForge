use crate::{kicad::*, DemoLensApp};
use egui::{Color32, Painter, Pos2, Rect, Stroke, Vec2};

pub struct PcbRenderer;

impl PcbRenderer {
    pub fn render_pcb(painter: &Painter, app: &DemoLensApp, viewport: &Rect) {
        if let Some(pcb) = &app.pcb_data {
            // For now, let's draw a simple representation of each layer
            // We'll draw rectangles to represent the PCB layers
            
            // Calculate a simple bounding box for the PCB (placeholder values)
            let pcb_width = 100.0; // mm
            let pcb_height = 80.0; // mm
            
            // Convert to screen coordinates
            let scale = app.view_state.scale;
            let center = viewport.center();
            
            // Draw PCB outline
            let pcb_rect = Rect::from_center_size(
                center,
                Vec2::new(pcb_width * scale, pcb_height * scale)
            );
            
            // Draw edge cuts layer (board outline)
            if app.active_pcb_layers.contains(&"Edge.Cuts".to_string()) {
                // Draw rectangle outline using lines
                let tl = pcb_rect.min;
                let tr = Pos2::new(pcb_rect.max.x, pcb_rect.min.y);
                let br = pcb_rect.max;
                let bl = Pos2::new(pcb_rect.min.x, pcb_rect.max.y);
                
                let stroke = Stroke::new(2.0, Color32::from_rgb(255, 255, 0));
                painter.line_segment([tl, tr], stroke);
                painter.line_segment([tr, br], stroke);
                painter.line_segment([br, bl], stroke);
                painter.line_segment([bl, tl], stroke);
            }
            
            // Draw copper layers as filled rectangles with transparency
            for layer_name in &app.active_pcb_layers {
                if let Some(layer) = pcb.layers.values().find(|l| l.name == *layer_name) {
                    let color = super::pcb_layer_panel::get_layer_color(&layer.name);
                    let color_with_alpha = Color32::from_rgba_unmultiplied(
                        color.r(),
                        color.g(),
                        color.b(),
                        100 // Semi-transparent
                    );
                    
                    // Offset each layer slightly for visibility
                    let offset = layer.id as f32 * 2.0;
                    let layer_rect = Rect::from_center_size(
                        center + Vec2::new(offset, offset),
                        Vec2::new((pcb_width - 10.0) * scale, (pcb_height - 10.0) * scale)
                    );
                    
                    if layer.name.contains("Cu") {
                        // Copper layers - draw as filled rectangles
                        painter.rect_filled(
                            layer_rect,
                            5.0,
                            color_with_alpha
                        );
                    } else if layer.name.contains("SilkS") {
                        // Silkscreen layers - draw as outlines
                        let tl = layer_rect.min;
                        let tr = Pos2::new(layer_rect.max.x, layer_rect.min.y);
                        let br = layer_rect.max;
                        let bl = Pos2::new(layer_rect.min.x, layer_rect.max.y);
                        
                        let stroke = Stroke::new(1.5, color);
                        painter.line_segment([tl, tr], stroke);
                        painter.line_segment([tr, br], stroke);
                        painter.line_segment([br, bl], stroke);
                        painter.line_segment([bl, tl], stroke);
                        
                        // Add some text to represent silkscreen
                        painter.text(
                            layer_rect.center(),
                            egui::Align2::CENTER_CENTER,
                            &layer.name,
                            egui::FontId::default(),
                            color
                        );
                    }
                }
            }
            
            // Draw layer info in corner
            let mut y_offset = 20.0;
            for layer_name in &app.active_pcb_layers {
                if let Some(layer) = pcb.layers.values().find(|l| l.name == *layer_name) {
                    let color = super::pcb_layer_panel::get_layer_color(&layer.name);
                    painter.text(
                        Pos2::new(viewport.min.x + 10.0, viewport.min.y + y_offset),
                        egui::Align2::LEFT_TOP,
                        format!("[{}] {}", layer.id, layer.name),
                        egui::FontId::default(),
                        color
                    );
                    y_offset += 18.0;
                }
            }
        }
    }
    
    pub fn calculate_pcb_bounds(_pcb: &PcbFile) -> Rect {
        // For now, return a default size
        // In a real implementation, you'd calculate from footprints, tracks, etc.
        Rect::from_min_size(
            Pos2::new(-50.0, -40.0),
            Vec2::new(100.0, 80.0)
        )
    }
}