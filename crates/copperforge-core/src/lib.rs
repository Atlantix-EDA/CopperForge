// KiForge Core Library
// Re-export all modules for external use

pub mod display;
pub mod drc_operations;
pub mod ecs;
pub mod export;
pub mod layer_operations;
pub mod navigation;
pub mod platform;
pub mod project;
pub mod project_manager;
pub mod ui;
pub mod app;

// Re-export DemoLensApp from app module
pub use app::DemoLensApp;

