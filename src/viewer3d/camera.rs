//! 3D Camera system for the PCB viewer
//! 
//! Provides camera controls and projection management for 3D visualization.

use nalgebra::{Matrix4, Point3, Vector3};
use std::f32::consts::PI;

/// 3D Camera for PCB visualization with orbit controls
#[derive(Clone, Debug)]
pub struct Camera3D {
    pub eye: Point3<f32>,
    pub target: Point3<f32>,
    pub up: Vector3<f32>,
    pub fovy: f32,
    pub znear: f32,
    pub zfar: f32,
    pub aspect: f32,
    
    // Orbit parameters for smooth control
    pub distance: f32,
    pub azimuth: f32,   // Horizontal angle (radians)
    pub elevation: f32, // Vertical angle (radians)
}

impl Camera3D {
    pub fn new() -> Self {
        let mut camera = Self {
            eye: Point3::new(0.0, 0.0, 5.0),
            target: Point3::new(0.0, 0.0, 0.0),
            up: Vector3::new(0.0, 1.0, 0.0),
            fovy: 45.0,
            znear: 0.1,
            zfar: 100.0,
            aspect: 1.0,
            distance: 5.0,
            azimuth: 0.0,
            elevation: 0.0,
        };
        camera.update_from_orbit();
        camera
    }
    
    /// Update eye position from orbit parameters
    pub fn update_from_orbit(&mut self) {
        let x = self.target.x + self.distance * self.elevation.cos() * self.azimuth.cos();
        let y = self.target.y + self.distance * self.elevation.sin();
        let z = self.target.z + self.distance * self.elevation.cos() * self.azimuth.sin();
        
        self.eye = Point3::new(x, y, z);
        
        // Update up vector based on elevation
        if self.elevation.abs() > PI * 0.49 {
            // Near poles, flip up vector
            self.up = Vector3::new(0.0, -self.elevation.signum(), 0.0);
        } else {
            self.up = Vector3::new(0.0, 1.0, 0.0);
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
        self.elevation = PI * 0.49; // Almost straight down
        self.azimuth = 0.0;
        self.distance = 5.0;
        self.update_from_orbit();
    }

    /// Set to side view
    pub fn set_side_view(&mut self) {
        self.elevation = 0.0;
        self.azimuth = 0.0;
        self.distance = 5.0;
        self.update_from_orbit();
    }

    /// Set to isometric view
    pub fn set_isometric_view(&mut self) {
        self.elevation = PI * 0.2; // 36 degrees up
        self.azimuth = PI * 0.25;  // 45 degrees around
        self.distance = 7.0;
        self.update_from_orbit();
    }
    
    /// Set to front view
    pub fn set_front_view(&mut self) {
        self.elevation = 0.0;
        self.azimuth = PI; // 180 degrees
        self.distance = 5.0;
        self.update_from_orbit();
    }
    
    /// Set to right view
    pub fn set_right_view(&mut self) {
        self.elevation = 0.0;
        self.azimuth = PI * 0.5; // 90 degrees
        self.distance = 5.0;
        self.update_from_orbit();
    }
    
    /// Reset to default view
    pub fn reset(&mut self) {
        self.target = Point3::new(0.0, 0.0, 0.0);
        self.distance = 5.0;
        self.azimuth = 0.0;
        self.elevation = 0.0;
        self.fovy = 45.0;
        self.update_from_orbit();
    }
    
    /// Frame the given bounding box
    pub fn frame_bounds(&mut self, min: Point3<f32>, max: Point3<f32>) {
        // Calculate center and size of bounding box
        let center = Point3::new(
            (min.x + max.x) * 0.5,
            (min.y + max.y) * 0.5,
            (min.z + max.z) * 0.5,
        );
        
        let size = Vector3::new(
            (max.x - min.x).abs(),
            (max.y - min.y).abs(),
            (max.z - min.z).abs(),
        );
        
        // Set target to center
        self.target = center;
        
        // Calculate distance to fit the object
        let max_dim = size.x.max(size.y).max(size.z);
        self.distance = max_dim * 2.0; // Factor for comfortable viewing
        
        self.update_from_orbit();
    }
}

/// Camera controller for handling user input with advanced orbit controls
pub struct CameraController {
    // Sensitivity settings
    pub orbit_sensitivity: f32,
    pub zoom_sensitivity: f32,
    pub pan_sensitivity: f32,
    pub zoom_wheel_sensitivity: f32,
    
    // Constraints
    pub min_distance: f32,
    pub max_distance: f32,
    pub min_elevation: f32,
    pub max_elevation: f32,
    
    // Smoothing
    pub enable_smoothing: bool,
    pub smoothing_factor: f32,
    
    // State for smooth transitions
    target_azimuth: f32,
    target_elevation: f32,
    target_distance: f32,
    target_center: Point3<f32>,
}

impl CameraController {
    pub fn new() -> Self {
        Self {
            orbit_sensitivity: 0.01,
            zoom_sensitivity: 0.1,
            pan_sensitivity: 0.01,
            zoom_wheel_sensitivity: 0.1,
            
            min_distance: 0.1,
            max_distance: 100.0,
            min_elevation: -PI * 0.49,
            max_elevation: PI * 0.49,
            
            enable_smoothing: true,
            smoothing_factor: 0.15,
            
            target_azimuth: 0.0,
            target_elevation: 0.0,
            target_distance: 5.0,
            target_center: Point3::new(0.0, 0.0, 0.0),
        }
    }
    
    /// Initialize targets from current camera state
    pub fn sync_with_camera(&mut self, camera: &Camera3D) {
        self.target_azimuth = camera.azimuth;
        self.target_elevation = camera.elevation;
        self.target_distance = camera.distance;
        self.target_center = camera.target;
    }

