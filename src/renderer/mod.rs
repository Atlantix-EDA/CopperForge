//! WGPU-based PCB renderer module
//!
//! This module provides hardware-accelerated rendering for PCB visualization
//! using wgpu, integrated with the existing egui interface.

pub mod wgpu_renderer;
pub mod pcb_renderer;
pub mod egui_wgpu_widget;

pub use wgpu_renderer::{WgpuRenderer, WgpuRendererError};
pub use pcb_renderer::PcbRenderer;
pub use egui_wgpu_widget::{WgpuWidget, create_wgpu_widget};

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