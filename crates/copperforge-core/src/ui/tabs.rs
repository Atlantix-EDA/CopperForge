use crate::CopperForgeApp;
use egui_citizen::message::CitizenId;

use egui_dock::{SurfaceIndex, NodeIndex};
use serde::{Serialize, Deserialize};



/// Define the tabs for the DockArea
#[derive(Clone, Serialize, Deserialize)]
pub enum TabKind {
    ViewSettings,
    DRC,
    GerberView,
    GerberView3d,
    Logger,
    /// Forge commands (via clap) + bash passthrough — merged from the
    /// former `Shell` panel. Any unknown first-token falls through to
    /// `bash -c`, so the tab behaves like a normal terminal too.
    Terminal,
    Projects,  // Project database + tree + Import modal (replaced old Project tab)
    Settings,
    BOM,
    /// A panel contributed by an external crate, indexed into
    /// `CopperForgeApp::plugin_panels`. Core dispatches to it through the
    /// `DockPanel` trait without knowing what it is.
    Plugin(usize),
}

impl TabKind {
    /// Map each tab to its citizen ID.
    pub fn citizen_id(&self) -> CitizenId {
        match self {
            TabKind::ViewSettings => CitizenId::new("view_settings"),
            TabKind::DRC => CitizenId::new("drc"),
            TabKind::GerberView => CitizenId::new("gerber_view"),
            TabKind::GerberView3d => CitizenId::new("gerber_view_3d"),
            TabKind::Logger => CitizenId::new("logger"),
            TabKind::Terminal => CitizenId::new("terminal"),
            TabKind::Projects => CitizenId::new("projects"),
            TabKind::Settings => CitizenId::new("settings"),
            TabKind::BOM => CitizenId::new("bom"),
            TabKind::Plugin(_) => CitizenId::new("plugin"),
        }
    }
}

pub struct TabParams<'a> {
    pub app: &'a mut CopperForgeApp,
}

/// Tab container struct for DockArea
#[derive(Clone, Serialize, Deserialize)]
pub struct Tab {
    pub kind: TabKind,
    #[serde(skip)]
    #[allow(dead_code)]
    pub surface: Option<SurfaceIndex>,
    #[serde(skip)]
    #[allow(dead_code)]
    pub node: Option<NodeIndex>,
}

impl Tab {
    /// Helper to get units from app
    pub(crate) fn get_units(app: &CopperForgeApp) -> &crate::layer_store::UnitsState {
        &app.services.layer_store.units
    }
    
    pub fn new(kind: TabKind, surface: SurfaceIndex, node: NodeIndex) -> Self {
        Self {
            kind,
            surface: Some(surface),
            node: Some(node),
        }
    }

    pub fn title(&self) -> String {
        match self.kind {
            TabKind::ViewSettings => "View Settings".to_string(),
            TabKind::DRC => "DRC".to_string(),
            TabKind::GerberView => "Gerber View".to_string(),
            TabKind::GerberView3d => "Gerber View 3D".to_string(),
            TabKind::Logger => "Logger".to_string(),
            TabKind::Terminal => "Terminal".to_string(),
            TabKind::Projects => "Projects".to_string(),
            TabKind::Settings => "Settings".to_string(),
            TabKind::BOM => "BOM".to_string(),
            // Real title comes from the registered panel via
            // TabViewer::title (which has app access); fallback only.
            TabKind::Plugin(_) => "Plugin".to_string(),
        }
    }

