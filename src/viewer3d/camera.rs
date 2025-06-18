//! 3D Camera system for the PCB viewer
//! 
//! Provides camera controls and projection management for 3D visualization.

use nalgebra::{Matrix4, Point3, Vector3};

/// 3D Camera for PCB visualization
#[derive(Clone, Debug)]
pub struct Camera3D {
    pub eye: Point3<f32>,
    pub target: Point3<f32>,
    pub up: Vector3<f32>,
    pub fovy: f32,
    pub znear: f32,
    pub zfar: f32,
    pub aspect: f32,
}

impl Camera3D {
    pub fn new() -> Self {
        Self {
            eye: Point3::new(0.0, 0.0, 5.0),
            target: Point3::new(0.0, 0.0, 0.0),
            up: Vector3::new(0.0, 1.0, 0.0),
            fovy: 45.0,
            znear: 0.1,
            zfar: 100.0,
            aspect: 1.0,
        }
    }

    /// Build the view-projection matrix
    pub fn build_view_projection_matrix(&self) -> Matrix4<f32> {
        let view = Matrix4::look_at_rh(&self.eye, &self.target, &self.up);
        let proj = Matrix4::new_perspective(self.aspect, self.fovy.to_radians(), self.znear, self.zfar);
        proj * view
    }

    /// Update the aspect ratio
    pub fn update_aspect(&mut self, width: f32, height: f32) {
        self.aspect = width / height;
    }

    /// Set to top view
    pub fn set_top_view(&mut self) {
        self.eye = Point3::new(0.0, 5.0, 0.0);
        self.target = Point3::new(0.0, 0.0, 0.0);
        self.up = Vector3::new(0.0, 0.0, -1.0);
    }

    /// Set to side view
    pub fn set_side_view(&mut self) {
        self.eye = Point3::new(5.0, 0.0, 0.0);
        self.target = Point3::new(0.0, 0.0, 0.0);
        self.up = Vector3::new(0.0, 1.0, 0.0);
    }

    /// Set to isometric view
    pub fn set_isometric_view(&mut self) {
        self.eye = Point3::new(3.0, 3.0, 3.0);
        self.target = Point3::new(0.0, 0.0, 0.0);
        self.up = Vector3::new(0.0, 1.0, 0.0);
    }
}

/// Camera controller for handling user input
pub struct CameraController {
    rotation_sensitivity: f32,
    zoom_sensitivity: f32,
    pan_sensitivity: f32,
}

impl CameraController {
    pub fn new() -> Self {
        Self {
            rotation_sensitivity: 0.01,
            zoom_sensitivity: 0.1,
            pan_sensitivity: 0.01,
        }
    }

    /// Handle rotation input
    pub fn handle_rotation(&self, camera: &mut Camera3D, delta_x: f32, delta_y: f32) {
        // Convert to spherical coordinates around target
        let radius = (camera.eye - camera.target).magnitude();
        
        // Calculate current angles
        let mut theta = (camera.eye.z - camera.target.z).atan2(camera.eye.x - camera.target.x);
        let mut phi = ((camera.eye.y - camera.target.y) / radius).asin();
        
        // Apply rotation
        theta -= delta_x * self.rotation_sensitivity;
        phi += delta_y * self.rotation_sensitivity;
        
        // Clamp phi to prevent flipping
        phi = phi.clamp(-std::f32::consts::PI / 2.0 + 0.1, std::f32::consts::PI / 2.0 - 0.1);
        
        // Convert back to cartesian
        camera.eye = Point3::new(
            camera.target.x + radius * phi.cos() * theta.cos(),
            camera.target.y + radius * phi.sin(),
            camera.target.z + radius * phi.cos() * theta.sin(),
        );
    }

    /// Handle zoom input
    pub fn handle_zoom(&self, camera: &mut Camera3D, zoom_delta: f32) {
        let direction = (camera.target - camera.eye).normalize();
        let zoom_amount = zoom_delta * self.zoom_sensitivity;
        
        // Move camera towards/away from target
        camera.eye += direction * zoom_amount;
        
        // Clamp distance to prevent going too close or too far
        let distance = (camera.eye - camera.target).magnitude();
        if distance < 0.1 {
            camera.eye = camera.target - direction * 0.1;
        } else if distance > 50.0 {
            camera.eye = camera.target - direction * 50.0;
        }
    }

    /// Handle pan input
    pub fn handle_pan(&self, camera: &mut Camera3D, delta_x: f32, delta_y: f32) {
        let forward = (camera.target - camera.eye).normalize();
        let right = forward.cross(&camera.up).normalize();
        let up = right.cross(&forward).normalize();
        
        let pan_offset = right * (-delta_x * self.pan_sensitivity) + up * (delta_y * self.pan_sensitivity);
        
        camera.eye += pan_offset;
        camera.target += pan_offset;
    }

    /// Convenience method to handle all input types
    pub fn handle_input(&self, camera: &mut Camera3D, delta_x: f32, delta_y: f32, zoom: f32, is_panning: bool) {
        if is_panning {
            self.handle_pan(camera, delta_x, delta_y);
        } else {
            self.handle_rotation(camera, delta_x, delta_y);
        }
        
        if zoom != 0.0 {
            self.handle_zoom(camera, zoom);
        }
    }
}