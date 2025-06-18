//! Main 3D PCB Viewer component
//! 
//! Combines camera, renderer, and controls into a complete 3D visualization system for PCBs.

use crate::ecs::Mesh3D;
use crate::viewer3d::{Camera3D, CameraController, Renderer3D, ViewerControls};
use crate::viewer3d::controls::ViewerControlsResponse;
use crate::viewer3d::renderer::WgpuPaintCallback;
use nalgebra::Point3;

/// Main 3D PCB Viewer widget for egui
pub struct PcbViewer {
    camera: Camera3D,
    camera_controller: CameraController,
    renderer: Renderer3D,
    controls: ViewerControls,
    meshes: Vec<Mesh3D>,
    last_mouse_pos: Option<egui::Pos2>,
}

impl PcbViewer {
    /// Create a new 3D PCB viewer
    pub fn new() -> Self {
        Self {
            camera: Camera3D::new(),
            camera_controller: CameraController::new(),
            renderer: Renderer3D::new(),
            controls: ViewerControls::new(),
            meshes: Vec::new(),
            last_mouse_pos: None,
        }
    }

    /// Set the PCB meshes to render
    pub fn set_meshes(&mut self, meshes: Vec<Mesh3D>) {
        self.meshes = meshes;
    }

    /// Show the 3D PCB viewer UI
    pub fn show(&mut self, ui: &mut egui::Ui) {
        let available_size = ui.available_size();
        let viewer_size = egui::Vec2::new(
            available_size.x.max(400.0),
            available_size.y.max(300.0) - 150.0, // Leave space for controls
        );

        // Render the 3D viewport
        let (rect, response) = ui.allocate_exact_size(viewer_size, egui::Sense::click_and_drag());
        
        // Handle input
        self.handle_input(&response);

        // Draw viewport background
        ui.painter().rect(
            rect,
            5.0,
            egui::Color32::from_rgb(
                (self.controls.background_color[0] * 255.0) as u8,
                (self.controls.background_color[1] * 255.0) as u8,
                (self.controls.background_color[2] * 255.0) as u8,
            ),
            egui::Stroke::new(1.0, egui::Color32::from_rgb(60, 60, 80)),
            egui::StrokeKind::Outside,
        );

        // Try to get wgpu render state
        let render_state = ui.ctx().data(|d| d.get_temp::<std::sync::Arc<egui_wgpu::RenderState>>(egui::Id::NULL));
        
        if let Some(render_state) = render_state {
            // Initialize renderer if needed
            if !self.renderer.is_initialized() {
                let device = &render_state.device;
                let format = render_state.target_format;
                self.renderer.initialize(device, format);
            }

            // Update camera aspect ratio
            self.camera.update_aspect(rect.width(), rect.height());

            // Create wgpu paint callback
            let callback = egui_wgpu::Callback::new_paint_callback(
                rect,
                WgpuPaintCallback {
                    renderer: self.renderer.get_wgpu_renderer(),
                    meshes: self.meshes.clone(),
                    camera: self.camera.clone(),
                    size: rect.size(),
                },
            );
            
            ui.painter().add(callback);

            // Upload meshes to renderer if we have any
            if !self.meshes.is_empty() {
                self.renderer.upload_meshes(&render_state.device, &self.meshes);
            }

            // Update camera in renderer
            self.renderer.update_camera(&render_state.queue, &self.camera, rect.width(), rect.height());
        } else {
            // Fallback: wireframe rendering
            self.draw_wireframe_fallback(ui, rect);
        }

        // Show controls below the viewport
        ui.separator();
        let controls_response = self.controls.show_ui(ui);
        self.handle_controls_response(controls_response);

        // Show statistics
        let total_vertices: usize = self.meshes.iter().map(|m| m.vertices.len()).sum();
        let total_triangles: usize = self.meshes.iter().map(|m| m.indices.len() / 3).sum();
        self.controls.show_stats(ui, self.meshes.len(), total_vertices, total_triangles);

        // Show camera info
        self.controls.show_camera_info(ui, &self.camera.eye, &self.camera.target);

        // Show help
        self.controls.show_help(ui);
    }

    /// Handle user input for camera control
    fn handle_input(&mut self, response: &egui::Response) {
        let mut delta_x = 0.0;
        let mut delta_y = 0.0;
        let mut zoom = 0.0;

        // Handle mouse dragging
        if response.dragged() {
            let current_pos = response.hover_pos();
            if let (Some(current), Some(last)) = (current_pos, self.last_mouse_pos) {
                delta_x = current.x - last.x;
                delta_y = current.y - last.y;
            }
        }

        // Handle scroll wheel for zoom
        if response.hovered() {
            let scroll_delta = response.ctx.input(|i| i.smooth_scroll_delta.y);
            zoom = scroll_delta * 0.01;
        }

        // Update camera with input
        if delta_x != 0.0 || delta_y != 0.0 || zoom != 0.0 {
            let is_panning = response.ctx.input(|i| i.modifiers.shift);
            self.camera_controller.handle_input(
                &mut self.camera,
                delta_x,
                delta_y,
                zoom,
                is_panning,
            );
        }

        // Update last mouse position
        self.last_mouse_pos = response.hover_pos();
    }

    /// Handle responses from the controls UI
    fn handle_controls_response(&mut self, response: ViewerControlsResponse) {
        if response.reset_view {
            self.camera = Camera3D::new();
        }
        if response.top_view {
            self.camera.set_top_view();
        }
        if response.side_view {
            self.camera.set_side_view();
        }
        if response.isometric_view {
            self.camera.set_isometric_view();
        }
    }

