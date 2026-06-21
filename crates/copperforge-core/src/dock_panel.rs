//! The plug-in seam: external crates register their own dock panels.
//!
//! This is the generic contract — core knows the *trait*, never the
//! concrete panel. A panel from any external crate implements
//! `DockPanel`, declaring exactly the shared state it depends on by the
//! `&mut SharedServices` it receives, and registers itself via
//! [`CopperForgeApp::register_panel`]. Core
//! dispatches to it through the dock without naming it — same idea as
//! the `egui_lens` / `egui_quill` / `egui_grafica` citizens.

/// A dockable panel contributed by an external crate.
pub trait DockPanel {
    /// Stable identifier for the panel (used as its citizen / tab key).
    fn id(&self) -> &str;

    /// Title shown on the dock tab.
    fn title(&self) -> &str;

    /// Render the panel. The only thing it gets from the host is the
    /// shared services — that `&mut SharedServices` *is* the panel's
    /// declared dependency surface. Anything it needs must be reachable
    /// from there (the reactive cells, the project state, etc.); it has
    /// no access to the host app struct.
    fn ui(&mut self, ui: &mut egui::Ui, services: &mut crate::services::SharedServices);

    /// Optionally contribute an overlay drawn over the board in the gerber
    /// view — the viewport analogue of contributing a tab. Returns generic
    /// shapes in world (mm) coords; core transforms and paints them without
    /// interpreting them. Default: none.
    fn viewport_overlay(
        &self,
        _services: &crate::services::SharedServices,
    ) -> Option<crate::services::ViewOverlay> {
        None
    }
}
