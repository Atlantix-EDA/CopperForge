use egui::{Ui, Rect, Vec2, Pos2, Color32, Stroke};
use crate::layer_operations::LayerManager;
use super::pcb_3d::{GerberTo3D, LayerStackup, LayerMesh};
use transform_gizmo_egui::prelude::*;
use transform_gizmo_egui::math::{DVec3, DQuat, DMat4, Transform};
use nalgebra::{Matrix4, Vector3, Point3, Isometry3, Vector4};

/// 3D PCB viewer using transform-gizmo-egui - copying exact pattern from working example
pub struct Simple3DViewer {
    layers: Vec<LayerMesh>,
    substrate_mesh: Option<LayerMesh>,
    bounds_2d: (f32, f32, f32, f32), // min_x, min_y, max_x, max_y
    
    // Transform-gizmo fields - exactly like the example
    gizmo: Gizmo,
    gizmo_modes: EnumSet<GizmoMode>,
    gizmo_orientation: GizmoOrientation,
    
    // PCB transform - using same pattern as example
    scale: DVec3,
    rotation: DQuat,
    translation: DVec3,
    
    // Camera controls
    camera_distance: f32,
    camera_rotation: (f32, f32), // (phi, theta) in spherical coordinates
    
    // Last gizmo operation for display
    last_gizmo_operation: Option<String>,
}

impl Simple3DViewer {
    pub fn new() -> Self {
        Self {
            layers: Vec::new(),
            substrate_mesh: None,
            bounds_2d: (0.0, 0.0, 100.0, 100.0),
            
            // Initialize exactly like the example
            gizmo: Gizmo::default(),
            gizmo_modes: GizmoMode::all(),
            gizmo_orientation: GizmoOrientation::Local,
            scale: DVec3::ONE,
            rotation: DQuat::IDENTITY,
            translation: DVec3::ZERO,
            
            // Camera
            camera_distance: 150.0,
            camera_rotation: (0.5, 0.5),
            
            // Gizmo operation display
            last_gizmo_operation: None,
        }
    }
    
    pub fn build_from_layers(&mut self, layer_manager: &LayerManager) {
        self.layers.clear();
        
        let converter = GerberTo3D::new(LayerStackup::default());
        
        // Find overall bounds
        let mut overall_bounds: Option<gerber_viewer::BoundingBox> = None;
        
        for (_layer_type, layer_info) in &layer_manager.layers {
            if let Some(ref gerber_layer) = layer_info.gerber_layer {
                let bbox = gerber_layer.bounding_box();
                overall_bounds = Some(match overall_bounds {
                    None => bbox.clone(),
                    Some(existing) => gerber_viewer::BoundingBox {
                        min: nalgebra::Point2::new(
                            existing.min.x.min(bbox.min.x),
                            existing.min.y.min(bbox.min.y),
                        ),
                        max: nalgebra::Point2::new(
                            existing.max.x.max(bbox.max.x),
                            existing.max.y.max(bbox.max.y),
                        ),
                    },
                });
            }
        }
        
        // Create substrate
        if let Some(ref bounds) = overall_bounds {
            self.substrate_mesh = Some(converter.create_substrate_mesh(bounds));
            self.bounds_2d = (
                bounds.min.x as f32,
                bounds.min.y as f32,
                bounds.max.x as f32,
                bounds.max.y as f32,
            );
        }
        
        // Convert visible layers
        for (layer_type, layer_info) in &layer_manager.layers {
            if layer_info.visible {
                if let Some(ref gerber_layer) = layer_info.gerber_layer {
                    let mesh = converter.extrude_layer(gerber_layer, *layer_type);
                    self.layers.push(mesh);
                }
            }
        }
    }
    