    /// Dispatch rendering through citizen panels.
    ///
    /// Each TabKind maps to a citizen panel's show() method.
    /// GerberView is special — it renders through the legacy Tab path
    /// because it has 1300 lines of viewport interaction logic.
    pub fn content(&self, ui: &mut egui::Ui, params: &mut TabParams<'_>) {
        // Plug-in panels: dispatch through the registry by index. The
        // panel gets only `&mut SharedServices` (its declared dependency).
        if let TabKind::Plugin(idx) = &self.kind {
            let idx = *idx;
            if let Some(panel) = params.app.plugin_panels.get_mut(idx) {
                panel.ui(ui, &mut params.app.services);
            }
            return;
        }

        use crate::panels::*;

        match self.kind {
            TabKind::ViewSettings => {
                ui.vertical(|ui| {
                    ui.heading("Layer Controls");
                    ui.separator();
                    ViewSettingsPanel::new(egui_citizen::CitizenState::default())
                        .show(ui, params.app);
                });
            }
            TabKind::DRC => {
                DrcPanel::new(egui_citizen::CitizenState::default())
                    .show(ui, params.app);
            }
            TabKind::GerberView => {
                crate::panels::GerberViewPanel::new(egui_citizen::CitizenState::default()).render(ui, params.app);
            }
            TabKind::GerberView3d => {
                // Rebuild meshes when the board geometry changed via any reload
                // path (app ribbon or the pro panel through services).
                let geom_gen = params.app.services.board_geometry_gen;
                if params.app.gerber_view_3d_panel.last_geometry_gen != geom_gen {
                    params.app.gerber_view_3d_panel.mark_dirty();
                    params.app.gerber_view_3d_panel.last_geometry_gen = geom_gen;
                }
                let gl = params.app.gl_context.clone();
                let outline = params.app.services.geometry.board_outline.as_ref();
                let top_copper = params.app.services.geometry.top_copper.as_ref();
                let bottom_copper = params.app.services.geometry.bottom_copper.as_ref();
                let top_mask = params.app.services.geometry.top_mask.as_ref();
                let bottom_mask = params.app.services.geometry.bottom_mask.as_ref();
                let top_silk = params.app.services.geometry.top_silk.as_ref();
                let bottom_silk = params.app.services.geometry.bottom_silk.as_ref();
                let inner_copper = params.app.services.geometry.inner_copper.as_slice();
                let drill = params.app.services.geometry.drill.as_ref();
                let units_mils = params.app.services.global_units_mils;
                params.app.gerber_view_3d_panel.show(
                    ui,
                    gl.as_ref(),
                    outline,
                    top_copper,
                    bottom_copper,
                    top_mask,
                    bottom_mask,
                    top_silk,
                    bottom_silk,
                    inner_copper,
                    drill,
                    units_mils,
                );
            }
            TabKind::Logger => {
                params.app.logger_panel.show(ui, &mut params.app.services);
            }
            TabKind::Terminal => {
                params.app.terminal_panel.show(ui, &mut params.app.services);
            }
            TabKind::Projects => {
                params.app.projects_panel.show(ui, &mut params.app.services);
            }
            TabKind::Settings => {
                SettingsPanel::new(egui_citizen::CitizenState::default())
                    .show(ui, params.app);
            }
            TabKind::BOM => {
                params.app.bom_panel.show(ui, &mut params.app.services);
            }
            TabKind::Plugin(_) => unreachable!("handled before the match"),
        }
    }
}

pub struct TabViewer<'a> {
    pub app: &'a mut CopperForgeApp,
    pub dispatcher: &'a mut egui_citizen::Dispatcher,
}

impl<'a> egui_dock::TabViewer for TabViewer<'a> {
    type Tab = Tab;

    fn title(&mut self, tab: &mut Self::Tab) -> egui::WidgetText {
        if let TabKind::Plugin(idx) = &tab.kind {
            if let Some(panel) = self.app.plugin_panels.get(*idx) {
                return panel.title().to_string().into();
            }
        }
        tab.title().into()
    }

    fn on_tab_button(&mut self, tab: &mut Self::Tab, response: &egui::Response) {
        if response.clicked() {
            let id = tab.kind.citizen_id();
            self.dispatcher.activate(&id);
        }
    }

    fn ui(&mut self, ui: &mut egui::Ui, tab: &mut Self::Tab) {
        let mut params = TabParams {
            app: self.app,
        };
        tab.content(ui, &mut params);
    }
}
