//! 3D Viewer Module
//! 
//! This module provides 3D visualization capabilities for PCB designs using egui and wgpu.
//! It includes camera controls, mesh rendering, and interactive 3D navigation.

pub mod pcb_viewer;
pub mod camera;
pub mod renderer;
pub mod controls;

// Re-export main types for easy access
pub use pcb_viewer::PcbViewer;
pub use camera::{Camera3D, CameraController, CameraInput, ViewPreset};
pub use renderer::Renderer3D;
pub use controls::ViewerControls;