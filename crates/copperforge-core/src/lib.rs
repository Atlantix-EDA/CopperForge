// KiForge Core Library
// Re-export all modules for external use

pub mod bom;
pub mod display;
pub mod drc_operations;
pub mod event_logger;
pub mod export;
pub mod layer_store;
pub mod messages;
pub mod panels;
pub mod services;
// layer_operations module removed - all functionality moved to layer_store
pub mod navigation;
pub mod platform;
pub mod project;
pub mod project_manager;
pub mod release;
pub mod theme;
pub mod ui;
pub mod vendor;
pub mod app;

// Re-export CopperForgeApp from app module
pub use app::CopperForgeApp;

