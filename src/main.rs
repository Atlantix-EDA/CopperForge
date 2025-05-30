use std::io::BufReader;
use std::{fs, path::PathBuf};

use eframe::emath::{Rect, Vec2};
use eframe::epaint::Color32;
use egui::ViewportBuilder;
use egui_dock::{DockArea, DockState, NodeIndex, Style, SurfaceIndex};
use serde::{Serialize, Deserialize};

/// egui_lens imports
use egui_lens::{ReactiveEventLogger, ReactiveEventLoggerState, LogColors};

/// Use of prelude for egui_mobius_reactive
use egui_mobius_reactive::Dynamic;  
use std::collections::HashMap;

use gerber_viewer::gerber_parser::parse;
use gerber_viewer::{
    draw_arrow, draw_outline, draw_crosshair, BoundingBox, GerberLayer, GerberRenderer, 
    ViewState, Mirroring, draw_marker, UiState
};
use gerber_viewer::position::Vector;


// Import platform modules
mod platform;
use platform::{banner, details};

// Import new modules
mod constants;
mod layers;
mod grid;
mod ui;
mod kicad;
mod kicad_api;

use constants::*;
use layers::{LayerType, LayerInfo};
use grid::GridSettings;

/// Define the tabs for the DockArea
#[derive(Clone, Serialize, Deserialize)]
enum TabKind {
    ViewSettings,
    DRC,
    GerberView,
    EventLog,
    KiCadSync,
}

pub struct TabParams<'a> {
    pub app: &'a mut DemoLensApp,

}

/// Tab container struct for DockArea
#[derive(Clone, Serialize, Deserialize)]
struct Tab {
    kind: TabKind,
    #[serde(skip)]
    surface: Option<SurfaceIndex>,
    #[serde(skip)]
    node: Option<NodeIndex>,
}

/// The main application struct
pub struct DemoLensApp {
    // Multi-layer support
    pub layers: HashMap<LayerType, LayerInfo>,
    pub active_layer: LayerType,
    
    // KiCad support
    pub pcb_data: Option<kicad::PcbFile>,
    pub symbol_data: Option<Vec<kicad::Symbol>>,
    pub active_pcb_layers: Vec<String>,
    pub file_type: ui::FileType,
    pub selected_component: Option<String>,
    
    // Legacy single layer support (for compatibility)
    pub gerber_layer: GerberLayer,
    pub view_state: ViewState,
    pub ui_state: UiState,
    pub needs_initial_view: bool,

    pub rotation_degrees: f32,
    
    // Logger state, colors, banner, details
    pub logger_state : Dynamic<ReactiveEventLoggerState>,
    pub log_colors   : Dynamic<LogColors>,
    pub banner       : banner::Banner,
    pub details      : details::Details,
    
    // Properties
    pub enable_unique_colors: bool,
    pub enable_polygon_numbering: bool,
    pub mirroring: Mirroring,
    pub center_offset: Vector,
    pub design_offset: Vector,
    pub showing_top: bool,  // true = top layers, false = bottom layers
    
    // DRC Properties
    pub current_drc_ruleset: Option<String>,
    
    // Grid Settings
    pub grid_settings: GridSettings,

    // Dock state
    dock_state: DockState<Tab>,
    config_path: PathBuf,
    
    // KiCad API connection
    kicad_monitor: kicad_api::KiCadMonitor,
    kicad_auto_refresh: bool,
    kicad_refresh_interval: f32,
    kicad_synced_layers: Vec<LayerType>,
}

impl Tab {
    fn new(kind: TabKind, surface: SurfaceIndex, node: NodeIndex) -> Self {
        Self {
            kind,
            surface: Some(surface),
            node: Some(node),
        }
    }

    fn title(&self) -> String {
        match self.kind {
            TabKind::ViewSettings => "View Settings".to_string(),
            TabKind::DRC => "DRC".to_string(),
            TabKind::GerberView => "Gerber View".to_string(),
            TabKind::EventLog => "Event Log".to_string(),
            TabKind::KiCadSync => "KiCad Sync".to_string(),
        }
    }


