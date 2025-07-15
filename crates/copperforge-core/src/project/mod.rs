pub mod manager;
pub mod constants;
pub mod defaults;

// Re-export the main types for easy access
pub use manager::{ProjectManager, ProjectState};
pub use defaults::load_demo_gerber; // load_default_gerbers removed with LayerManager