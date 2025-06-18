use crate::ecs::Mesh3D;

/// 3D PCB viewer that integrates with egui (simplified version)
pub struct PcbViewer {
    meshes: Vec<Mesh3D>,
    camera_rotation: f32,
}

impl PcbViewer {
    pub fn new() -> Self {
        Self {
            meshes: Vec::new(),
            camera_rotation: 0.0,
        }
    }

    /// Set the PCB meshes to render
    pub fn set_meshes(&mut self, meshes: Vec<Mesh3D>) {
        self.meshes = meshes;
    }

    /// Show the 3D PCB viewer in egui (simplified placeholder)
    pub fn show(&mut self, ui: &mut egui::Ui) {
        // Allocate space for the 3D viewer
        let available_size = ui.available_size();
        let size = egui::Vec2::new(
            available_size.x.max(200.0),
            available_size.y.max(200.0),
        );

        let (rect, response) = ui.allocate_exact_size(size, egui::Sense::click_and_drag());

        // For now, just draw a placeholder rectangle
        ui.painter().rect(
            rect,
            5.0,
            egui::Color32::DARK_GRAY,
            egui::Stroke::new(2.0, egui::Color32::WHITE),
            egui::StrokeKind::Outside,
        );

        // Add some UI controls
        ui.horizontal(|ui| {
            ui.label("3D PCB Viewer");
            if ui.button("Reset View").clicked() {
                self.camera_rotation = 0.0;
            }
        });

        // Show some stats
        ui.label(format!("Meshes: {}", self.meshes.len()));
        ui.label(format!("Camera rotation: {:.1}°", self.camera_rotation));

        // Handle mouse input for camera controls
        if response.dragged() {
            let delta = response.drag_delta();
            self.camera_rotation += delta.x * 0.5;
        }

        // Add a simple wireframe visualization
        if !self.meshes.is_empty() {
            let painter = ui.painter();
            let rect_center = rect.center();
            
            // Draw a simple 3D cube representation
            let size = 50.0;
            let rotation = self.camera_rotation.to_radians();
            let cos_r = rotation.cos();
            let sin_r = rotation.sin();
            
            // Define cube vertices (simplified 2D projection)
            let vertices = [
                [-size, -size], [size, -size], [size, size], [-size, size],
                [-size * cos_r, -size * sin_r], [size * cos_r, -size * sin_r], 
                [size * cos_r, size * sin_r], [-size * cos_r, size * sin_r],
            ];
            
            // Draw wireframe edges
            let color = egui::Color32::LIGHT_GREEN;
            for i in 0..4 {
                let next = (i + 1) % 4;
                painter.line_segment([
                    rect_center + egui::Vec2::new(vertices[i][0], vertices[i][1]),
                    rect_center + egui::Vec2::new(vertices[next][0], vertices[next][1]),
                ], egui::Stroke::new(2.0, color));
                
                painter.line_segment([
                    rect_center + egui::Vec2::new(vertices[i + 4][0], vertices[i + 4][1]),
                    rect_center + egui::Vec2::new(vertices[(next) + 4][0], vertices[(next) + 4][1]),
                ], egui::Stroke::new(2.0, color));
                
                painter.line_segment([
                    rect_center + egui::Vec2::new(vertices[i][0], vertices[i][1]),
                    rect_center + egui::Vec2::new(vertices[i + 4][0], vertices[i + 4][1]),
                ], egui::Stroke::new(2.0, color));
            }
        }
        
        // Display mesh information
        if !self.meshes.is_empty() {
            ui.separator();
            ui.label("Mesh Details:");
            for (i, mesh) in self.meshes.iter().enumerate() {
                ui.label(format!(
                    "Mesh {}: {} vertices, {} triangles, material: {:?}",
                    i,
                    mesh.vertices.len(),
                    mesh.indices.len() / 3,
                    mesh.material_id
                ));
            }
        }
    }
}