    fn content(&self, ui: &mut egui::Ui, params: &mut TabParams<'_>) {
        match self.kind {
            TabKind::ViewSettings => {
                // Use vertical layout like diskforge
                ui.vertical(|ui| {
                    let logger_state_clone = params.app.logger_state.clone();
                    let log_colors_clone = params.app.log_colors.clone();
                    
                    // Layer Controls Section
                    ui.heading("Layer Controls");
                    ui.separator();
                    
                    match params.app.file_type {
                        ui::FileType::Gerber => {
                            ui::show_layers_panel(ui, params.app, &logger_state_clone, &log_colors_clone);
                        }
                        ui::FileType::KicadPcb => {
                            ui::show_pcb_layer_panel(ui, params.app, &logger_state_clone, &log_colors_clone);
                        }
                        ui::FileType::KicadSymbol => {
                            ui.label("Symbol view - no layers");
                        }
                    }
                    
                    ui.add_space(20.0);
                    
                    // Orientation Section
                    ui.heading("Orientation");
                    ui.separator();
                    ui::show_orientation_panel(ui, params.app, &logger_state_clone, &log_colors_clone);
                    
                    ui.add_space(20.0);
                    
                    // Grid Settings Section
                    ui.heading("Grid Settings");
                    ui.separator();
                    ui::show_grid_panel(ui, params.app, &logger_state_clone, &log_colors_clone);
                });
            }
            TabKind::DRC => {
                let logger_state_clone = params.app.logger_state.clone();
                let log_colors_clone = params.app.log_colors.clone();
                ui::show_drc_panel(ui, params.app, &logger_state_clone, &log_colors_clone);
            }
            TabKind::GerberView => {
                self.render_gerber_view(ui, params.app);
            }
            TabKind::EventLog => {
                let logger = ReactiveEventLogger::with_colors(&params.app.logger_state, &params.app.log_colors);
                logger.show(ui);
            }
            TabKind::KiCadSync => {
                let logger_state_clone = params.app.logger_state.clone();
                let log_colors_clone = params.app.log_colors.clone();
                ui::kicad_panel::show_kicad_panel(ui, params.app, &logger_state_clone, &log_colors_clone);
            }
        }
    }

    fn render_gerber_view(&self, ui: &mut egui::Ui, app: &mut DemoLensApp) {
        // Fill all available space in the panel
        ui.ctx().request_repaint(); // Ensure continuous updates
        
        // Use allocate_response to ensure we fill the entire available area
        let available_size = ui.available_size();
        // Ensure minimum size to avoid zero-sized allocations
        let size = egui::Vec2::new(
            available_size.x.max(100.0),
            available_size.y.max(100.0)
        );
        let response = ui.allocate_response(size, egui::Sense::drag());
        let viewport = response.rect;

        // Fill the background with the panel color to ensure no black gaps
        let painter = ui.painter_at(viewport);
        painter.rect_filled(viewport, 0.0, ui.visuals().extreme_bg_color);

        if app.needs_initial_view {
            app.reset_view(viewport)
        }
        
        app.ui_state.update(ui, &viewport, &response, &mut app.view_state);

        let painter = ui.painter().with_clip_rect(viewport);
        
        // Draw grid if enabled (before other elements so it appears underneath)
        grid::draw_grid(&painter, &viewport, &app.view_state, &app.grid_settings);
        
        draw_crosshair(&painter, app.ui_state.origin_screen_pos, Color32::BLUE);
        draw_crosshair(&painter, app.ui_state.center_screen_pos, Color32::LIGHT_GRAY);

        // Render based on file type
        match app.file_type {
            ui::FileType::KicadPcb => {
                self.render_pcb_layers(&painter, app, &viewport);
            }
            _ => {
                // Render Gerber layers
                self.render_gerber_layers(&painter, app, &viewport);
            }
        }
    }
    
    fn get_pcb_layer_color(&self, layer_name: &str) -> Color32 {
        ui::pcb_layer_panel::get_layer_color(layer_name)
    }
    
    fn render_pcb_layers(&self, painter: &egui::Painter, app: &mut DemoLensApp, viewport: &Rect) {
        ui::pcb_renderer::PcbRenderer::render_pcb(painter, app, viewport);
    }
    
