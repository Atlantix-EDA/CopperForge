use bevy_ecs::prelude::*;
use nalgebra::{Point3, Vector3, Matrix4};
use std::collections::HashMap;
use crate::ecs::{Mesh3D, Position3D, Transform3D, MaterialProperties, StackupLayer};
use crate::layer_operations::LayerType;

/// 3D rendering system for PCB visualization
#[derive(Component, Debug, Clone)]
pub struct Renderable3D {
    /// Whether this entity should be rendered
    pub visible: bool,
    /// Rendering layer/priority
    pub render_layer: u32,
    /// Material override
    pub material_override: Option<u32>,
    /// Transparency (0.0 = transparent, 1.0 = opaque)
    pub alpha: f32,
}

impl Default for Renderable3D {
    fn default() -> Self {
        Self {
            visible: true,
            render_layer: 0,
            material_override: None,
            alpha: 1.0,
        }
    }
}

/// 3D camera for viewing the PCB
#[derive(Debug, Clone)]
pub struct Camera3D {
    /// Camera position in world space
    pub position: Point3<f32>,
    /// Camera target (look-at point)
    pub target: Point3<f32>,
    /// Up vector
    pub up: Vector3<f32>,
    /// Field of view in degrees
    pub fov: f32,
    /// Near clipping plane
    pub near: f32,
    /// Far clipping plane
    pub far: f32,
    /// Viewport aspect ratio (width/height)
    pub aspect_ratio: f32,
}

impl Default for Camera3D {
    fn default() -> Self {
        Self {
            position: Point3::new(0.0, 0.0, 100.0),
            target: Point3::new(0.0, 0.0, 0.0),
            up: Vector3::new(0.0, 1.0, 0.0),
            fov: 45.0,
            near: 0.1,
            far: 1000.0,
            aspect_ratio: 16.0 / 9.0,
        }
    }
}

impl Camera3D {
    /// Calculate view matrix
    pub fn view_matrix(&self) -> Matrix4<f32> {
        let forward = (self.target - self.position).normalize();
        let right = forward.cross(&self.up).normalize();
        let up = right.cross(&forward);

        Matrix4::look_at_rh(&self.position, &self.target, &up)
    }

    /// Calculate projection matrix
    pub fn projection_matrix(&self) -> Matrix4<f32> {
        Matrix4::new_perspective(
            self.aspect_ratio,
            self.fov.to_radians(),
            self.near,
            self.far,
        )
    }

    /// Orbit around target point
    pub fn orbit(&mut self, delta_theta: f32, delta_phi: f32) {
        let offset = self.position - self.target;
        let radius = offset.magnitude();
        
        // Convert to spherical coordinates
        let mut theta = offset.z.atan2(offset.x);
        let mut phi = (offset.y / radius).acos();
        
        // Apply deltas
        theta += delta_theta;
        phi = (phi + delta_phi).clamp(0.1, std::f32::consts::PI - 0.1);
        
        // Convert back to Cartesian
        let new_offset = Vector3::new(
            radius * phi.sin() * theta.cos(),
            radius * phi.cos(),
            radius * phi.sin() * theta.sin(),
        );
        
        self.position = self.target + new_offset;
    }

    /// Zoom in/out by moving closer to or further from target
    pub fn zoom(&mut self, factor: f32) {
        let direction = (self.position - self.target).normalize();
        let distance = (self.position - self.target).magnitude();
        let new_distance = (distance * factor).max(1.0).min(500.0);
        self.position = self.target + direction * new_distance;
    }

    /// Pan the camera (move target and position together)
    pub fn pan(&mut self, delta_x: f32, delta_y: f32) {
        let forward = (self.target - self.position).normalize();
        let right = forward.cross(&self.up).normalize();
        let up = right.cross(&forward);
        
        let offset = right * delta_x + up * delta_y;
        self.position += offset;
        self.target += offset;
    }
}

/// Material definition for 3D rendering
#[derive(Debug, Clone)]
pub struct RenderMaterial {
    pub id: u32,
    pub name: String,
    pub diffuse_color: [f32; 4], // RGBA
    pub metallic: f32,
    pub roughness: f32,
    pub emission: [f32; 3], // RGB
    pub transparent: bool,
}

impl RenderMaterial {
    /// Create copper material
    pub fn copper() -> Self {
        Self {
            id: 1,
            name: "Copper".to_string(),
            diffuse_color: [0.72, 0.45, 0.20, 1.0], // Copper color
            metallic: 0.9,
            roughness: 0.1,
            emission: [0.0, 0.0, 0.0],
            transparent: false,
        }
    }

    /// Create soldermask material
    pub fn soldermask(color: [f32; 4]) -> Self {
        Self {
            id: 2,
            name: "Soldermask".to_string(),
            diffuse_color: color,
            metallic: 0.0,
            roughness: 0.8,
            emission: [0.0, 0.0, 0.0],
            transparent: color[3] < 1.0,
        }
    }

