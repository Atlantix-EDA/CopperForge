use std::{fs, path::PathBuf};

use eframe::emath::{Rect, Vec2};
use egui::Pos2;
use egui::ViewportBuilder;
use egui_dock::{DockArea, DockState, NodeIndex, Style, SurfaceIndex};

mod managers;
use managers::{ProjectManager, ProjectState, DisplayManager};
use layer_operations::LayerManager;
use drc_operations::DrcManager;

/// egui_lens imports
use egui_lens::{ReactiveEventLogger, ReactiveEventLoggerState, LogColors};

/// Use of prelude for egui_mobius_reactive
use egui_mobius_reactive::Dynamic;  

use log;
use gerber_viewer::{
   BoundingBox, GerberLayer, 
   ViewState, UiState
};


// Import platform modules
mod platform;

// Import new modules
mod constants;
mod layer_operations;
mod drc_operations;
mod grid;
mod ui;
use ui::{Tab, TabKind, TabViewer, initialize_and_show_banner, show_system_info};
mod defaults;

use layer_operations::{LayerType, LayerInfo};
// DRC operations are imported where needed in UI modules
use grid::GridSettings;

// DRC structures are now imported from the drc module






/// The main application struct
pub struct DemoLensApp {
    // Layer management
    pub layer_manager: LayerManager,
    
    // Legacy single layer support (for compatibility)
    pub gerber_layer: GerberLayer,
    pub view_state: ViewState,
    pub ui_state: UiState,
    pub needs_initial_view: bool,

    pub rotation_degrees: f32,
    
    // Logger state and colors
    pub logger_state : Dynamic<ReactiveEventLoggerState>,
    pub log_colors   : Dynamic<LogColors>,
    
    // Display settings
    pub display_manager: DisplayManager,
    
    // DRC management
    pub drc_manager: DrcManager,
    
    // Global units setting
    pub global_units_mils: bool, // true = mils, false = mm
    
    // Grid Settings
    pub grid_settings: GridSettings,
    
    // Project management
    pub project_manager: ProjectManager,
    
    // Legacy fields for compatibility (will be removed later)
    pub selected_pcb_file: Option<PathBuf>,
    pub generated_gerber_dir: Option<PathBuf>,
    pub generating_gerbers: bool,
    pub loading_gerbers: bool,

    // Dock state
    dock_state: DockState<Tab>,
    config_path: PathBuf,
    
    
    // Zoom window state
    pub zoom_window_start: Option<Pos2>,
    pub zoom_window_dragging: bool,
    
    // User preferences
    pub user_timezone: Option<String>,
}



