use std::io::BufReader;

use eframe::emath::{Rect, Vec2};
use eframe::epaint::Color32;
use egui::ViewportBuilder;

/// egui_lens imports
use egui_lens::{ReactiveEventLogger, ReactiveEventLoggerState, LogColors};

/// Use of prelude for egui_mobius_reactive
use egui_mobius_reactive::*; 

use gerber_viewer::gerber_parser::parse;
use gerber_viewer::{
    draw_arrow, draw_outline, draw_crosshair, BoundingBox, GerberLayer, GerberRenderer, 
    Transform2D, ViewState, Mirroring, draw_marker, UiState
};
use gerber_viewer::position::Vector;
use std::collections::HashMap;

// Import platform modules
mod platform;
use platform::{banner, details};

// Import new modules
mod constants;
mod layers;
mod grid;
mod ui;

use constants::*;
use layers::{LayerType, LayerInfo};
use grid::GridSettings;



/// The main application struct
/// 
/// This struct contains the state of the application, including the Gerber layer, view state, UI state,
/// and other properties. It also contains the logger state and the banner and details instances. The 
/// Logger state is used to log events and changes in the application, while the banner and details instances
/// are used to display information about the application and the system it is running on. Note that the 
/// logger_state is "reactive" and is used to log events in the application. The log_colors is also "reactive" and is used to
/// manage the colors used in the logger. 
pub struct DemoLensApp {
    // Multi-layer support
    pub layers: HashMap<LayerType, LayerInfo>,
    pub active_layer: LayerType,
    
    // Legacy single layer support (for compatibility)
    pub gerber_layer: GerberLayer,
    pub view_state: ViewState,
    pub ui_state: UiState,
    pub needs_initial_view: bool,

    pub rotation_degrees: f32,
    
    // Logger state
    pub logger_state: Dynamic<ReactiveEventLoggerState>,
    pub log_colors: Dynamic<LogColors>,
    pub banner: banner::Banner,
    pub details: details::Details,
    
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
}

/// Implement the DemoLensApp struct
///
/// This implementation contains methods for creating a new instance of the app,
/// configuring custom log colors, and watching for changes in the log colors.
/// It also contains methods for resetting the view and adding platform details to the app.
/// 
impl DemoLensApp {
    
    /// **Configure custom colors** 
    /// 
    /// This function will get the current colors from the `Dynamic<LogColors>` instance, 
    /// check if the custom colors for the specified log types are already set,
    /// and if not, set them to the default values.
    ///
    fn configure_custom_log_colors_if_missing(colors: &mut Dynamic<LogColors>) {

        let mut colors_value = colors.get();
        
        if !colors_value.custom_colors.contains_key(LOG_TYPE_ROTATION) {
            colors_value.set_custom_color(LOG_TYPE_ROTATION, egui::Color32::from_rgb(230, 126, 34));
        }
        if !colors_value.custom_colors.contains_key(LOG_TYPE_CENTER_OFFSET) {
            colors_value.set_custom_color(LOG_TYPE_CENTER_OFFSET, egui::Color32::from_rgb(142, 68, 173));
        }
        if !colors_value.custom_colors.contains_key(LOG_TYPE_DESIGN_OFFSET) {
            colors_value.set_custom_color(LOG_TYPE_DESIGN_OFFSET, egui::Color32::from_rgb(39, 174, 96));
        }
        if !colors_value.custom_colors.contains_key(LOG_TYPE_MIRROR) {
            colors_value.set_custom_color(LOG_TYPE_MIRROR, egui::Color32::from_rgb(192, 57, 43));
        }
        if !colors_value.custom_colors.contains_key(LOG_TYPE_DRC) {
            colors_value.set_custom_color(LOG_TYPE_DRC, egui::Color32::from_rgb(155, 89, 182));
        }
        if !colors_value.custom_colors.contains_key(LOG_TYPE_GRID) {
            colors_value.set_custom_color(LOG_TYPE_GRID, egui::Color32::from_rgb(52, 152, 219));
        }
        
        colors.set(colors_value);
    }
    