    fn render_gerber_layers(&self, painter: &egui::Painter, app: &mut DemoLensApp, viewport: &Rect) {
        // Render all visible layers based on showing_top
        for layer_type in LayerType::all() {
            if let Some(layer_info) = app.layers.get(&layer_type) {
                if layer_info.visible {
                    // Filter based on showing_top
                    let should_render = layer_type.should_render(app.showing_top);
                    
                    if should_render {
                        // Use the layer's specific gerber data if available, otherwise fall back to demo
                        let gerber_to_render = layer_info.gerber_layer.as_ref()
                            .unwrap_or(&app.gerber_layer);
                        
                        GerberRenderer::default().paint_layer(
                            &painter,
                            app.view_state,
                            gerber_to_render,
                            layer_type.color(),
                            false, // Don't use unique colors for multi-layer view
                            false, // Don't show polygon numbering
                            app.rotation_degrees.to_radians(),
                            app.mirroring,
                            app.center_offset.into(),
                            app.design_offset.into(),
                        );
                    }
                }
            }
        }

        // Get bounding box and outline vertices
        let bbox = app.gerber_layer.bounding_box();
        let origin = app.center_offset - app.design_offset;
        let bbox_vertices = bbox.vertices();  
        let outline_vertices = bbox.vertices();  
        
        // Transform vertices after getting them
        let bbox_vertices_screen = bbox_vertices.iter()
            .map(|v| app.view_state.gerber_to_screen_coords(*v + origin.to_position()))
            .collect::<Vec<_>>();
            
        let outline_vertices_screen = outline_vertices.iter()
            .map(|v| app.view_state.gerber_to_screen_coords(*v + origin.to_position()))
            .collect::<Vec<_>>();

        draw_outline(&painter, bbox_vertices_screen, Color32::RED);
        draw_outline(&painter, outline_vertices_screen, Color32::GREEN);

        let screen_radius = MARKER_RADIUS * app.view_state.scale;

        let design_offset_screen_position = app.view_state.gerber_to_screen_coords(app.design_offset.to_position());
        draw_arrow(&painter, design_offset_screen_position, app.ui_state.origin_screen_pos, Color32::ORANGE);
        draw_marker(&painter, design_offset_screen_position, Color32::ORANGE, Color32::YELLOW, screen_radius);

        let design_origin_screen_position = app.view_state.gerber_to_screen_coords((app.center_offset - app.design_offset).to_position());
        draw_marker(&painter, design_origin_screen_position, Color32::PURPLE, Color32::MAGENTA, screen_radius);
        
        // Draw board dimensions in mils at the bottom
        if let Some(layer_info) = app.layers.get(&LayerType::MechanicalOutline) {
            if let Some(ref outline_layer) = layer_info.gerber_layer {
                let bbox = outline_layer.bounding_box();
                let width_mm = bbox.width();
                let height_mm = bbox.height();
                let width_mils = width_mm / 0.0254;
                let height_mils = height_mm / 0.0254;
                
                let dimension_text = format!("{:.0} x {:.0} mils", width_mils, height_mils);
                let text_pos = viewport.max - Vec2::new(10.0, 30.0);
                painter.text(
                    text_pos,
                    egui::Align2::RIGHT_BOTTOM,
                    dimension_text,
                    egui::FontId::default(),
                    Color32::from_rgb(200, 200, 200),
                );
            }
        }
    }
}

struct TabViewer<'a> {
    app: &'a mut DemoLensApp,
}

impl<'a> egui_dock::TabViewer for TabViewer<'a> {
    type Tab = Tab;

    fn title(&mut self, tab: &mut Self::Tab) -> egui::WidgetText {
        tab.title().into()
    }

    fn ui(&mut self, ui: &mut egui::Ui, tab: &mut Self::Tab) {
        let mut params = TabParams {
            app: self.app,
            // ...other fields as needed
        };
        tab.content(ui, &mut params);
    }
}

impl Drop for DemoLensApp {
    fn drop(&mut self) {
        // Save dock state when application closes
        self.save_dock_state();
    }
}