impl Drop for DemoLensApp {
    fn drop(&mut self) {
        // Save dock state when application closes
        self.save_dock_state();
        // Save project config
        if let Err(e) = self.project_manager.save_to_file(&self.config_path) {
            eprintln!("Failed to save project config: {}", e);
        }
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
        // Load default gerbers and demo layer
        let gerber_layer = defaults::load_demo_gerber();
        let layer_manager = defaults::load_default_gerbers();
        
        // Create logger state and colors
        let logger_state = Dynamic::new(ReactiveEventLoggerState::new());
        let log_colors = Dynamic::new(LogColors::default());
        

        // Initialize dock state - load from saved state or create default
        let dock_state = if let Some(saved_dock_state) = Self::load_dock_state() {
            saved_dock_state
        } else {
            // Create default dock layout if no saved state exists
            let view_settings_tab = Tab::new(TabKind::ViewSettings, SurfaceIndex::main(), NodeIndex(0));
            let drc_tab = Tab::new(TabKind::DRC, SurfaceIndex::main(), NodeIndex(1));
            let project_tab = Tab::new(TabKind::Project, SurfaceIndex::main(), NodeIndex(2));
            let settings_tab = Tab::new(TabKind::Settings, SurfaceIndex::main(), NodeIndex(3));
            let gerber_tab = Tab::new(TabKind::GerberView, SurfaceIndex::main(), NodeIndex(4));
            let log_tab = Tab::new(TabKind::EventLog, SurfaceIndex::main(), NodeIndex(5));
            
            // Create dock state with gerber view as the root
            let mut dock_state = DockState::new(vec![gerber_tab]);
            let surface = dock_state.main_surface_mut();
            
            // Split left for control panels
            let [left, _right] = surface.split_left(
                NodeIndex::root(),
                0.3, // Left panel takes 30% of width
                vec![view_settings_tab, drc_tab, project_tab, settings_tab],
            );
            
            // Add event log to bottom of left panel
            surface.split_below(
                left,
                0.7, // Top takes 70% of height
                vec![log_tab],
            );
            
            dock_state
        };

        let mut app = Self {
            layer_manager,
            gerber_layer,
            view_state: ViewState::default(),
            ui_state: UiState::default(),
            needs_initial_view: true,
            rotation_degrees: 0.0,
            logger_state,
            log_colors,
            display_manager: DisplayManager::new(),
            drc_manager: DrcManager::new(),
            global_units_mils: false, // Default to mm
            grid_settings: GridSettings::default(),
            project_manager: ProjectManager::new(),
            selected_pcb_file: None,
            generated_gerber_dir: None,
            generating_gerbers: false,
            loading_gerbers: false,
            dock_state,
            config_path: dirs::config_dir()
                .map(|d| d.join("kiforge"))
                .unwrap_or_default(),
            zoom_window_start: None,
            zoom_window_dragging: false,
            user_timezone: None,
        };
        
        // Load project config from disk
        if let Ok(project_manager) = ProjectManager::load_from_file(&app.config_path) {
            app.project_manager = project_manager;
            
            // Sync legacy fields with project state
            match &app.project_manager.state {
                ProjectState::NoProject => {},
                ProjectState::PcbSelected { pcb_path } |
                ProjectState::GeneratingGerbers { pcb_path } => {
                    app.selected_pcb_file = Some(pcb_path.clone());
                },
                ProjectState::GerbersGenerated { pcb_path, gerber_dir } |
                ProjectState::LoadingGerbers { pcb_path, gerber_dir } |
                ProjectState::Ready { pcb_path, gerber_dir, .. } => {
                    app.selected_pcb_file = Some(pcb_path.clone());
                    app.generated_gerber_dir = Some(gerber_dir.clone());
                },
            }
        }
        
        // Add platform details and initialize project
        let logger = ReactiveEventLogger::with_colors(&app.logger_state, &app.log_colors);
        initialize_and_show_banner(&logger);
        app.initialize_project();
        
        app
    }
    
    fn initialize_project(&mut self) {
        let logger = ReactiveEventLogger::with_colors(&self.logger_state, &self.log_colors);
        
        match &self.project_manager.state.clone() {
            ProjectState::NoProject => {
                logger.log_info("No previous project found. Please select a PCB file.");
            },
            _ => {
                // Use the centralized project state management
                self.project_manager.manage_project_state(
                    &mut self.generating_gerbers,
                    &mut self.loading_gerbers,
                    &mut self.generated_gerber_dir
                );
            }
        }
    }


