use bevy_ecs::prelude::*;
use gerber_viewer::ViewState;
use super::{LayerType, LayerDetector, UnassignedGerber};
use std::collections::HashMap;

// Simple view mode enum
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ViewMode {
    Normal,
    Quadrant,
}

// Wrapper for gerber_viewer's ViewState
#[derive(Resource, Clone)]
pub struct ViewStateResource {
    pub view_state: ViewState,
    pub view_mode: ViewMode,
}

impl Default for ViewStateResource {
    fn default() -> Self {
        Self {
            view_state: ViewState::default(),
            view_mode: ViewMode::Normal,
        }
    }
}

// ECS-managed zoom and view state
#[derive(Resource, Clone, Debug)]
pub struct ZoomResource {
    pub scale: f32,
    pub center_x: f32,
    pub center_y: f32,
    pub min_scale: f32,
    pub max_scale: f32,
    pub fit_to_view_scale: f32, // Reference scale for percentage calculations
}

impl Default for ZoomResource {
    fn default() -> Self {
        Self {
            scale: 1.0,
            center_x: 0.0,
            center_y: 0.0,
            min_scale: 0.001,
            max_scale: 1000.0,
            fit_to_view_scale: 1.0,
        }
    }
}

impl ZoomResource {
    pub fn new(scale: f32, center_x: f32, center_y: f32) -> Self {
        Self {
            scale: scale.clamp(0.001, 1000.0),
            center_x,
            center_y,
            min_scale: 0.001,
            max_scale: 1000.0,
            fit_to_view_scale: scale, // Use initial scale as fit-to-view reference
        }
    }
    
    pub fn set_scale(&mut self, scale: f32) {
        self.scale = scale.clamp(self.min_scale, self.max_scale);
    }
    
    pub fn zoom_in(&mut self, factor: f32) {
        self.set_scale(self.scale * factor);
    }
    
    pub fn zoom_out(&mut self, factor: f32) {
        self.set_scale(self.scale / factor);
    }
    
    pub fn set_center(&mut self, x: f32, y: f32) {
        self.center_x = x;
        self.center_y = y;
    }
    
    pub fn set_fit_to_view_scale(&mut self, scale: f32) {
        self.fit_to_view_scale = scale.clamp(self.min_scale, self.max_scale);
    }
    
    pub fn get_zoom_percentage(&self) -> f32 {
        // Calculate percentage relative to fit-to-view scale
        // fit_to_view_scale = 100%, so current_scale / fit_to_view_scale * 100
        (self.scale / self.fit_to_view_scale) * 100.0
    }
    
    pub fn reset_to_fit(&mut self, content_width: f32, content_height: f32, viewport_width: f32, viewport_height: f32) {
        // Calculate scale to fit content with some margin
        let scale_x = viewport_width / content_width;
        let scale_y = viewport_height / content_height;
        let fit_scale = scale_x.min(scale_y) * 0.95;
        
        self.set_scale(fit_scale);
        self.fit_to_view_scale = fit_scale; // Update reference scale
        
        // Center the view
        self.center_x = 0.0;
        self.center_y = 0.0;
    }
}

// Global rendering configuration
#[derive(Resource, Clone)]
pub struct RenderConfig {
    pub show_grid: bool,
    pub grid_spacing: f32,
    pub background_color: egui::Color32,
}

impl Default for RenderConfig {
    fn default() -> Self {
        Self {
            show_grid: true,
            grid_spacing: 1.0,
            background_color: egui::Color32::from_gray(20),
        }
    }
}

// Active layer resource (replaces LayerManager.active_layer)
#[derive(Resource)]
pub struct ActiveLayer(pub LayerType);

// Layer assignment tracking (replaces LayerManager.layer_assignments)
#[derive(Resource, Default)]
pub struct LayerAssignments(pub HashMap<String, LayerType>);

// Unassigned gerber files (replaces LayerManager.unassigned_gerbers)
#[derive(Resource, Default)]
pub struct UnassignedGerbers(pub Vec<UnassignedGerber>);

// Layer detection system (replaces LayerManager.layer_detector)
#[derive(Resource)]
pub struct LayerDetectorResource(pub LayerDetector);

impl Default for LayerDetectorResource {
    fn default() -> Self {
        Self(LayerDetector::new())
    }
}

// Coordinate update tracking (replaces LayerManager.coordinates_*)
#[derive(Resource)]
pub struct CoordinateUpdateTracker {
    pub dirty: bool,
    pub last_updated: std::time::Instant,
}

impl Default for CoordinateUpdateTracker {
    fn default() -> Self {
        Self {
            dirty: false,
            last_updated: std::time::Instant::now(),
        }
    }
}