impl DemoLensApp {
    /// **Create a new instance of the DemoLensApp**
    ///
    /// This function initializes the application state, including loading the Gerber layer,
    /// setting up the logger, and configuring the UI properties. It also sets up the initial view
    /// and adds platform details to the app. The function returns a new instance of the DemoLensApp.
    ///
    pub fn new() -> Self {
        // Load the demo gerber for legacy compatibility
        let demo_str = include_str!("../assets/demo.gbr").as_bytes();
        let reader = BufReader::new(demo_str);
        let doc = parse(reader).unwrap();
        let commands = doc.into_commands();
        let gerber_layer = GerberLayer::new(commands);
        
        // Initialize layers HashMap
        let mut layers = HashMap::new();
        
        // Map layer types to their corresponding gerber files
        let layer_files = [
            (LayerType::TopCopper, "cmod_s7-F_Cu.gbr"),
            (LayerType::BottomCopper, "cmod_s7-B_Cu.gbr"),
            (LayerType::TopSilk, "cmod_s7-F_SilkS.gbr"),
            (LayerType::BottomSilk, "cmod_s7-B_SilkS.gbr"),
            (LayerType::TopSoldermask, "cmod_s7-F_Mask.gbr"),
            (LayerType::BottomSoldermask, "cmod_s7-B_Mask.gbr"),
            (LayerType::MechanicalOutline, "cmod_s7-Edge_Cuts.gbr"),
        ];
        
        // Load each layer's gerber file
        for (layer_type, filename) in layer_files {
            let gerber_data = match filename {
                "cmod_s7-F_Cu.gbr" => include_str!("../assets/cmod_s7-F_Cu.gbr"),
                "cmod_s7-B_Cu.gbr" => include_str!("../assets/cmod_s7-B_Cu.gbr"),
                "cmod_s7-F_SilkS.gbr" => include_str!("../assets/cmod_s7-F_SilkS.gbr"),
                "cmod_s7-B_SilkS.gbr" => include_str!("../assets/cmod_s7-B_SilkS.gbr"),
                "cmod_s7-F_Mask.gbr" => include_str!("../assets/cmod_s7-F_Mask.gbr"),
                "cmod_s7-B_Mask.gbr" => include_str!("../assets/cmod_s7-B_Mask.gbr"),
                "cmod_s7-Edge_Cuts.gbr" => include_str!("../assets/cmod_s7-Edge_Cuts.gbr"),
                _ => include_str!("../assets/demo.gbr"), // Fallback
            };
            
            let reader = BufReader::new(gerber_data.as_bytes());
            let layer_gerber = match parse(reader) {
                Ok(doc) => {
                    let commands = doc.into_commands();
                    Some(GerberLayer::new(commands))
                }
                Err(e) => {
                    eprintln!("Failed to parse {}: {:?}", filename, e);
                    None
                }
            };
            
            let layer_info = LayerInfo::new(
                layer_type,
                layer_gerber,
                matches!(layer_type, LayerType::TopCopper | LayerType::MechanicalOutline),
            );
            layers.insert(layer_type, layer_info);
        }
        
        // Create logger state, colors, banner, and details
        let logger_state = Dynamic::new(ReactiveEventLoggerState::new());
        let log_colors = Dynamic::new(LogColors::default());
        let mut banner = banner::Banner::new(); 
        banner.format(); 
        let mut details = details::Details::new(); 
        details.get_os();
        
        // Load the PCB file from assets
        let pcb_str = include_str!("../assets/fpga.kicad_pcb");
        let pcb_data = match kicad::parse_layers_only(pcb_str) {
            Ok(pcb) => {
                eprintln!("Successfully loaded PCB file with {} layers", pcb.layers.len());
                Some(pcb)
            }
            Err(e) => {
                eprintln!("Failed to parse PCB file: {:?}", e);
                None
            }
        };
        
        // Set up active PCB layers if PCB was loaded
        let mut active_pcb_layers = Vec::new();
        if let Some(ref pcb) = pcb_data {
            // Enable common layers by default
            let default_layers = ["F.Cu", "B.Cu", "F.SilkS", "B.SilkS", "Edge.Cuts"];
            for (_, layer) in &pcb.layers {
                if default_layers.contains(&layer.name.as_str()) {
                    active_pcb_layers.push(layer.name.clone());
                }
            }
        }
        

        // Initialize dock state with gerber view as the main content
        let view_settings_tab = Tab::new(TabKind::ViewSettings, SurfaceIndex::main(), NodeIndex(0));
        let drc_tab = Tab::new(TabKind::DRC, SurfaceIndex::main(), NodeIndex(1));
        let kicad_tab = Tab::new(TabKind::KiCadSync, SurfaceIndex::main(), NodeIndex(2));
        let gerber_tab = Tab::new(TabKind::GerberView, SurfaceIndex::main(), NodeIndex(3));
        let log_tab = Tab::new(TabKind::EventLog, SurfaceIndex::main(), NodeIndex(4));
        
        // Create dock state with gerber view as the root
        let mut dock_state = DockState::new(vec![gerber_tab]);
        let surface = dock_state.main_surface_mut();
        
        // Split left for control panels
        let [left, _right] = surface.split_left(
            NodeIndex::root(),
            0.3, // Left panel takes 30% of width
            vec![view_settings_tab, drc_tab, kicad_tab],
        );
        
        // Add event log to bottom of left panel
        surface.split_below(
            left,
            0.7, // Top takes 70% of height
            vec![log_tab],
        );

        // Determine file type before moving pcb_data
        let file_type = if pcb_data.is_some() { ui::FileType::KicadPcb } else { ui::FileType::Gerber };
        
        let app = Self {
            layers,
            active_layer: LayerType::TopCopper,
            pcb_data,
            symbol_data: None,
            active_pcb_layers,
            file_type,
            selected_component: None,
            gerber_layer,
            view_state: ViewState::default(),
            ui_state: UiState::default(),
            needs_initial_view: true,
            rotation_degrees: 0.0,
            logger_state,
            log_colors,
            banner,
            details,
            enable_unique_colors: ENABLE_UNIQUE_SHAPE_COLORS,
            enable_polygon_numbering: ENABLE_POLYGON_NUMBERING,
            mirroring: MIRRORING.into(),
            center_offset: CENTER_OFFSET,
            design_offset: DESIGN_OFFSET,
            showing_top: true,
            current_drc_ruleset: None,
            grid_settings: GridSettings::default(),
            dock_state,
            config_path: dirs::config_dir()
                .map(|d| d.join("kiforge"))
                .unwrap_or_default(),
            kicad_monitor: kicad_api::KiCadMonitor::new(),
            kicad_auto_refresh: true,
            kicad_refresh_interval: 1.0,
            kicad_synced_layers: Vec::new(),
        };
        
        // Add platform details
        app.add_banner_platform_details();
        
        app
    }

