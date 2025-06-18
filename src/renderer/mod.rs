//! WGPU-based PCB renderer module
//!
//! This module provides hardware-accelerated rendering for PCB visualization
//! using wgpu, integrated with the existing egui interface.
//! 
//! It also includes 3D mesh generation and Gerber-to-3D conversion capabilities.

pub mod wgpu_renderer;
pub mod pcb_renderer;
pub mod egui_wgpu_widget;
pub mod mesh3d;
pub mod gerber_extrudable;
pub mod gerber_components;
pub mod pcb3d_integration;

pub use wgpu_renderer::{WgpuRenderer, WgpuRendererError};
pub use pcb_renderer::PcbRenderer;
pub use egui_wgpu_widget::{WgpuWidget, create_wgpu_widget};
pub use mesh3d::{Mesh3D, Polygon2D, ExtrusionEngine, ExtrusionError, MeshStats};
pub use gerber_extrudable::{Extrudable, layer_to_3d_meshes, combine_meshes};
pub use gerber_components::{
    GerberLayerComponent, StackupLayer, LayerTypeExt, MaterialProperties,
    Position3D, Transform3D, Renderable3D, LayerMesh
};
pub use pcb3d_integration::{
    Pcb3DSystem, Pcb3DGenerationResult, Pcb3DStatistics, 
    WgpuVertex, WgpuMeshData, Pcb3DError
};

/// Common types and utilities for rendering
#[derive(Debug, Clone, Copy)]
pub struct RenderSettings {
    /// Enable multisampling
    pub msaa_samples: u32,
    /// Enable depth testing
    pub depth_test: bool,
    /// Background color
    pub background_color: [f32; 4],
}

impl Default for RenderSettings {
    fn default() -> Self {
        Self {
            msaa_samples: 4,
            depth_test: true,
            background_color: [0.1, 0.1, 0.1, 1.0], // Dark gray background
        }
    }
}