    pub fn render(&mut self, ui: &mut Ui, rect: Rect) {
        let painter = ui.painter_at(rect);
        
        // Background
        painter.rect_filled(rect, 0.0, Color32::from_rgb(40, 40, 40));
        
        // Handle camera controls first (when not interacting with gizmo)
        let response = ui.allocate_rect(rect, egui::Sense::click_and_drag());
        
        // Camera controls - drag to rotate, scroll to zoom
        if response.dragged() {
            let delta = response.drag_delta();
            self.update_camera_rotation(delta.x * 0.01, delta.y * 0.01);
        }
        
        // Handle zoom with proper responsiveness
        if response.hovered() {
            ui.input(|i| {
                if i.raw_scroll_delta.y != 0.0 {
                    self.update_camera_zoom(i.raw_scroll_delta.y * 0.1);
                }
            });
        }
        
        // Set up 3D matrices exactly like the example
        self.draw_gizmo(ui);
        
        // Render the PCB
        self.render_pcb_meshes(&painter, rect);
        
        // Draw info overlay
        let mut info_text = format!(
            "🔥 3D PCB View (Transform-Gizmo)\n\
            Camera Distance: {:.1}\n\
            Layers: {}\n\
            🎮 Drag gizmo to transform PCB, drag background to rotate camera\n\
            📜 Hover and scroll to zoom",
            self.camera_distance,
            self.layers.len()
        );
        
        // Add gizmo operation info if available
        if let Some(ref operation) = self.last_gizmo_operation {
            info_text.push_str(&format!("\n🔧 {}", operation));
        }
        
        painter.text(
            rect.min + Vec2::new(10.0, 10.0),
            egui::Align2::LEFT_TOP,
            info_text,
            egui::FontId::default(),
            Color32::WHITE,
        );
    }
    
    // Copy the draw_gizmo function exactly from the working example
    fn draw_gizmo(&mut self, ui: &mut egui::Ui) {
        // Use ui.clip_rect() exactly like the working example
        let viewport = ui.clip_rect();

        let projection_matrix = DMat4::perspective_infinite_reverse_lh(
            std::f64::consts::PI / 4.0,
            (viewport.width() / viewport.height()).into(),
            0.1,
        );

        // Use the camera position from our camera system
        let camera_pos = self.get_camera_position();
        let view_matrix = DMat4::look_at_lh(camera_pos, DVec3::ZERO, DVec3::Y);

        // Ctrl toggles snapping
        let snapping = ui.input(|input| input.modifiers.ctrl);

        self.gizmo.update_config(GizmoConfig {
            view_matrix: view_matrix.into(),
            projection_matrix: projection_matrix.into(),
            viewport,
            modes: self.gizmo_modes,
            orientation: self.gizmo_orientation,
            snapping,
            ..Default::default()
        });

        let mut transform =
            Transform::from_scale_rotation_translation(self.scale, self.rotation, self.translation);

        if let Some((result, new_transforms)) = self.gizmo.interact(ui, &[transform]) {
            for (new_transform, transform) in
                new_transforms.iter().zip(std::iter::once(&mut transform))
            {
                *transform = *new_transform;
            }

            self.scale = transform.scale.into();
            self.rotation = transform.rotation.into();
            self.translation = transform.translation.into();

            let text = match result {
                GizmoResult::Rotation {
                    axis,
                    delta: _,
                    total,
                    is_view_axis: _,
                } => {
                    format!(
                        "Rotation axis: ({:.2}, {:.2}, {:.2}), Angle: {:.2} deg",
                        axis.x,
                        axis.y,
                        axis.z,
                        total.to_degrees()
                    )
                }
                GizmoResult::Translation { delta: _, total } => {
                    format!(
                        "Translation: ({:.2}, {:.2}, {:.2})",
                        total.x, total.y, total.z,
                    )
                }
                GizmoResult::Scale { total } => {
                    format!("Scale: ({:.2}, {:.2}, {:.2})", total.x, total.y, total.z,)
                }
                GizmoResult::Arcball { delta: _, total } => {
                    let (axis, angle) = DQuat::from(total).to_axis_angle();
                    format!(
                        "Rotation axis: ({:.2}, {:.2}, {:.2}), Angle: {:.2} deg",
                        axis.x,
                        axis.y,
                        axis.z,
                        angle.to_degrees()
                    )
                }
            };

            // Store the operation text for display in main render
            self.last_gizmo_operation = Some(text);
        }
    }
    