    /// **Color change watcher** 
    /// 
    /// This function sets up a watcher for changes to the log colors, and when a change is detected,
    /// it saves the current colors to a JSON file in the config directory. The egui_mobius_reactive 
    /// on_change method is used to trigger the save operation whenever the colors change. One does not
    /// need to call the signal registery for the `Derived` type to do this, as the `on_change` method 
    /// is a built in to the `Dynamic` type.
    ///
    fn watch_for_color_changes(&self) {
        let log_colors_clone = self.log_colors.clone();
        
        self.log_colors.on_change(move || {
            let colors = log_colors_clone.get();
            
            let config_dir = dirs::config_dir()
                .unwrap_or_else(|| std::path::PathBuf::from("."))
                .join("gerber_viewer");
            
            if let Err(e) = std::fs::create_dir_all(&config_dir) {
                eprintln!("Failed to create config directory: {}", e);
                return;
            }
            
            let config_path = config_dir.join("log_colors.json");
            
            match serde_json::to_string_pretty(&colors) {
                Ok(json) => {
                    // Write JSON to file
                    if let Err(e) = std::fs::write(&config_path, json) {
                        eprintln!("Failed to write colors to {}: {}", config_path.display(), e);
                    } else {
                        println!("Successfully saved colors to {}", config_path.display());
                    }
                },
                Err(e) => eprintln!("Failed to serialize colors: {}", e),
            }
        });
    }
    
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
        
        // Create logger state
        let logger_state = Dynamic::new(ReactiveEventLoggerState::new());
        
        // Custom load logic for gerber_viewer
        let mut log_colors = Dynamic::new({
            let config_dir = dirs::config_dir()
                .unwrap_or_else(|| std::path::PathBuf::from("."))
                .join("gerber_viewer");
            let config_path = config_dir.join("log_colors.json");
            
            println!("Loading colors from: {}", config_path.display());
            
            if let Ok(file_content) = std::fs::read_to_string(&config_path) {
                match serde_json::from_str(&file_content) {
                    Ok(colors) => {
                        println!("Successfully loaded colors from file");
                        colors
                    }
                    Err(e) => {
                        eprintln!("Failed to parse colors JSON: {}", e);
                        LogColors::default()
                    }
                }
            } else {
                println!("No saved colors found, using defaults");
                LogColors::default()
            }
        });
        
        // Configure custom colors for different event types (only if they don't exist)
        Self::configure_custom_log_colors_if_missing(&mut log_colors);

         // Create banner and details instances
        let mut banner = banner::Banner::new();
        let mut details = details::Details::new();
        
        // Format banner and get system info
        banner.format();
        details.get_os();

        let app = Self {
            layers,
            active_layer: LayerType::TopCopper,
            gerber_layer,
            view_state: Default::default(),
            needs_initial_view: true,
            rotation_degrees: 0.0,
            ui_state: Default::default(),
            
            // Logger state
            logger_state,
            log_colors,
            banner,
            details,
            
            // Properties with defaults
            enable_unique_colors: ENABLE_UNIQUE_SHAPE_COLORS,
            enable_polygon_numbering: ENABLE_POLYGON_NUMBERING,
            mirroring: MIRRORING.into(),
            center_offset: CENTER_OFFSET,
            design_offset: DESIGN_OFFSET,
            showing_top: true,
            
            // DRC Properties
            current_drc_ruleset: None,
            
            // Grid Settings
            grid_settings: GridSettings::default(),
        };
        
        // Setup color change watcher to auto-save when colors change
        app.watch_for_color_changes();

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
        // Create a logger using references to our Dynamic state
        let logger = ReactiveEventLogger::with_colors(&self.logger_state, &self.log_colors);
        
        // Log banner message (welcome message)
        logger.log_info(&self.banner.message);
        
