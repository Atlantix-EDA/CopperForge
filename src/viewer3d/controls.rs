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
            if ui.button("Front View").clicked() {
                response.front_view = true;
            }
            if ui.button("Right View").clicked() {
                response.right_view = true;
            }
            if ui.button("Iso View").clicked() {
                response.isometric_view = true;
            }
        });

        ui.horizontal(|ui| {
            if ui.button("Frame All").clicked() {
                response.frame_all = true;
            }
            if ui.button("Fit View").clicked() {
                response.fit_view = true;
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

    /// Show camera information with enhanced orbit details
    pub fn show_camera_info(&self, ui: &mut egui::Ui, camera: &crate::viewer3d::Camera3D) {
        ui.group(|ui| {
            ui.label("Camera Info:");
            ui.horizontal(|ui| {
                ui.label(format!("Eye: ({:.1}, {:.1}, {:.1})", camera.eye.x, camera.eye.y, camera.eye.z));
                ui.label(format!("Target: ({:.1}, {:.1}, {:.1})", camera.target.x, camera.target.y, camera.target.z));
            });
            ui.horizontal(|ui| {
                ui.label(format!("Distance: {:.1}", camera.distance));
                ui.label(format!("Azimuth: {:.1}°", camera.azimuth.to_degrees()));
                ui.label(format!("Elevation: {:.1}°", camera.elevation.to_degrees()));
            });
        });
    }

    /// Show help/instructions
    pub fn show_help(&self, ui: &mut egui::Ui) {
        ui.group(|ui| {
            ui.label("3D Camera Controls:");
            ui.label("• Drag: Orbit camera around target");
            ui.label("• Shift+Drag / Middle+Drag: Pan view");
            ui.label("• Scroll: Zoom in/out");
            ui.label("• View buttons: Quick camera presets");
            ui.label("• Frame All: Fit all geometry in view");
        });
    }
}

/// Response from the controls UI indicating what actions were requested
#[derive(Default)]
pub struct ViewerControlsResponse {
    pub reset_view: bool,
    pub top_view: bool,
    pub front_view: bool,
    pub right_view: bool,
    pub isometric_view: bool,
    pub frame_all: bool,
    pub fit_view: bool,
}

impl ViewerControlsResponse {
    /// Check if any view change was requested
    pub fn has_view_change(&self) -> bool {
        self.reset_view || self.top_view || self.front_view || self.right_view || self.isometric_view || self.frame_all || self.fit_view
    }
}