    fn get_camera_position(&self) -> DVec3 {
        let (phi, theta) = self.camera_rotation;
        let distance = self.camera_distance as f64;
        
        // Convert spherical coordinates to cartesian
        let x = distance * theta.sin() as f64 * phi.cos() as f64;
        let y = distance * theta.cos() as f64;
        let z = distance * theta.sin() as f64 * phi.sin() as f64;
        
        DVec3::new(x, y, z)
    }
    
    fn update_camera_rotation(&mut self, delta_x: f32, delta_y: f32) {
        let (mut phi, mut theta) = self.camera_rotation;
        
        phi += delta_x;
        theta += delta_y;
        
        // Clamp theta to avoid gimbal lock
        theta = theta.clamp(0.1, std::f32::consts::PI - 0.1);
        
        self.camera_rotation = (phi, theta);
    }
    
    fn update_camera_zoom(&mut self, delta: f32) {
        // Simple, responsive zoom
        self.camera_distance -= delta * 5.0; // Simple fixed speed
        self.camera_distance = self.camera_distance.clamp(10.0, 300.0); // Reasonable range
    }
    
    fn render_pcb_meshes(&self, painter: &egui::Painter, rect: Rect) {
        // Get the current transform
        let transform = Transform::from_scale_rotation_translation(
            self.scale,
            self.rotation,
            self.translation,
        );
        
        // Convert transform to nalgebra matrix
        let transform_matrix = self.gizmo_transform_to_matrix4(transform);
        
        // Camera and projection setup
        let camera_pos = self.get_camera_position();
        let view_matrix = DMat4::look_at_lh(camera_pos, DVec3::ZERO, DVec3::Y);
        let projection_matrix = DMat4::perspective_infinite_reverse_lh(
            std::f64::consts::PI / 4.0,
            (rect.width() / rect.height()) as f64,
            0.1,
        );
        
        // Project 3D vertices to 2D screen coordinates
        let project_vertex = |vertex: [f32; 3]| -> Option<Pos2> {
            let point = Point3::new(vertex[0], vertex[1], vertex[2]);
            
            // Apply PCB transform
            let transformed_point = transform_matrix.transform_point(&point);
            
            // Convert to DVec3 for gizmo math
            let world_pos = DVec3::new(
                transformed_point.x as f64,
                transformed_point.y as f64,
                transformed_point.z as f64,
            );
            
            // Apply view and projection
            let mvp = projection_matrix * view_matrix;
            let mut clip_pos = mvp * world_pos.extend(1.0);
            
            if clip_pos.w > 0.0 {
                clip_pos /= clip_pos.w;
                clip_pos.y *= -1.0; // Flip Y for screen space
                
                let screen_x = rect.center().x + (clip_pos.x as f32) * rect.width() / 2.0;
                let screen_y = rect.center().y + (clip_pos.y as f32) * rect.height() / 2.0;
                
                Some(Pos2::new(screen_x, screen_y))
            } else {
                None
            }
        };
        
        // Center the PCB for better viewing
        let pcb_center_x = (self.bounds_2d.0 + self.bounds_2d.2) / 2.0;
        let pcb_center_y = (self.bounds_2d.1 + self.bounds_2d.3) / 2.0;
        
        // Collect triangles with depth for sorting
        let mut triangles = Vec::new();
        
        // Render substrate
        if let Some(ref substrate) = self.substrate_mesh {
            for chunk in substrate.indices.chunks(3) {
                let v1 = substrate.vertices[chunk[0] as usize];
                let v2 = substrate.vertices[chunk[1] as usize];
                let v3 = substrate.vertices[chunk[2] as usize];
                
                // Center vertices
                let v1 = [v1[0] - pcb_center_x, v1[1] - pcb_center_y, v1[2]];
                let v2 = [v2[0] - pcb_center_x, v2[1] - pcb_center_y, v2[2]];
                let v3 = [v3[0] - pcb_center_x, v3[1] - pcb_center_y, v3[2]];
                
                if let (Some(p1), Some(p2), Some(p3)) = (
                    project_vertex(v1),
                    project_vertex(v2),
                    project_vertex(v3),
                ) {
                    let z_avg = (v1[2] + v2[2] + v3[2]) / 3.0;
                    let color = Color32::from_rgba_unmultiplied(
                        (substrate.color[0] * 255.0) as u8,
                        (substrate.color[1] * 255.0) as u8,
                        (substrate.color[2] * 255.0) as u8,
                        (substrate.color[3] * 255.0) as u8,
                    );
                    triangles.push((vec![p1, p2, p3], z_avg, color));
                }
            }
        }
        
        // Render layers
        for layer in &self.layers {
            for chunk in layer.indices.chunks(3) {
                let v1 = layer.vertices[chunk[0] as usize];
                let v2 = layer.vertices[chunk[1] as usize];
                let v3 = layer.vertices[chunk[2] as usize];
                
                // Center vertices
                let v1 = [v1[0] - pcb_center_x, v1[1] - pcb_center_y, v1[2]];
                let v2 = [v2[0] - pcb_center_x, v2[1] - pcb_center_y, v2[2]];
                let v3 = [v3[0] - pcb_center_x, v3[1] - pcb_center_y, v3[2]];
                
                if let (Some(p1), Some(p2), Some(p3)) = (
                    project_vertex(v1),
                    project_vertex(v2),
                    project_vertex(v3),
                ) {
                    let z_avg = (v1[2] + v2[2] + v3[2]) / 3.0;
                    let color = Color32::from_rgba_unmultiplied(
                        (layer.color[0] * 255.0) as u8,
                        (layer.color[1] * 255.0) as u8,
                        (layer.color[2] * 255.0) as u8,
                        (layer.color[3] * 255.0) as u8,
                    );
                    triangles.push((vec![p1, p2, p3], z_avg, color));
                }
            }
        }
        
        // Sort triangles by depth (back to front)
        triangles.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        
        // Draw sorted triangles
        for (vertices, _depth, color) in triangles {
            if vertices.len() >= 3 {
                painter.add(egui::Shape::convex_polygon(
                    vertices,
                    color,
                    Stroke::new(0.3, Color32::from_rgba_unmultiplied(0, 0, 0, 50)),
                ));
            }
        }
    }
    
    fn gizmo_transform_to_matrix4(&self, transform: Transform) -> Matrix4<f32> {
        // Convert DVec3 to nalgebra Vector3
        let translation = Vector3::new(
            transform.translation.x as f32,
            transform.translation.y as f32,
            transform.translation.z as f32,
        );
        
        // Convert mint::Quaternion to nalgebra UnitQuaternion
        let rotation = nalgebra::UnitQuaternion::new_normalize(nalgebra::Quaternion::new(
            transform.rotation.s as f32,
            transform.rotation.v.x as f32,
            transform.rotation.v.y as f32,
            transform.rotation.v.z as f32,
        ));
        
        // Convert DVec3 to nalgebra Vector3
        let scale = Vector3::new(
            transform.scale.x as f32,
            transform.scale.y as f32,
            transform.scale.z as f32,
        );
        
        // Create the transformation matrix
        let scale_matrix = Matrix4::new_nonuniform_scaling(&scale);
        let rotation_matrix = rotation.to_homogeneous();
        let translation_matrix = Matrix4::new_translation(&translation);
        
        translation_matrix * rotation_matrix * scale_matrix
    }
}