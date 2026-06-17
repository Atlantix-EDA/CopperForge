//! Tab definitions and the `TabViewer` bridge into `egui_dock`.
//!
//! Mirrors zicad/src/tabs.rs in structure. Each tab routes to a
//! `render_*_tab` method on `WebApp` so the existing UI code lives
//! where the state does, and the dock layer stays a thin dispatch.

use crate::app::WebApp;
use eframe::egui;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum TabKind {
    Layers,
    Canvas,
    Board3d,
    Board,
    Logger,
    Settings,
}

#[derive(Clone, Debug)]
pub struct Tab {
    pub kind: TabKind,
}

impl Tab {
    pub fn new(kind: TabKind) -> Self {
        Self { kind }
    }

    pub fn title(&self) -> &'static str {
        match self.kind {
            TabKind::Layers => "Layers",
            TabKind::Canvas => "Canvas",
            TabKind::Board3d => "3D Board",
            TabKind::Board => "Board",
            TabKind::Logger => "Logger",
            TabKind::Settings => "Settings",
        }
    }
}

/// The `TabViewer` impl egui_dock calls back into each frame. Holds
/// `&mut WebApp` so tabs can mutate scene / view state / etc.; each
/// tab dispatches to a `render_*_tab` method on `WebApp`.
pub struct TabViewer<'a> {
    pub app: &'a mut WebApp,
}

impl egui_dock::TabViewer for TabViewer<'_> {
    type Tab = Tab;

    fn title(&mut self, tab: &mut Tab) -> egui::WidgetText {
        tab.title().into()
    }

    fn ui(&mut self, ui: &mut egui::Ui, tab: &mut Tab) {
        match tab.kind {
            TabKind::Layers => self.app.render_layer_tab(ui),
            TabKind::Canvas => self.app.render_canvas_tab(ui),
            TabKind::Board3d => self.app.render_board3d_tab(ui),
            TabKind::Board => self.app.render_board_tab(ui),
            TabKind::Logger => self.app.render_logger_tab(ui),
            TabKind::Settings => self.app.render_settings_tab(ui),
        }
    }
}
