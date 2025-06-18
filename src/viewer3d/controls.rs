//! UI Controls for the 3D viewer
//! 
//! Provides the user interface elements for controlling the 3D view,
//! including camera presets, rendering options, and view settings.

/// View control buttons and settings
pub struct ViewerControls {
    pub show_wireframe: bool,
    pub show_materials: bool,
    pub auto_rotate: bool,
    pub background_color: [f32; 3],
}

impl ViewerControls {
    pub fn new() -> Self {
        Self {
            show_wireframe: false,
            show_materials: true,
            auto_rotate: false,
            background_color: [0.1, 0.1, 0.15],
        }
    }

    /// Render the control panel UI
    pub fn show_ui(&mut self, ui: &mut egui::Ui) -> ViewerControlsResponse {
        let mut response = ViewerControlsResponse::default();

        ui.horizontal(|ui| {
            if ui.button("Reset View").clicked() {
                response.reset_view = true;
            }
            if ui.button("Top View").clicked() {
                response.top_view = true;
            }
            if ui.button("Side View").clicked() {
                response.side_view = true;
            }
            if ui.button("Iso View").clicked() {
                response.isometric_view = true;
            }
        });

        ui.separator();

        ui.horizontal(|ui| {
            ui.checkbox(&mut self.show_wireframe, "Wireframe");
            ui.checkbox(&mut self.show_materials, "Materials");
            ui.checkbox(&mut self.auto_rotate, "Auto Rotate");
        });

        ui.horizontal(|ui| {
            ui.label("Background:");
            ui.color_edit_button_rgb(&mut self.background_color);
        });

        response
    }

    /// Show statistics panel
    pub fn show_stats(&self, ui: &mut egui::Ui, mesh_count: usize, vertex_count: usize, triangle_count: usize) {
        ui.group(|ui| {
            ui.label("Statistics:");
            ui.horizontal(|ui| {
                ui.label(format!("Meshes: {}", mesh_count));
                ui.label(format!("Vertices: {}", vertex_count));
                ui.label(format!("Triangles: {}", triangle_count));
            });
        });
    }

    /// Show camera information
    pub fn show_camera_info(&self, ui: &mut egui::Ui, eye: &nalgebra::Point3<f32>, target: &nalgebra::Point3<f32>) {
        ui.group(|ui| {
            ui.label("Camera:");
            ui.label(format!("Eye: ({:.1}, {:.1}, {:.1})", eye.x, eye.y, eye.z));
            ui.label(format!("Target: ({:.1}, {:.1}, {:.1})", target.x, target.y, target.z));
        });
    }

    /// Show help/instructions
    pub fn show_help(&self, ui: &mut egui::Ui) {
        ui.group(|ui| {
            ui.label("Controls:");
            ui.label("• Drag: Rotate view");
            ui.label("• Shift+Drag: Pan view");
            ui.label("• Scroll: Zoom in/out");
            ui.label("• Right-click: Context menu");
        });
    }
}

/// Response from the controls UI indicating what actions were requested
#[derive(Default)]
pub struct ViewerControlsResponse {
    pub reset_view: bool,
    pub top_view: bool,
    pub side_view: bool,
    pub isometric_view: bool,
}

impl ViewerControlsResponse {
    /// Check if any view change was requested
    pub fn has_view_change(&self) -> bool {
        self.reset_view || self.top_view || self.side_view || self.isometric_view
    }
}