    /// **Add platform details to the app**
    /// 
    /// These functions are customizable via the `platform` module.
    /// The `add_banner_platform_details` function is responsible for logging the banner message
    /// and system details. It creates a logger using the `ReactiveEventLogger` and logs the banner
    /// and operating system details.
     fn add_banner_platform_details(&self) {
        // Create a logger using references to our logger state
        let logger = ReactiveEventLogger::with_colors(&self.logger_state, &self.log_colors);
        
        // Log banner message (welcome message)
        logger.log_info(&self.banner.message);
        
        // Log system details
        let details_text = self.details.clone().format_os();
        logger.log_info(&details_text);
     }

    fn reset_view(&mut self, viewport: Rect) {
        // Handle PCB view
        if self.file_type == ui::FileType::KicadPcb {
            if let Some(pcb) = &self.pcb_data {
                let pcb_bounds = ui::pcb_renderer::PcbRenderer::calculate_pcb_bounds(pcb);
                
                // Set a reasonable default scale for PCB viewing
                let scale_x = viewport.width() / (pcb_bounds.width() * 1.2);
                let scale_y = viewport.height() / (pcb_bounds.height() * 1.2);
                let scale = scale_x.min(scale_y).max(0.1);
                
                self.view_state.scale = scale;
                self.view_state.translation = Vec2::new(
                    viewport.center().x,
                    viewport.center().y
                );
                self.needs_initial_view = false;
                return;
            }
        }
        
        // Find bounding box from all loaded layers (for Gerber)
        let mut combined_bbox: Option<BoundingBox> = None;
        
        for layer_info in self.layers.values() {
            if let Some(ref layer_gerber) = layer_info.gerber_layer {
                let layer_bbox = layer_gerber.bounding_box();
                combined_bbox = Some(match combined_bbox {
                    None => layer_bbox.clone(),
                    Some(existing) => BoundingBox {
                        min: gerber_viewer::position::Position::new(
                            existing.min.x.min(layer_bbox.min.x),
                            existing.min.y.min(layer_bbox.min.y),
                        ),
                        max: gerber_viewer::position::Position::new(
                            existing.max.x.max(layer_bbox.max.x),
                            existing.max.y.max(layer_bbox.max.y),
                        ),
                    },
                });
            }
        }
        
        // Fall back to demo gerber if no layers loaded
        let bbox = combined_bbox.unwrap_or_else(|| self.gerber_layer.bounding_box().clone());
        let content_width = bbox.width();
        let content_height = bbox.height();

        // Calculate scale to fit the content (100% zoom)
        let scale = f32::min(
            viewport.width() / (content_width as f32),
            viewport.height() / (content_height as f32),
        );
        // adjust slightly to add a margin
        let scale = scale * 0.95;

        let center = bbox.center();

        // Offset from viewport center to place content in the center
        self.view_state.translation = Vec2::new(
            viewport.center().x - (center.x as f32 * scale),
            viewport.center().y + (center.y as f32 * scale), // Note the + here since we flip Y
        );

        self.view_state.scale = scale;
        self.needs_initial_view = false;
    }
}

