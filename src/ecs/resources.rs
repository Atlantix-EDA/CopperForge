use bevy_ecs::prelude::*;
use gerber_viewer::ViewState;

// Simple view mode enum
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ViewMode {
    Normal,
    Quadrant,
}

// Wrapper for gerber_viewer's ViewState
#[derive(Resource)]
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
#[derive(Resource)]
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