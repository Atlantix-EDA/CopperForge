// KiForge Core Library
// Re-export all modules for external use

pub mod bom;
pub mod display;
pub mod drc_operations;
pub mod event_logger;
pub mod fsm;
pub mod export;
pub mod gerber_geom;
pub mod layer_store;
pub mod messages;
pub mod panels;
pub mod render3d;
pub mod services;
/// STEP → shaded triangle soup for the 3D board view. Native-only: the
/// solidcraft-kernel dep is target-gated in Cargo.toml (wasm32 would build,
/// but bloats the web bundle for a prototype feature).
#[cfg(not(target_arch = "wasm32"))]
pub mod step_model;
pub mod cuforge_client;
pub mod cuforge_api;
// layer_operations module removed - all functionality moved to layer_store
pub mod navigation;
pub mod perspectives;
pub mod platform;
pub mod project;
pub mod project_manager;
pub mod release;
pub mod theme;
pub mod ui;
pub mod vendor;
pub mod app;
pub mod dock_panel;

// Re-export CopperForgeApp from app module
pub use app::CopperForgeApp;
pub use dock_panel::DockPanel;

