use std::{fs, path::PathBuf};

use eframe::emath::{Rect, Vec2};
use egui::Pos2;
use egui::ViewportBuilder;
use egui_dock::{DockArea, DockState, NodeIndex, Style, SurfaceIndex};

mod display;
use display::DisplayManager;
use layer_operations::LayerManager;
use drc_operations::DrcManager;

/// egui_lens imports
use egui_lens::{ReactiveEventLogger, ReactiveEventLoggerState, LogColors};
use egui_mobius_reactive::*; 
use log;
use gerber_viewer::{
   BoundingBox, GerberLayer, 
   ViewState, UiState, Transform2D
};
// Import platform modules
mod platform;
use platform::parameters::gui::{APPLICATION_NAME, VERSION};
// Import new modules
mod project;
mod layer_operations;
mod drc_operations;
mod ui;
use ui::{Tab, TabKind, TabViewer, initialize_and_show_banner, show_system_info};

use layer_operations::{LayerType, LayerInfo};
use project::{load_default_gerbers, load_demo_gerber, ProjectManager, ProjectState};
use display::GridSettings;

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
    

    // Dock state
    dock_state: DockState<Tab>,
    config_path: PathBuf,
    
    
    // Zoom window state
    pub zoom_window_start: Option<Pos2>,
    pub zoom_window_dragging: bool,
    
    // User preferences
    pub user_timezone: Option<String>,
    pub use_24_hour_clock: bool, // true = 24-hour, false = 12-hour
    
    // Modal states
    pub show_about_modal: bool,
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
    pub fn new() -> Self {

        let gerber_layer = load_demo_gerber();
        let layer_manager = load_default_gerbers();
        
        let logger_state = Dynamic::new(ReactiveEventLoggerState::new());
        let log_colors = Dynamic::new(LogColors::default());
        
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
            
            let mut dock_state = DockState::new(vec![gerber_tab]);
            let surface = dock_state.main_surface_mut();
            
            let [left, _right] = surface.split_left(
                NodeIndex::root(),
                0.3, // Left panel takes 30% of width
                vec![view_settings_tab, drc_tab, project_tab, settings_tab],
            );
            
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
            dock_state,
            config_path: dirs::config_dir()
                .map(|d| d.join("kiforge"))
                .unwrap_or_default(),
            zoom_window_start: None,
            zoom_window_dragging: false,
            user_timezone: None,
            use_24_hour_clock: true, // Default to 24-hour format
            show_about_modal: false,
        };
        
        if let Ok(project_manager) = ProjectManager::load_from_file(&app.config_path) {
            app.project_manager = project_manager;
            
        }
        
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
                self.project_manager.manage_project_state();
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
                        min: nalgebra::Point2::new(
                            existing.min.x.min(layer_bbox.min.x),
                            existing.min.y.min(layer_bbox.min.y),
                        ),
                        max: nalgebra::Point2::new(
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

        // Create the transform that will be used during rendering
        let origin: nalgebra::Vector2<f64> = self.display_manager.center_offset.clone().into();
        let offset: nalgebra::Vector2<f64> = self.display_manager.design_offset.clone().into();
        let transform = Transform2D {
            rotation_radians: self.rotation_degrees.to_radians(),
            mirroring: self.display_manager.mirroring.clone().into(),
            origin: origin - offset,
            offset,
        };

        // Compute transformed bounding box
        let outline_vertices: Vec<_> = bbox
            .vertices()
            .into_iter()
            .map(|v| transform.apply_to_position(v))
            .collect();

        let transformed_bbox = BoundingBox::from_points(&outline_vertices);

        // Use the center of the transformed bounding box
        let transformed_center = transformed_bbox.center();

        // Offset from viewport center to place content in the center
        self.view_state.translation = Vec2::new(
            viewport.center().x - (transformed_center.x as f32 * scale),
            viewport.center().y + (transformed_center.y as f32 * scale), // Note the + here since we flip Y
        );

        self.view_state.scale = scale;
        self.needs_initial_view = false;
    }
    
    
    /// Show clock display in the upper right corner
    fn show_clock_display(&mut self, ui: &mut egui::Ui) {
        use chrono::{Local, Utc};
        use chrono_tz::Tz;
        
        // Show version as clickable button
        if ui.button(egui::RichText::new(format!("KiForge v{}", VERSION))
            .color(egui::Color32::from_rgb(100, 150, 200))).clicked() {
            self.show_about_modal = true;
        }
        
        ui.separator();
        
        // Show clock with user's preferred format
        let time_format = if self.use_24_hour_clock { "%H:%M:%S" } else { "%I:%M:%S %p" };
        
        let clock_text = if let Some(tz_name) = &self.user_timezone {
            if let Ok(tz) = tz_name.parse::<Tz>() {
                let now = Utc::now().with_timezone(&tz);
                format!("🕐 {} {}", now.format(time_format), tz.name())
            } else {
                let now = Local::now();
                format!("🕐 {}", now.format(time_format))
            }
        } else {
            let now = Local::now();
            format!("🕐 {}", now.format(time_format))
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
            
            // R key - rotate board 90 degrees clockwise
            if i.key_pressed(egui::Key::R) {
                // Update rotation
                self.rotation_degrees = (self.rotation_degrees + 90.0) % 360.0;
                
                // Trigger view update to recalculate centering with new rotation
                self.needs_initial_view = true;
                
                let logger = ReactiveEventLogger::with_colors(&self.logger_state, &self.log_colors);
                logger.log_custom(
                    project::constants::LOG_TYPE_ROTATION,
                    &format!("Rotated board to {:.0}° (R key)", self.rotation_degrees)
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
                            self.project_manager.state = ProjectState::PcbSelected { pcb_path: path_buf.clone() };
                            let logger = ReactiveEventLogger::with_colors(&self.logger_state, &self.log_colors);
                            logger.log_info(&format!("Selected PCB file: {}", path_buf.display()));
                        }
                    });
                });
                
                // Hotkeys menu
                ui.menu_button("📋 Hotkeys", |ui| {
                    ui.heading("Keyboard Shortcuts");
                    ui.separator();
                    
                    ui.horizontal(|ui| {
                        ui.label("F");
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            ui.label("Flip Top/Bottom view");
                        });
                    });
                    
                    ui.horizontal(|ui| {
                        ui.label("R");
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            ui.label("Rotate 90° clockwise");
                        });
                    });
                    
                    ui.horizontal(|ui| {
                        ui.label("U");
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            ui.label("Toggle units (mm/mils)");
                        });
                    });
                    
                    ui.separator();
                    ui.heading("Mouse Controls");
                    
                    ui.horizontal(|ui| {
                        ui.label("Double-click");
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            ui.label("Center view");
                        });
                    });
                    
                    ui.horizontal(|ui| {
                        ui.label("Right-click + drag");
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            ui.label("Zoom to selection");
                        });
                    });
                    
                    ui.horizontal(|ui| {
                        ui.label("Scroll wheel");
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            ui.label("Zoom in/out");
                        });
                    });
                    
                    ui.horizontal(|ui| {
                        ui.label("Drag");
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            ui.label("Pan view");
                        });
                    });
                    
                    ui.horizontal(|ui| {
                        ui.label("Escape");
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            ui.label("Cancel zoom selection");
                        });
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
        
        // Show About modal if requested
        if self.show_about_modal {
            egui::Window::new("About KiForge")
                .collapsible(false)
                .resizable(false)
                .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
                .show(ctx, |ui| {
                    ui::AboutPanel::render(ui);
                    
                    ui.add_space(20.0);
                    ui.horizontal(|ui| {
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui.button("Close").clicked() {
                                self.show_about_modal = false;
                            }
                        });
                    });
                });
        }
        
        // Save dock state to disk periodically
        if ctx.input(|i| i.time) % 30.0 < 0.1 {
            self.save_dock_state();
        }
    }
}


fn main() -> eframe::Result<()> {
    // Configure env_logger to filter out gerber_parser warnings
    env_logger::Builder::from_default_env()
        .filter_module("gerber_parser::parser", log::LevelFilter::Off)
        .init();
    eframe::run_native(
        APPLICATION_NAME,
        eframe::NativeOptions {
            viewport: ViewportBuilder::default().with_inner_size([1280.0, 768.0]),
            ..Default::default()
        },
        Box::new(|cc|{
            egui_extras::install_image_loaders(&cc.egui_ctx);
            Ok(Box::new(DemoLensApp::new()))
        }))
    
}