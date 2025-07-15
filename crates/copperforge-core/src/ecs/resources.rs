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