        // Log system details
        let details_text = self.details.clone().format_os();
        logger.log_info(&details_text);
     }

    fn reset_view(&mut self, viewport: Rect) {
        // Find bounding box from all loaded layers
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
        // Create a logger for this frame
        let _logger = ReactiveEventLogger::with_colors(&self.logger_state, &self.log_colors);
        
        let show_system_info = ctx.memory(|mem| {
            mem.data.get_temp::<bool>(egui::Id::new("show_system_info")).unwrap_or(false)
        });
        
        if show_system_info {
            // Clear the flag
            ctx.memory_mut(|mem| {
                mem.data.remove::<bool>(egui::Id::new("show_system_info"));
            });
            
            // Create a logger to display system info
            let logger = ReactiveEventLogger::with_colors(&self.logger_state, &self.log_colors);
            
            // Log system details
            let details_text = self.details.format_os();
            logger.log_info(&details_text);
            
            // Then log banner message
            logger.log_info(&self.banner.message);
        }
        
        // No more automatic rotation

        //
        // Compute bounding box and outline
        //
        let bbox = self.gerber_layer.bounding_box();

        let origin = self.center_offset - self.design_offset;

        let transform = Transform2D {
            rotation_radians: self.rotation_degrees.to_radians(),
            mirroring: self.mirroring,
            origin,
            offset: self.design_offset,
        };

        // Compute rotated outline (GREEN)
        let outline_vertices: Vec<_> = bbox
            .vertices()
            .into_iter()
            .map(|v| transform.apply_to_position(v))
            .collect();

        // Compute transformed AABB (RED)
        let bbox = BoundingBox::from_points(&outline_vertices);

        // Convert to screen coords
        let bbox_vertices_screen = bbox.vertices().into_iter()
            .map(|v| self.view_state.gerber_to_screen_coords(v))
            .collect::<Vec<_>>();

        let outline_vertices_screen = outline_vertices.into_iter()
            .map(|v| self.view_state.gerber_to_screen_coords(v))
            .collect::<Vec<_>>();
        
        //
        // Build a UI
        //
        
        // Show the properties panel using our modular UI
        ui::show_properties_panel(ctx, self);

        egui::CentralPanel::default().show(ctx, |ui| {
            ui.centered_and_justified(|ui| {
                    let response = ui.allocate_rect(ui.available_rect_before_wrap(), egui::Sense::drag());
                    let viewport = response.rect;

                    if self.needs_initial_view {
                        self.reset_view(viewport)
                    }
                    
                    //
                    // handle pan, drag and cursor position
                    //
                    self.ui_state.update(ui, &viewport, &response, &mut self.view_state);

                    //
                    // Show the gerber layer and other overlays
                    //

                    let painter = ui.painter().with_clip_rect(viewport);
                    
                    // Draw grid if enabled (before other elements so it appears underneath)
                    grid::draw_grid(&painter, &viewport, &self.view_state, &self.grid_settings);
                    
                    draw_crosshair(&painter, self.ui_state.origin_screen_pos, Color32::BLUE);
                    draw_crosshair(&painter, self.ui_state.center_screen_pos, Color32::LIGHT_GRAY);

                    // Render all visible layers based on showing_top
                    for layer_type in LayerType::all() {
                        if let Some(layer_info) = self.layers.get(&layer_type) {
                            if layer_info.visible {
                                // Filter based on showing_top
                                let should_render = layer_type.should_render(self.showing_top);
                                
                                if should_render {
                                    // Use the layer's specific gerber data if available, otherwise fall back to demo
                                    let gerber_to_render = layer_info.gerber_layer.as_ref()
                                        .unwrap_or(&self.gerber_layer);
                                    
                                    GerberRenderer::default().paint_layer(
                                        &painter,
                                        self.view_state,
                                        gerber_to_render,
                                        layer_type.color(),
                                        false, // Don't use unique colors for multi-layer view
                                        false, // Don't show polygon numbering
                                        self.rotation_degrees.to_radians(),
                                        self.mirroring,
                                        self.center_offset.into(),
                                        self.design_offset.into(),
                                    );
                                }
                            }
                        }
                    }

                    draw_outline(&painter, bbox_vertices_screen, Color32::RED);
                    draw_outline(&painter, outline_vertices_screen, Color32::GREEN);

                    let screen_radius = MARKER_RADIUS * self.view_state.scale;

                    let design_offset_screen_position = self.view_state.gerber_to_screen_coords(self.design_offset.to_position());
                    draw_arrow(&painter, design_offset_screen_position, self.ui_state.origin_screen_pos, Color32::ORANGE);
                    draw_marker(&painter, design_offset_screen_position, Color32::ORANGE, Color32::YELLOW, screen_radius);

                    let design_origin_screen_position = self.view_state.gerber_to_screen_coords((self.center_offset - self.design_offset).to_position());
                    draw_marker(&painter, design_origin_screen_position, Color32::PURPLE, Color32::MAGENTA, screen_radius);
                    
                    // Draw board dimensions in mils at the bottom
                    if let Some(layer_info) = self.layers.get(&LayerType::MechanicalOutline) {
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
                });
        });
    }
}

/// The main function is the entry point of the application.
/// 
/// It initializes the logger, sets up the native window options,
/// and runs the application using the `eframe` framework.
fn main() -> eframe::Result<()> {
    env_logger::init(); // Log to stderr (optional).
    eframe::run_native(
        "Gerber Viewer Lens Demo (egui)",
        eframe::NativeOptions {
            viewport: ViewportBuilder::default().with_inner_size([1280.0, 768.0]),
            ..Default::default()
        },
        Box::new(|_cc| Ok(Box::new(DemoLensApp::new()))),
    )
}