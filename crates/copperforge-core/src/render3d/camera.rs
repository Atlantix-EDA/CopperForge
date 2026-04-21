use nalgebra::{Matrix4, Perspective3, Translation3, UnitQuaternion, Vector3};

/// Simple orbit camera: rotates the world about the origin, viewed from a
/// fixed distance `zoom` along -Z. Pan reserved for Phase 3+.
pub struct Camera {
    pub rotation: UnitQuaternion<f32>,
    pub zoom: f32,
}

impl Default for Camera {
    fn default() -> Self {
        // Tilt the world ~55° forward so the XY plane reads as a receding
        // floor rather than edge-on, and pull the camera back far enough to
        // see the 10×10 ground grid.
        let tilt = UnitQuaternion::from_axis_angle(&Vector3::x_axis(), -55f32.to_radians());
        Self {
            rotation: tilt,
            zoom: 12.0,
        }
    }
}

impl Camera {
    /// Build `P * V * I` (identity model). Caller multiplies per-object M
    /// in later phases.
    pub fn mvp(&self, viewport: egui::Rect) -> Matrix4<f32> {
        let aspect = (viewport.width() / viewport.height().max(1.0)).max(0.01);
        let proj = Perspective3::new(aspect, 60f32.to_radians(), 0.1, 10_000.0);
        let view = Translation3::new(0.0, 0.0, -self.zoom).to_homogeneous()
            * self.rotation.to_homogeneous();
        proj.as_matrix() * view
    }

    /// Orbit by a screen-space drag delta (pixels). Yaw about world Y,
    /// pitch about world X.
    pub fn orbit(&mut self, drag_delta: egui::Vec2) {
        let yaw = UnitQuaternion::from_axis_angle(&Vector3::y_axis(), drag_delta.x * 0.01);
        let pitch = UnitQuaternion::from_axis_angle(&Vector3::x_axis(), drag_delta.y * 0.01);
        self.rotation = yaw * pitch * self.rotation;
    }

    /// Multiplicative zoom: >1 → closer, <1 → farther.
    pub fn zoom_by(&mut self, factor: f32) {
        if factor <= 0.0 {
            return;
        }
        self.zoom = (self.zoom / factor).clamp(0.3, 500.0);
    }
}