    /// Create silkscreen material
    pub fn silkscreen() -> Self {
        Self {
            id: 3,
            name: "Silkscreen".to_string(),
            diffuse_color: [1.0, 1.0, 1.0, 1.0], // White
            metallic: 0.0,
            roughness: 0.9,
            emission: [0.0, 0.0, 0.0],
            transparent: false,
        }
    }

    /// Create FR4 substrate material
    pub fn fr4() -> Self {
        Self {
            id: 5,
            name: "FR4".to_string(),
            diffuse_color: [0.2, 0.3, 0.1, 1.0], // Dark green
            metallic: 0.0,
            roughness: 0.7,
            emission: [0.0, 0.0, 0.0],
            transparent: false,
        }
    }
}

/// 3D rendering system
pub struct Renderer3D {
    camera: Camera3D,
    materials: HashMap<u32, RenderMaterial>,
    lighting: LightingSetup,
    render_stats: RenderStats,
}

impl Renderer3D {
    pub fn new() -> Self {
        let mut materials = HashMap::new();
        materials.insert(1, RenderMaterial::copper());
        materials.insert(2, RenderMaterial::soldermask([0.0, 0.5, 0.0, 0.8])); // Green soldermask
        materials.insert(3, RenderMaterial::silkscreen());
        materials.insert(5, RenderMaterial::fr4());

        Self {
            camera: Camera3D::default(),
            materials,
            lighting: LightingSetup::default(),
            render_stats: RenderStats::default(),
        }
    }

    /// Get mutable reference to camera
    pub fn camera_mut(&mut self) -> &mut Camera3D {
        &mut self.camera
    }

    /// Get camera reference
    pub fn camera(&self) -> &Camera3D {
        &self.camera
    }

    /// Add or update a material
    pub fn add_material(&mut self, material: RenderMaterial) {
        self.materials.insert(material.id, material);
    }

    /// Get material by ID
    pub fn get_material(&self, id: u32) -> Option<&RenderMaterial> {
        self.materials.get(&id)
    }

    /// Update camera aspect ratio
    pub fn set_aspect_ratio(&mut self, aspect_ratio: f32) {
        self.camera.aspect_ratio = aspect_ratio;
    }

    /// Frame the entire PCB in view
    pub fn frame_pcb(&mut self, bbox_min: Point3<f32>, bbox_max: Point3<f32>) {
        let center = Point3::new(
            (bbox_min.x + bbox_max.x) / 2.0,
            (bbox_min.y + bbox_max.y) / 2.0,
            (bbox_min.z + bbox_max.z) / 2.0,
        );
        
        let size = bbox_max - bbox_min;
        let max_size = size.x.max(size.y).max(size.z);
        let distance = max_size / (self.camera.fov.to_radians() / 2.0).tan() * 1.5;
        
        self.camera.target = center;
        self.camera.position = center + Vector3::new(distance, distance, distance);
    }

    /// Render a single mesh with transform
    pub fn render_mesh(
        &mut self,
        mesh: &Mesh3D,
        position: &Position3D,
        transform: &Transform3D,
        renderable: &Renderable3D,
    ) -> Result<(), RenderError> {
        if !renderable.visible {
            return Ok(());
        }

        // Transform mesh vertices
        let world_matrix = self.calculate_world_matrix(position, transform);
        let view_matrix = self.camera.view_matrix();
        let proj_matrix = self.camera.projection_matrix();
        let mvp_matrix = proj_matrix * view_matrix * world_matrix;

        // Get material
        let material_id = renderable.material_override
            .or(mesh.material_id)
            .unwrap_or(1);

        let material = self.materials.get(&material_id)
            .ok_or(RenderError::MaterialNotFound(material_id))?;

        // For now, just count rendering operations
        // In a real implementation, this would submit geometry to GPU
        self.render_stats.meshes_rendered += 1;
        self.render_stats.triangles_rendered += mesh.indices.len() / 3;
        self.render_stats.vertices_processed += mesh.vertices.len();

        log::debug!("Rendered mesh with {} vertices, {} triangles, material: {}", 
                   mesh.vertices.len(), mesh.indices.len() / 3, material.name);

        Ok(())
    }

    /// Calculate world transformation matrix
    fn calculate_world_matrix(&self, position: &Position3D, transform: &Transform3D) -> Matrix4<f32> {
        let translation = Matrix4::new_translation(&Vector3::new(
            position.x as f32,
            position.y as f32, 
            position.z as f32,
        ));

        let rotation_z = Matrix4::new_rotation(Vector3::new(0.0, 0.0, transform.z_rotation));
        let rotation_x = Matrix4::new_rotation(Vector3::new(transform.rotation.x, 0.0, 0.0));
        let rotation_y = Matrix4::new_rotation(Vector3::new(0.0, transform.rotation.y, 0.0));

        let scale = Matrix4::new_nonuniform_scaling(&Vector3::new(
            transform.scale.x,
            transform.scale.y,
            1.0, // Z scale is usually 1.0 for PCB layers
        ));

        translation * rotation_z * rotation_y * rotation_x * scale
    }

    /// Get rendering statistics
    pub fn get_stats(&self) -> &RenderStats {
        &self.render_stats
    }