    fn reset_view(&mut self, viewport: Rect) {
        // Find bounding box from all loaded layers
        let mut combined_bbox: Option<BoundingBox> = None;
        
        for layer_info in self.layer_manager.layers.values() {
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
    
    
    /// Show clock display in the upper right corner
    fn show_clock_display(&self, ui: &mut egui::Ui) {
        use chrono::{Local, Utc};
        use chrono_tz::Tz;
        
        // Show version
        ui.label(egui::RichText::new(format!("KiForge v{}", env!("CARGO_PKG_VERSION")))
            .color(egui::Color32::from_rgb(100, 150, 200)));
        
        ui.separator();
        
        // Show clock
        let clock_text = if let Some(tz_name) = &self.user_timezone {
            if let Ok(tz) = tz_name.parse::<Tz>() {
                let now = Utc::now().with_timezone(&tz);
                format!("🕐 {} {}", now.format("%H:%M:%S"), tz.name())
            } else {
                let now = Local::now();
                format!("🕐 {}", now.format("%H:%M:%S"))
            }
        } else {
            let now = Local::now();
            format!("🕐 {}", now.format("%H:%M:%S"))
        };
        
        ui.label(egui::RichText::new(clock_text).color(egui::Color32::from_rgb(150, 150, 150)));
    }
    
    /// Show the main content area (dock layout without Project tab)
    #[allow(dead_code)]
    fn show_main_content(&mut self, ui: &mut egui::Ui) {
        // Clone the dock state but filter out the Project tab
        let mut dock_state = self.dock_state.clone();
        
        // Create the dock layout and tab viewer
        let mut tab_viewer = TabViewer { app: self };
        
        // Create custom style to match panel colors
        let mut style = Style::from_egui(ui.ctx().style().as_ref());
        style.dock_area_padding = None;
        style.tab_bar.fill_tab_bar = true;
        
        // Show the dock area but filtered to exclude Project tab
        DockArea::new(&mut dock_state)
            .style(style)
            .show_add_buttons(false)
            .show_close_buttons(true)
            .show(ui.ctx(), &mut tab_viewer);
            
        // Save the updated dock state back to the app
        self.dock_state = dock_state;
    }
}

impl DemoLensApp {
    fn save_dock_state(&self) {
        if let Some(config_dir) = dirs::config_dir() {
            let kiforge_dir = config_dir.join("kiforge");
            if let Err(e) = fs::create_dir_all(&kiforge_dir) {
                eprintln!("Failed to create config directory: {}", e);
                return;
            }
            let config_path = kiforge_dir.join("dock_state.json");
            match serde_json::to_string_pretty(&self.dock_state) {
                Ok(json) => {
                    if let Err(e) = fs::write(&config_path, json) {
                        eprintln!("Failed to write dock state: {}", e);
                    }
                }
                Err(e) => {
                    eprintln!("Failed to serialize dock state: {}", e);
                }
            }
        }
    }

    fn load_dock_state() -> Option<DockState<Tab>> {
        if let Some(config_dir) = dirs::config_dir() {
            let config_path = config_dir.join("kiforge").join("dock_state.json");
            if let Ok(json) = fs::read_to_string(&config_path) {
                match serde_json::from_str::<DockState<Tab>>(&json) {
                    Ok(dock_state) => {
                        // Successfully loaded dock state
                        return Some(dock_state);
                    }
                    Err(e) => {
                        eprintln!("Failed to deserialize dock state: {}", e);
                        // Delete corrupted file
                        fs::remove_file(config_path).ok();
                    }
                }
            }
        }
        None
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
        let show_system_info_clicked = ctx.memory(|mem| {
            mem.data.get_temp::<bool>(egui::Id::new("show_system_info")).unwrap_or(false)
        });
        
        if show_system_info_clicked {
            // Clear the flag
            ctx.memory_mut(|mem| {
                mem.data.remove::<bool>(egui::Id::new("show_system_info"));
            });
            
            // Show system info
            let logger = ReactiveEventLogger::with_colors(&self.logger_state, &self.log_colors);
            show_system_info(&logger);
        }
        
        // Handle hotkeys first
        ctx.input(|i| {
            // F key - flip board view (top/bottom)
            if i.key_pressed(egui::Key::F) {
                self.display_manager.showing_top = !self.display_manager.showing_top;
                let view_name = if self.display_manager.showing_top { "top" } else { "bottom" };
                let logger = ReactiveEventLogger::with_colors(&self.logger_state, &self.log_colors);
                logger.log_info(&format!("Flipped to {} view (F key)", view_name));
            }
            
            // U key - toggle units (mm/mils)
            if i.key_pressed(egui::Key::U) {
                self.global_units_mils = !self.global_units_mils;
                let units_name = if self.global_units_mils { "mils" } else { "mm" };
                let logger = ReactiveEventLogger::with_colors(&self.logger_state, &self.log_colors);
                logger.log_info(&format!("Toggled units to {} (U key)", units_name));
            }
            
            // R key - rotate board 90 degrees clockwise around PCB centroid
            if i.key_pressed(egui::Key::R) {
                // Calculate the centroid of all visible gerber layers
                let mut combined_bbox: Option<gerber_viewer::BoundingBox> = None;
                
                for (_layer_type, layer_info) in &self.layer_manager.layers {
                    if layer_info.visible {
                        if let Some(ref gerber_layer) = layer_info.gerber_layer {
                            let layer_bbox = gerber_layer.bounding_box();
                            combined_bbox = Some(match combined_bbox {
                                None => layer_bbox.clone(),
                                Some(existing) => gerber_viewer::BoundingBox {
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
                }
                
                // Get the current center point that we're rotating around
                let rotation_center = if let Some(bbox) = combined_bbox {
                    bbox.center()
                } else {
                    // Fallback to current design offset if no layers
                    {
                        let design_vec: gerber_viewer::position::Vector = self.display_manager.design_offset.clone().into();
                        design_vec.to_position()
                    }
                };
                
                // To rotate around a specific point, we need to:
                // 1. Translate so the rotation center is at origin (subtract center)
                // 2. Rotate 90 degrees
                // 3. Translate back (add rotated center)
                
                // Calculate what the rotation center will be after rotation
                let angle_rad = 90.0_f32.to_radians();
                let cos_a = angle_rad.cos() as f64;
                let sin_a = angle_rad.sin() as f64;
                
                // Rotate the center point itself
                let rotated_center_x = rotation_center.x * cos_a - rotation_center.y * sin_a;
                let rotated_center_y = rotation_center.x * sin_a + rotation_center.y * cos_a;
                
                // Update rotation
                self.rotation_degrees = (self.rotation_degrees + 90.0) % 360.0;
                
                // Adjust the design offset to account for the rotation around the centroid
                // The offset difference keeps the same point at the center of rotation
                let offset_adjustment = gerber_viewer::position::Vector::new(
                    rotation_center.x - rotated_center_x,
                    rotation_center.y - rotated_center_y
                );
                
                // Apply the offset adjustment
                {
                    let current_offset: gerber_viewer::position::Vector = self.display_manager.design_offset.clone().into();
                    let new_offset = current_offset + offset_adjustment;
                    self.display_manager.design_offset = managers::display::VectorOffset { x: new_offset.x, y: new_offset.y };
                }
                
                let logger = ReactiveEventLogger::with_colors(&self.logger_state, &self.log_colors);
                logger.log_custom(
                    constants::LOG_TYPE_ROTATION,
                    &format!("Rotated board to {:.0}° around PCB centroid (R key)", self.rotation_degrees)
                );
            }
        });
        
        // Project Ribbon at the top
        egui::TopBottomPanel::top("project_ribbon").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 10.0;
                
                // Project Ribbon with file selection
                ui.group(|ui| {
                    ui.horizontal(|ui| {
                        ui.label("📁 KiCad PCB File:");
                        
                        // Show current file or placeholder
                        let current_file_text = match &self.project_manager.state {
                            ProjectState::NoProject => "No file selected".to_string(),
                            ProjectState::Ready { pcb_path, .. } |
                            ProjectState::PcbSelected { pcb_path } |
                            ProjectState::GeneratingGerbers { pcb_path } |
                            ProjectState::GerbersGenerated { pcb_path, .. } |
                            ProjectState::LoadingGerbers { pcb_path, .. } => {
                                pcb_path.file_name()
                                    .map(|n| n.to_string_lossy().to_string())
                                    .unwrap_or_else(|| "Unknown file".to_string())
                            }
                        };
                        
                        ui.label(egui::RichText::new(current_file_text).strong());
                        
                        if ui.button("Browse...").clicked() {
                            self.project_manager.open_file_dialog();
                        }
                        
                        // Handle file dialog
                        if let Some(path_buf) = self.project_manager.update_file_dialog(ui.ctx()) {
                            self.selected_pcb_file = Some(path_buf.clone());
                            let logger = ReactiveEventLogger::with_colors(&self.logger_state, &self.log_colors);
                            logger.log_info(&format!("Selected PCB file: {}", path_buf.display()));
                        }
                    });
                });
                
                // Clock in the upper right
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    self.show_clock_display(ui);
                });
            });
        });
        
        // Main dock area below the ribbon
        let mut dock_state = self.dock_state.clone();
        let mut tab_viewer = TabViewer { app: self };
        let mut style = Style::from_egui(ctx.style().as_ref());
        style.dock_area_padding = None;
        style.tab_bar.fill_tab_bar = true;
        
        DockArea::new(&mut dock_state)
            .style(style)
            .show_add_buttons(false)
            .show_close_buttons(true)
            .show(ctx, &mut tab_viewer);
            
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
    // Configure env_logger to filter out gerber_parser warnings
    env_logger::Builder::from_default_env()
        .filter_module("gerber_parser::parser", log::LevelFilter::Off)
        .init();
    eframe::run_native(
        "KiForge - PCB & CAM for KiCad",
        eframe::NativeOptions {
            viewport: ViewportBuilder::default().with_inner_size([1280.0, 768.0]),
            ..Default::default()
        },
        Box::new(|_cc| Ok(Box::new(DemoLensApp::new()))),
    )
}