    /// Handle orbit rotation (mouse drag)
    pub fn orbit(&mut self, camera: &mut Camera3D, delta_x: f32, delta_y: f32) {
        self.target_azimuth -= delta_x * self.orbit_sensitivity;
        self.target_elevation += delta_y * self.orbit_sensitivity;
        
        // Wrap azimuth to [-PI, PI]
        while self.target_azimuth > PI {
            self.target_azimuth -= 2.0 * PI;
        }
        while self.target_azimuth < -PI {
            self.target_azimuth += 2.0 * PI;
        }
        
        // Clamp elevation to prevent gimbal lock
        self.target_elevation = self.target_elevation.clamp(self.min_elevation, self.max_elevation);
        
        self.apply_to_camera(camera);
    }
    
    /// Handle zoom (scroll wheel or drag)
    pub fn zoom(&mut self, camera: &mut Camera3D, delta: f32) {
        let zoom_factor = if delta > 0.0 {
            1.0 + delta * self.zoom_wheel_sensitivity
        } else {
            1.0 / (1.0 - delta * self.zoom_wheel_sensitivity)
        };
        
        self.target_distance *= zoom_factor;
        self.target_distance = self.target_distance.clamp(self.min_distance, self.max_distance);
        
        self.apply_to_camera(camera);
    }
    
    /// Handle panning (shift+drag)
    pub fn pan(&mut self, camera: &mut Camera3D, delta_x: f32, delta_y: f32) {
        // Calculate pan vectors in screen space
        let forward = (camera.target - camera.eye).normalize();
        let right = forward.cross(&camera.up).normalize();
        let up = right.cross(&forward).normalize();
        
        // Scale pan speed by distance for consistent feel
        let pan_scale = self.pan_sensitivity * self.target_distance * 0.1;
        let pan_offset = right * (-delta_x * pan_scale) + up * (delta_y * pan_scale);
        
        self.target_center += pan_offset;
        
        self.apply_to_camera(camera);
    }
    
    /// Apply smooth interpolation or direct update to camera
    fn apply_to_camera(&self, camera: &mut Camera3D) {
        if self.enable_smoothing {
            // Smooth interpolation
            camera.azimuth = lerp(camera.azimuth, self.target_azimuth, self.smoothing_factor);
            camera.elevation = lerp(camera.elevation, self.target_elevation, self.smoothing_factor);
            camera.distance = lerp(camera.distance, self.target_distance, self.smoothing_factor);
            camera.target = lerp_point3(camera.target, self.target_center, self.smoothing_factor);
        } else {
            // Direct update
            camera.azimuth = self.target_azimuth;
            camera.elevation = self.target_elevation;
            camera.distance = self.target_distance;
            camera.target = self.target_center;
        }
        
        camera.update_from_orbit();
    }
    
    /// Update smoothing (call every frame)
    pub fn update(&self, camera: &mut Camera3D) {
        if self.enable_smoothing {
            self.apply_to_camera(camera);
        }
    }
    
    /// Handle all input types in one call
    pub fn handle_input(&mut self, camera: &mut Camera3D, input: CameraInput) {
        match input {
            CameraInput::Orbit { delta_x, delta_y } => {
                self.orbit(camera, delta_x, delta_y);
            }
            CameraInput::Pan { delta_x, delta_y } => {
                self.pan(camera, delta_x, delta_y);
            }
            CameraInput::Zoom { delta } => {
                self.zoom(camera, delta);
            }
            CameraInput::FrameBounds { min, max } => {
                camera.frame_bounds(min, max);
                self.sync_with_camera(camera);
            }
        }
    }
    
    /// Set view presets with smooth transitions
    pub fn set_preset(&mut self, camera: &mut Camera3D, preset: ViewPreset) {
        match preset {
            ViewPreset::Reset => {
                self.target_center = Point3::new(0.0, 0.0, 0.0);
                self.target_distance = 5.0;
                self.target_azimuth = 0.0;
                self.target_elevation = 0.0;
            }
            ViewPreset::Top => {
                self.target_elevation = PI * 0.49;
                self.target_azimuth = 0.0;
                self.target_distance = 5.0;
            }
            ViewPreset::Front => {
                self.target_elevation = 0.0;
                self.target_azimuth = PI;
                self.target_distance = 5.0;
            }
            ViewPreset::Right => {
                self.target_elevation = 0.0;
                self.target_azimuth = PI * 0.5;
                self.target_distance = 5.0;
            }
            ViewPreset::Isometric => {
                self.target_elevation = PI * 0.2;
                self.target_azimuth = PI * 0.25;
                self.target_distance = 7.0;
            }
        }
        
        self.apply_to_camera(camera);
    }
}

/// Camera input types
#[derive(Debug, Clone)]
pub enum CameraInput {
    Orbit { delta_x: f32, delta_y: f32 },
    Pan { delta_x: f32, delta_y: f32 },
    Zoom { delta: f32 },
    FrameBounds { min: Point3<f32>, max: Point3<f32> },
}

/// View presets
#[derive(Debug, Clone, Copy)]
pub enum ViewPreset {
    Reset,
    Top,
    Front,
    Right,
    Isometric,
}

/// Linear interpolation for f32
fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

/// Linear interpolation for Point3
fn lerp_point3(a: Point3<f32>, b: Point3<f32>, t: f32) -> Point3<f32> {
    Point3::new(
        lerp(a.x, b.x, t),
        lerp(a.y, b.y, t),
        lerp(a.z, b.z, t),
    )
}