    /// Fallback wireframe rendering when wgpu is not available
    fn draw_wireframe_fallback(&self, ui: &mut egui::Ui, rect: egui::Rect) {
        let painter = ui.painter();
        let center = rect.center();
        
        // Get view-projection matrix
        let view_proj = self.camera.build_view_projection_matrix();
        
        // If we have actual meshes, try to render them as wireframes
        if !self.meshes.is_empty() {
            self.draw_mesh_wireframes(painter, center, rect, view_proj);
        } else {
            // Draw a placeholder cube
            self.draw_placeholder_cube(painter, center, rect, view_proj);
        }
        
        // Draw overlay text
        painter.text(
            rect.min + egui::Vec2::new(10.0, 10.0),
            egui::Align2::LEFT_TOP,
            if self.meshes.is_empty() {
                "No meshes loaded - showing placeholder".to_string()
            } else {
                format!("{} meshes (wireframe fallback)", self.meshes.len())
            },
            egui::FontId::default(),
            egui::Color32::WHITE,
        );
    }

    /// Draw actual mesh wireframes
    fn draw_mesh_wireframes(
        &self,
        painter: &egui::Painter,
        center: egui::Pos2,
        rect: egui::Rect,
        view_proj: nalgebra::Matrix4<f32>,
    ) {
        let scale = rect.width().min(rect.height()) * 0.1;
        
        for (mesh_idx, mesh) in self.meshes.iter().take(5).enumerate() {
            let color = match mesh_idx {
                0 => egui::Color32::from_rgb(255, 100, 100),
                1 => egui::Color32::from_rgb(100, 255, 100),
                2 => egui::Color32::from_rgb(100, 100, 255),
                3 => egui::Color32::from_rgb(255, 255, 100),
                _ => egui::Color32::from_rgb(255, 100, 255),
            };
            
            // Project vertices
            let mut projected_vertices = Vec::new();
            for vertex in &mesh.vertices {
                let world_pos = Point3::new(vertex.x, vertex.y, vertex.z);
                let homogeneous = view_proj * world_pos.to_homogeneous();
                
                if homogeneous.w > 0.0 {
                    let ndc = Point3::new(
                        homogeneous.x / homogeneous.w,
                        homogeneous.y / homogeneous.w,
                        homogeneous.z / homogeneous.w,
                    );
                    
                    // Only draw if in front of camera and within reasonable bounds
                    if ndc.z > -1.0 && ndc.z < 1.0 && ndc.x.abs() < 2.0 && ndc.y.abs() < 2.0 {
                        let screen_x = center.x + ndc.x * scale;
                        let screen_y = center.y - ndc.y * scale; // Flip Y
                        projected_vertices.push(Some(egui::Pos2::new(screen_x, screen_y)));
                    } else {
                        projected_vertices.push(None);
                    }
                } else {
                    projected_vertices.push(None);
                }
            }
            
            // Draw triangles as wireframes
            for triangle in mesh.indices.chunks(3) {
                if triangle.len() == 3 {
                    let idx0 = triangle[0] as usize;
                    let idx1 = triangle[1] as usize;
                    let idx2 = triangle[2] as usize;
                    
                    if idx0 < projected_vertices.len() 
                        && idx1 < projected_vertices.len() 
                        && idx2 < projected_vertices.len() {
                        
                        if let (Some(p0), Some(p1), Some(p2)) = (
                            projected_vertices[idx0],
                            projected_vertices[idx1],
                            projected_vertices[idx2],
                        ) {
                            painter.line_segment([p0, p1], egui::Stroke::new(1.0, color));
                            painter.line_segment([p1, p2], egui::Stroke::new(1.0, color));
                            painter.line_segment([p2, p0], egui::Stroke::new(1.0, color));
                        }
                    }
                }
            }
        }
    }

    /// Draw a placeholder cube
    fn draw_placeholder_cube(
        &self,
        painter: &egui::Painter,
        center: egui::Pos2,
        rect: egui::Rect,
        view_proj: nalgebra::Matrix4<f32>,
    ) {
        let cube_vertices = [
            Point3::new(-1.0, -1.0, -1.0),
            Point3::new( 1.0, -1.0, -1.0),
            Point3::new( 1.0,  1.0, -1.0),
            Point3::new(-1.0,  1.0, -1.0),
            Point3::new(-1.0, -1.0,  1.0),
            Point3::new( 1.0, -1.0,  1.0),
            Point3::new( 1.0,  1.0,  1.0),
            Point3::new(-1.0,  1.0,  1.0),
        ];
        
        let edges = [
            (0, 1), (1, 2), (2, 3), (3, 0), // Front face
            (4, 5), (5, 6), (6, 7), (7, 4), // Back face
            (0, 4), (1, 5), (2, 6), (3, 7), // Connecting edges
        ];
        
        // Project vertices
        let scale = rect.width().min(rect.height()) * 0.3;
        let mut projected_points = Vec::new();
        
        for vertex in &cube_vertices {
            let homogeneous = view_proj * vertex.to_homogeneous();
            if homogeneous.w != 0.0 {
                let ndc = Point3::new(
                    homogeneous.x / homogeneous.w,
                    homogeneous.y / homogeneous.w,
                    homogeneous.z / homogeneous.w,
                );
                
                let screen_x = center.x + ndc.x * scale;
                let screen_y = center.y - ndc.y * scale;
                projected_points.push(egui::Pos2::new(screen_x, screen_y));
            }
        }
        
        // Draw edges
        let color = egui::Color32::from_rgb(100, 200, 100);
        for (i, j) in &edges {
            if *i < projected_points.len() && *j < projected_points.len() {
                painter.line_segment(
                    [projected_points[*i], projected_points[*j]],
                    egui::Stroke::new(2.0, color),
                );
            }
        }
    }
}