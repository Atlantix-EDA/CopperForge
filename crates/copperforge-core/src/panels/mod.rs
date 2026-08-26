//! Citizen panels — each dock panel implements the Citizen trait.
//!
//! Panel rendering logic migrates here from ui/ in Phase 5.

mod settings;
mod drc;
mod view_settings;
mod projects;
mod bom;
mod gerber_view;
mod board3d_view;
mod gerber_view_3d;
mod terminal;
mod logger;

pub use settings::SettingsPanel;
pub use drc::DrcPanel;
pub use view_settings::ViewSettingsPanel;
pub use projects::ProjectsPanel;
pub use bom::BomPanel;
pub use gerber_view::GerberViewPanel;
pub use board3d_view::Board3dView;
pub use gerber_view_3d::GerberView3dPanel;
pub use terminal::TerminalPanel;
pub use logger::LoggerPanel;

pub(crate) use egui_citizen::citizen_panel;