    /// Reset statistics
    pub fn reset_stats(&mut self) {
        self.render_stats = RenderStats::default();
    }
}

/// Lighting setup for 3D scene
#[derive(Debug, Clone)]
pub struct LightingSetup {
    pub ambient_color: [f32; 3],
    pub ambient_intensity: f32,
    pub directional_lights: Vec<DirectionalLight>,
}

impl Default for LightingSetup {
    fn default() -> Self {
        Self {
            ambient_color: [0.2, 0.2, 0.2],
            ambient_intensity: 0.3,
            directional_lights: vec![
                DirectionalLight {
                    direction: Vector3::new(-0.5, -1.0, -0.5).normalize(),
                    color: [1.0, 1.0, 1.0],
                    intensity: 0.8,
                },
                DirectionalLight {
                    direction: Vector3::new(0.5, -0.3, 0.8).normalize(),
                    color: [0.8, 0.9, 1.0],
                    intensity: 0.4,
                },
            ],
        }
    }
}

#[derive(Debug, Clone)]
pub struct DirectionalLight {
    pub direction: Vector3<f32>,
    pub color: [f32; 3],
    pub intensity: f32,
}

/// Rendering statistics
#[derive(Debug, Default)]
pub struct RenderStats {
    pub meshes_rendered: usize,
    pub triangles_rendered: usize,
    pub vertices_processed: usize,
    pub draw_calls: usize,
    pub materials_used: usize,
}

/// 3D rendering system for ECS (simplified to avoid Resource issues)
pub fn render_3d_system_simple(
    query: &Query<(&Mesh3D, &Position3D, &Transform3D, &Renderable3D)>,
    renderer: &mut Renderer3D,
) {
    renderer.reset_stats();

    for (mesh, position, transform, renderable) in query.iter() {
        if let Err(e) = renderer.render_mesh(mesh, position, transform, renderable) {
            log::warn!("Failed to render mesh: {}", e);
        }
    }

    let stats = renderer.get_stats();
    log::debug!("3D Render frame: {} meshes, {} triangles, {} vertices",
              stats.meshes_rendered, stats.triangles_rendered, stats.vertices_processed);
}

/// System to update 3D transforms based on layer stackup (simplified)
pub fn update_layer_transforms_system_simple(
    query: &mut Query<(&mut Position3D, &StackupLayer)>,
) {
    for (mut position, stackup) in query.iter_mut() {
        // Update Z position based on stackup
        position.z = (stackup.z_bottom + stackup.z_top) / 2.0;
    }
}

/// System to handle layer visibility (simplified)
pub fn layer_visibility_system_simple(
    query: &mut Query<(&mut Renderable3D, &crate::ecs::GerberLayerComponent)>,
) {
    for (mut renderable, layer_component) in query.iter_mut() {
        renderable.visible = layer_component.visible;
        
        // Set transparency for soldermask layers
        match layer_component.layer_type {
            LayerType::TopSoldermask | LayerType::BottomSoldermask => {
                renderable.alpha = 0.8;
            }
            _ => {
                renderable.alpha = 1.0;
            }
        }
    }
}

#[derive(Debug, Clone)]
pub enum RenderError {
    MaterialNotFound(u32),
    InvalidMesh(String),
    ShaderError(String),
    BufferError(String),
}

impl std::fmt::Display for RenderError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RenderError::MaterialNotFound(id) => write!(f, "Material not found: {}", id),
            RenderError::InvalidMesh(msg) => write!(f, "Invalid mesh: {}", msg),
            RenderError::ShaderError(msg) => write!(f, "Shader error: {}", msg),
            RenderError::BufferError(msg) => write!(f, "Buffer error: {}", msg),
        }
    }
}

impl std::error::Error for RenderError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_camera_orbit() {
        let mut camera = Camera3D::default();
        let initial_position = camera.position;
        
        camera.orbit(0.1, 0.1);
        
        // Position should have changed
        assert_ne!(camera.position, initial_position);
        
        // Distance from target should remain roughly the same
        let initial_distance = (initial_position - camera.target).magnitude();
        let new_distance = (camera.position - camera.target).magnitude();
        assert!((initial_distance - new_distance).abs() < 0.01);
    }

    #[test]
    fn test_camera_zoom() {
        let mut camera = Camera3D::default();
        let initial_distance = (camera.position - camera.target).magnitude();
        
        camera.zoom(0.5); // Zoom in
        let new_distance = (camera.position - camera.target).magnitude();
        
        assert!(new_distance < initial_distance);
    }

    #[test]
    fn test_material_creation() {
        let copper = RenderMaterial::copper();
        assert_eq!(copper.id, 1);
        assert_eq!(copper.name, "Copper");
        assert!(copper.metallic > 0.5);
    }

    #[test]
    fn test_render_stats() {
        let mut renderer = Renderer3D::new();
        assert_eq!(renderer.get_stats().meshes_rendered, 0);
        
        renderer.reset_stats();
        assert_eq!(renderer.get_stats().triangles_rendered, 0);
    }
}