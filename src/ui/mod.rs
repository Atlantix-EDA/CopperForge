pub mod layer_controls;
pub mod orientation_panel;
pub mod drc_panel;
pub mod grid_settings;
pub mod view_settings_panel;
pub mod project_panel;
pub mod settings_panel;
pub mod about_panel;
pub mod tabs;
pub mod selection;
pub mod bom_panel_v2;
pub mod project_manager_panel;

// Re-export the show functions for each panel
pub use layer_controls::show_layers_panel;
pub use drc_panel::show_drc_panel;
pub use grid_settings::show_grid_panel;
pub use project_panel::show_project_panel;
pub use settings_panel::show_settings_panel;
pub use about_panel::AboutPanel;
pub use bom_panel_v2::{show_bom_panel, BomPanelState};

// Re-export tab-related types
pub use tabs::{Tab, TabKind, TabViewer};

// Re-export selection functions
pub use selection::{initialize_and_show_banner, show_system_info};
