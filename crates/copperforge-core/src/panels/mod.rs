//! Citizen panels — each dock panel implements the Citizen trait.
//!
//! Panel rendering logic migrates here from ui/ in Phase 5.

mod settings;
mod drc;
mod view_settings;
mod projects;
mod bom;
mod gerber_view;
mod gerber_view_3d;
mod terminal;
mod logger;

pub use settings::SettingsPanel;
pub use drc::DrcPanel;
pub use view_settings::ViewSettingsPanel;
pub use projects::ProjectsPanel;
pub use bom::BomPanel;
pub use gerber_view::GerberViewPanel;
pub use gerber_view_3d::GerberView3dPanel;
pub use terminal::TerminalPanel;
pub use logger::LoggerPanel;

/// Helper to create a citizen panel with standard boilerplate.
macro_rules! citizen_panel {
    ($name:ident, $id:expr $(, $field:ident : $ty:ty = $default:expr)*) => {
        pub struct $name {
            citizen_id: CitizenId,
            citizen_state: CitizenState,
            $( pub $field: $ty, )*
        }

        impl $name {
            pub fn new(citizen_state: CitizenState) -> Self {
                Self {
                    citizen_id: CitizenId::new($id),
                    citizen_state,
                    $( $field: $default, )*
                }
            }
        }

        impl Citizen for $name {
            fn id(&self) -> &CitizenId { &self.citizen_id }
            fn state(&self) -> &CitizenState { &self.citizen_state }
            fn state_mut(&mut self) -> &mut CitizenState { &mut self.citizen_state }
        }
    };
}

pub(crate) use citizen_panel;