impl DemoLensApp {
    fn save_dock_state(&self) {
        if let Some(config_dir) = dirs::config_dir() {
            let kiforge_dir = config_dir.join("kiforge");
            fs::create_dir_all(&kiforge_dir).ok();
            let config_path = kiforge_dir.join("dock_state.json");
            if let Ok(json) = serde_json::to_string_pretty(&self.dock_state) {
                fs::write(config_path, json).ok();
            }
        }
    }
}

/// Implement the eframe::App trait for DemoLensApp
///
/// This implementation contains the main event loop for the application, including
/// handling user input, updating the UI, and rendering the Gerber layer. It also contains
/// the logic for handling the logger and displaying system information.
/// The `update` method is called every frame and is responsible for updating the UI
/// and rendering the Gerber layer. It also handles user input and updates the logger
/// state. The `update` method is where most of the application logic resides.
/// 
impl eframe::App for DemoLensApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Handle system info button clicked
        let show_system_info = ctx.memory(|mem| {
            mem.data.get_temp::<bool>(egui::Id::new("show_system_info")).unwrap_or(false)
        });
        
        if show_system_info {
            // Clear the flag
            ctx.memory_mut(|mem| {
                mem.data.remove::<bool>(egui::Id::new("show_system_info"));
            });
            
            // Create a temporary logger for system info output
            let logger = ReactiveEventLogger::with_colors(&self.logger_state, &self.log_colors);
            
            // Display system details
            let details_text = self.details.format_os();
            logger.log_info(&details_text);
            
            // Display banner
            logger.log_info(&self.banner.message);
        }
        
        // Top menu bar
        egui::TopBottomPanel::top("menu_bar").show(ctx, |ui| {
            egui::menu::bar(ui, |ui| {
                ui.menu_button("File", |ui| {
                    let logger_state_clone = self.logger_state.clone();
                    let log_colors_clone = self.log_colors.clone();
                    ui::show_file_menu(ui, self, &logger_state_clone, &log_colors_clone);
                    
                    ui.separator();
                    if ui.button("Exit").clicked() {
                        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                    }
                });
                
                ui.menu_button("View", |ui| {
                    if ui.button("Fit to View").clicked() {
                        let viewport = ctx.screen_rect();
                        self.reset_view(viewport);
                    }
                    ui.separator();
                    ui.checkbox(&mut self.showing_top, "Show Top Layers");
                });
            });
        });
        
        // Clone the dock state
        let mut dock_state = self.dock_state.clone();
        
        // Create the dock layout and tab viewer
        let mut tab_viewer = TabViewer { app: self };
        
        // Create custom style to match panel colors
        let mut style = Style::from_egui(ctx.style().as_ref());
        style.dock_area_padding = None;
        style.tab_bar.fill_tab_bar = true;
        
        // Show the dock area directly on the context
        DockArea::new(&mut dock_state)
            .style(style)
            .show_add_buttons(false)
            .show_close_buttons(true)
            .show(ctx, &mut tab_viewer);
            
        // Save the updated dock state back to the app
        self.dock_state = dock_state;
        
        // Save dock state to disk periodically
        if ctx.input(|i| i.time) % 30.0 < 0.1 {
            self.save_dock_state();
        }
    }
}

/// The main function is the entry point of the application.
/// 
/// It initializes the logger, sets up the native window options,
/// and runs the application using the `eframe` framework.
fn main() -> eframe::Result<()> {
    env_logger::init(); // Log to stderr (optional).
    eframe::run_native(
        platform::parameters::gui::APPLICATION_NAME, // Use the application name from the platform module
        eframe::NativeOptions {
            viewport: ViewportBuilder::default().with_inner_size([1280.0, 768.0]),
            ..Default::default()
        },
        Box::new(|_cc| Ok(Box::new(DemoLensApp::new()))),
    )
}