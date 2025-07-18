use std::{fs, path::PathBuf};

use eframe::emath::{Rect, Vec2};
use egui::Pos2;
use egui_dock::{DockArea, DockState, NodeIndex, Style, SurfaceIndex};

use crate::display;
use crate::display::DisplayManager;
use crate::drc_operations::DrcManager;

/// egui_lens imports
use egui_lens::{ReactiveEventLogger, ReactiveEventLoggerState, LogColors};
use egui_mobius_reactive::*; 
use gerber_viewer::{
   BoundingBox, GerberLayer, 
   ViewState, UiState, GerberTransform
};
// Import platform modules
use crate::platform::parameters::gui::VERSION;
// Import new modules
use crate::project;
use crate::ui;
use crate::ecs;
use crate::project_manager;

use crate::ui::{Tab, TabKind, TabViewer, initialize_and_show_banner, show_system_info};

use crate::project::{load_demo_gerber, ProjectManager, ProjectState, manager::ProjectConfig};
use crate::display::GridSettings;

/// The main application struct
pub struct DemoLensApp {
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
    
    // ECS World
    pub ecs_world: bevy_ecs::world::World,

    // Dock state
    dock_state: DockState<Tab>,
    pub config_path: PathBuf,
    
    
    // Zoom window state
    pub zoom_window_start: Option<Pos2>,
    pub zoom_window_dragging: bool,
    
    // User preferences
    pub user_timezone: Option<String>,
    pub use_24_hour_clock: bool, // true = 24-hour, false = 12-hour
    
    // Modal states
    pub show_about_modal: bool,
    
    // Origin setting mode
    pub setting_origin_mode: bool,
    
    // Track if origin has been set by user
    pub origin_has_been_set: bool,
    
    // Enterprise feature: Ruler tool
    pub ruler_active: bool,
    pub ruler_start: Option<nalgebra::Point2<f64>>,
    pub ruler_end: Option<nalgebra::Point2<f64>>,
    pub ruler_dragging: bool,
    pub ruler_drag_start: Option<nalgebra::Point2<f64>>,
    
    // Latched measurement (persists after measurement mode is exited)
    pub latched_measurement_start: Option<nalgebra::Point2<f64>>,
    pub latched_measurement_end: Option<nalgebra::Point2<f64>>,
    
    
    // BOM panel state
    pub bom_state: Option<ui::BomPanelState>,
    
    // Pending BOM components (loaded from project before BOM tab is opened)
    pub pending_bom_components: Option<Vec<project_manager::bom::BomComponent>>,
    
    // Cross-probe signal handling
    pub cross_probe_slot: Option<egui_mobius::slot::Slot<project_manager::bom::BomComponent>>,
    pub cross_probe_slot_started: bool,
    pub pending_cross_probe: egui_mobius::types::Value<Option<project_manager::bom::BomComponent>>,
    
    // Project manager state
    pub project_manager_state: Option<project_manager::ProjectManagerState>,
}

impl Drop for DemoLensApp {
    fn drop(&mut self) {
        // Save dock state when application closes
        self.save_dock_state();
        // Save project config with time settings
        self.save_settings();
    }
}

impl DemoLensApp {
    /// Sync units between legacy global_units_mils and ECS UnitsResource
    pub fn sync_units_to_ecs(&mut self) {
        if let Some(mut units_resource) = self.ecs_world.get_resource_mut::<ecs::UnitsResource>() {
            if self.global_units_mils {
                units_resource.set_mils();
            } else {
                units_resource.set_mm();
            }
        }
    }
    
    /// Sync units from ECS UnitsResource to legacy global_units_mils
    pub fn sync_units_from_ecs(&mut self) {
        if let Some(units_resource) = self.ecs_world.get_resource::<ecs::UnitsResource>() {
            self.global_units_mils = units_resource.is_mils();
        }
    }
    
    /// Sync zoom from legacy view_state to ECS ZoomResource
    pub fn sync_zoom_to_ecs(&mut self) {
        if let Some(mut zoom_resource) = self.ecs_world.get_resource_mut::<ecs::ZoomResource>() {
            zoom_resource.set_scale(self.view_state.scale);
            zoom_resource.set_center(self.view_state.translation.x, self.view_state.translation.y);
        }
    }
    
    /// Sync zoom from ECS ZoomResource to legacy view_state
    pub fn sync_zoom_from_ecs(&mut self) {
        if let Some(zoom_resource) = self.ecs_world.get_resource::<ecs::ZoomResource>() {
            self.view_state.scale = zoom_resource.scale;
            self.view_state.translation.x = zoom_resource.center_x;
            self.view_state.translation.y = zoom_resource.center_y;
        }
    }
    
    /// Render layers using ECS system
    pub fn render_layers_ecs(&mut self, painter: &egui::Painter) {
        // Update view state resource
        self.ecs_world.insert_resource(ecs::ViewStateResource {
            view_state: self.view_state.clone(),
            view_mode: ecs::ViewMode::Normal, // Will be updated based on display manager
        });
        
        // Run ECS systems to update entity states
        ecs::run_ecs_systems(&mut self.ecs_world, &self.display_manager, self.rotation_degrees);
        
        // Use the new ECS render system
        ecs::execute_render_system(
            &mut self.ecs_world,
            painter,
            self.view_state,
            &self.display_manager,
            true, // Use enhanced rendering with quadrant support
        );
    }

    pub fn new() -> Self {

        let gerber_layer = load_demo_gerber();
        let display_manager = DisplayManager::new();
        
        // Force initial view setup to center gerber at origin
        let dummy_viewport = Rect::from_min_size(Pos2::ZERO, Vec2::new(1280.0, 768.0));
        
        let mut initial_logger_state = ReactiveEventLoggerState::new();
        // Set timestamp to be unchecked by default
        initial_logger_state.show_timestamps = false;
        let logger_state = Dynamic::new(initial_logger_state);
        let log_colors = Dynamic::new(LogColors::default());
        let dock_state = Self::create_default_dock_state();
        
        // Setup ECS world without default gerbers (pure ECS now)
        let ecs_world = ecs::setup_ecs_world();
        
        let mut app = Self {
            gerber_layer,
            view_state: ViewState::default(),
            ui_state: UiState::default(),
            needs_initial_view: true,
            rotation_degrees: 0.0,
            logger_state,
            log_colors,
            display_manager,
            drc_manager: DrcManager::new(),
            global_units_mils: false, // Default to mm
            grid_settings: GridSettings::default(),
            project_manager: ProjectManager::new(),
            ecs_world,
            dock_state,
            config_path: dirs::config_dir()
                .map(|d| d.join("copperforge"))
                .unwrap_or_default(),
            zoom_window_start: None,
            zoom_window_dragging: false,
            user_timezone: None,
            use_24_hour_clock: false, // Default to 12-hour format
            show_about_modal: false,
            setting_origin_mode: false,
            origin_has_been_set: false,
            ruler_active: false,
            ruler_start: None,
            ruler_end: None,
            ruler_dragging: false,
            ruler_drag_start: None,
            latched_measurement_start: None,
            latched_measurement_end: None,
            bom_state: None,
            pending_bom_components: None,
            cross_probe_slot: None,
            cross_probe_slot_started: false,
            pending_cross_probe: egui_mobius::types::Value::new(None),
            project_manager_state: None,
        };
        
        if let Ok(project_config) = ProjectConfig::load_from_file(&app.config_path) {
            // Load time settings from saved config
            app.user_timezone = project_config.user_timezone.clone();
            app.use_24_hour_clock = project_config.use_24_hour_clock;
            app.global_units_mils = project_config.global_units_mils;
            
            // Sync units with ECS resource
            if let Some(mut units_resource) = app.ecs_world.get_resource_mut::<ecs::UnitsResource>() {
                if app.global_units_mils {
                    units_resource.set_mils();
                } else {
                    units_resource.set_mm();
                }
            }
            
            app.project_manager = ProjectManager::from_config(project_config);
        }
        
        let logger = ReactiveEventLogger::with_colors(&app.logger_state, &app.log_colors);
        initialize_and_show_banner(&logger);
        app.initialize_project();
        
        // Force reset view to center the gerber at origin
        app.reset_view(dummy_viewport);
        
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

    pub fn reset_view(&mut self, viewport: Rect) {
        // Find bounding box from all loaded layers using ECS
        let combined_bbox = crate::ecs::get_combined_bounding_box(&mut self.ecs_world);
        
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

        // Handle custom origin case differently
        if self.display_manager.design_offset.x != 0.0 || self.display_manager.design_offset.y != 0.0 {
            // When we have a custom origin, position (0,0) at the lower-left area of the viewport
            // This ensures the PCB is always visible
            
            // Position (0,0) at 20% from left and 20% from bottom of viewport
            let origin_screen_x = viewport.left() + viewport.width() * 0.2;
            let origin_screen_y = viewport.bottom() - viewport.height() * 0.2; // Remember Y is flipped
            
            // The origin in gerber coordinates is at design_offset
            // We need to translate the view so that this point appears at our desired screen position
            let origin_gerber_x = self.display_manager.design_offset.x;
            let origin_gerber_y = self.display_manager.design_offset.y;
            
            // Calculate the translation needed
            // Screen position = translation + (gerber_position * scale)
            // Therefore: translation = screen_position - (gerber_position * scale)
            self.view_state.translation = Vec2::new(
                origin_screen_x - (origin_gerber_x as f32 * scale),
                origin_screen_y + (origin_gerber_y as f32 * scale), // + because Y is flipped in screen coords
            );
        } else {
            // Standard case: no custom origin
            let gerber_center = bbox.center();
            
            // Set center offset to negate the gerber center, forcing it to (0,0)
            self.display_manager.center_offset = display::VectorOffset {
                x: -gerber_center.x,
                y: -gerber_center.y,
            };

            // Create the transform that will be used during rendering
            let origin: nalgebra::Vector2<f64> = self.display_manager.center_offset.clone().into();
            let offset: nalgebra::Vector2<f64> = self.display_manager.design_offset.clone().into();
            let transform = GerberTransform {
                rotation: self.rotation_degrees.to_radians(),
                mirroring: self.display_manager.mirroring.clone().into(),
                origin: origin - offset,
                offset,
                scale: 1.0,
            };

            // Compute transformed bounding box
            let outline_vertices: Vec<_> = bbox
                .vertices()
                .into_iter()
                .map(|v| transform.apply_to_position(v))
                .collect();

            let transformed_bbox = BoundingBox::from_points(&outline_vertices);
            let transformed_center = transformed_bbox.center();

            // Center the view
            self.view_state.translation = Vec2::new(
                viewport.center().x - (transformed_center.x as f32 * scale),
                viewport.center().y + (transformed_center.y as f32 * scale),
            );
        }

        self.view_state.scale = scale;
        
        // Update ECS zoom resource and set fit-to-view reference
        if let Some(mut zoom_resource) = self.ecs_world.get_resource_mut::<ecs::ZoomResource>() {
            zoom_resource.set_scale(scale);
            zoom_resource.set_fit_to_view_scale(scale); // This scale becomes the 100% reference
            zoom_resource.set_center(self.view_state.translation.x, self.view_state.translation.y);
        }
        
        self.needs_initial_view = false;
    }
    
    /// Zoom to a specific BOM component location
    pub fn zoom_to_component(&mut self, component: &project_manager::bom::BomComponent, viewport: Rect) {
        // Only allow cross-probing if origin has been set
        if !self.origin_has_been_set {
            let logger = ReactiveEventLogger::with_colors(&self.logger_state, &self.log_colors);
            logger.log_warning("Please set the origin before using cross-probing");
            return;
        }
        
        // Component coordinates from KiCad (in mm)
        let comp_x = component.x_location;
        let comp_y = component.y_location;
        
        // Just center the component at the current zoom level
        let viewport_center = viewport.center();
        self.view_state.translation = Vec2::new(
            viewport_center.x - (comp_x as f32 * self.view_state.scale),
            viewport_center.y + (comp_y as f32 * self.view_state.scale),
        );
        
        // Update ECS view state
        if let Some(mut view_state_resource) = self.ecs_world.get_resource_mut::<ecs::ViewStateResource>() {
            view_state_resource.view_state = self.view_state.clone();
        }
        
        // Log the action
        let logger = ReactiveEventLogger::with_colors(&self.logger_state, &self.log_colors);
        logger.log_info(&format!("Cross-probed to component: {} at ({:.2}, {:.2})", 
                                component.reference, comp_x, comp_y));
    }
    
    
    /// Show clock display in the upper right corner
    fn show_clock_display(&mut self, ui: &mut egui::Ui) {
        use chrono::{Local, Utc};
        use chrono_tz::Tz;
        
        // Show version as clickable button
        if ui.button(egui::RichText::new(format!("CopperForge v{}", VERSION))
            .color(egui::Color32::from_rgb(180, 200, 255))).clicked() {
            self.show_about_modal = true;
        }
        
        ui.separator();
        
        // Show clock with user's preferred format
        let time_format = if self.use_24_hour_clock { "%H:%M:%S" } else { "%I:%M:%S %p" };
        let date_format = "%Y-%m-%d";
        
        let clock_text = if let Some(tz_name) = &self.user_timezone {
            if let Ok(tz) = tz_name.parse::<Tz>() {
                let now = Utc::now().with_timezone(&tz);
                format!("{} 🕐 {} {}", now.format(date_format), now.format(time_format), tz.name())
            } else {
                let now = Local::now();
                format!("{} 🕐 {}", now.format(date_format), now.format(time_format))
            }
        } else {
            let now = Local::now();
            format!("{} 🕐 {}", now.format(date_format), now.format(time_format))
        };
        
        ui.label(egui::RichText::new(clock_text).color(egui::Color32::from_rgb(220, 220, 220)));
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
            let copperforge_dir = config_dir.join("copperforge");
            if let Err(e) = fs::create_dir_all(&copperforge_dir) {
                eprintln!("Failed to create config directory: {}", e);
                return;
            }
            let config_path = copperforge_dir.join("dock_state.json");
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
            let config_path = config_dir.join("copperforge").join("dock_state.json");
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
    
    fn save_settings(&self) {
        let mut config = self.project_manager.config.clone();
        config.state = self.project_manager.state.clone(); // Save current project state!
        config.user_timezone = self.user_timezone.clone();
        config.use_24_hour_clock = self.use_24_hour_clock;
        config.global_units_mils = self.global_units_mils;
        
        if let Err(e) = config.save_to_file(&self.config_path) {
            eprintln!("Failed to save settings: {}", e);
        }
    }
    
    fn create_default_dock_state() -> DockState<Tab> {
        if let Some(saved_dock_state) = Self::load_dock_state() {
            return saved_dock_state;
        }
        
        let view_settings_tab = Tab::new(TabKind::ViewSettings, SurfaceIndex::main(), NodeIndex(0));
        let drc_tab = Tab::new(TabKind::DRC, SurfaceIndex::main(), NodeIndex(1));
        let project_tab = Tab::new(TabKind::Project, SurfaceIndex::main(), NodeIndex(2));
        let settings_tab = Tab::new(TabKind::Settings, SurfaceIndex::main(), NodeIndex(3));
        let gerber_tab = Tab::new(TabKind::GerberView, SurfaceIndex::main(), NodeIndex(4));
        let log_tab = Tab::new(TabKind::EventLog, SurfaceIndex::main(), NodeIndex(5));
        let bom_tab = Tab::new(TabKind::BOM, SurfaceIndex::main(), NodeIndex(6));
        
        let mut dock_state = DockState::new(vec![gerber_tab]);
        let surface = dock_state.main_surface_mut();
        
        let [left, _right] = surface.split_left(
            NodeIndex::root(),
            0.3,
            vec![view_settings_tab, drc_tab, project_tab, settings_tab, bom_tab],
        );
        
        surface.split_below(left, 0.7, vec![log_tab]);
        dock_state
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
        
        // Only update coordinates when explicitly marked as dirty (not time-based)
        if crate::ecs::are_coordinates_dirty(&self.ecs_world) {
            // Use ECS-based coordinate updates for better sync
            crate::ecs::update_coordinates_from_display(&mut self.ecs_world, &self.display_manager);
        }
        
        // Process cross-probe signals from BOM component selection
        if let Some(ref mut cross_probe_slot) = self.cross_probe_slot {
            // Check if slot is not started yet
            if !self.cross_probe_slot_started {
                let pending_cross_probe = self.pending_cross_probe.clone();
                
                cross_probe_slot.start(move |component: project_manager::bom::BomComponent| {
                    // Store the component for the UI thread to process
                    *pending_cross_probe.lock().unwrap() = Some(component);
                });
                
                self.cross_probe_slot_started = true;
            }
        }
        
        // Check if there's a pending cross-probe to process
        let pending_component = {
            self.pending_cross_probe.lock().unwrap().take()
        };
        
        if let Some(component) = pending_component {
            // Get the current viewport
            let viewport = ctx.available_rect();
            
            // Zoom to the selected component
            self.zoom_to_component(&component, viewport);
            
            // Log the cross-probe action
            let logger = ReactiveEventLogger::with_colors(&self.logger_state, &self.log_colors);
            logger.log_info(&format!("Cross-probed to component: {} at ({:.2}, {:.2})", 
                                    component.reference, component.x_location, component.y_location));
            
            // Request repaint to show the zoomed view
            ctx.request_repaint();
        }
        
        // No longer need legacy sync - UI uses ECS directly
        
        // Handle hotkeys first (but only if no text field has focus)
        let text_input_active = ctx.memory(|mem| mem.focused().is_some());
        
        if !text_input_active {
            ctx.input(|i| {
                // F key - flip board view (top/bottom)
                if i.key_pressed(egui::Key::F) {
                self.display_manager.showing_top = !self.display_manager.showing_top;
                
                // Auto-toggle layer visibility based on flip state using ECS
                for layer_type in crate::ecs::LayerType::all() {
                    let visible = match layer_type {
                        crate::ecs::LayerType::Copper(1) |
                        crate::ecs::LayerType::Silkscreen(crate::ecs::Side::Top) |
                        crate::ecs::LayerType::Soldermask(crate::ecs::Side::Top) |
                        crate::ecs::LayerType::Paste(crate::ecs::Side::Top) => {
                            self.display_manager.showing_top
                        },
                        crate::ecs::LayerType::Copper(_) => {
                            !self.display_manager.showing_top
                        },
                        crate::ecs::LayerType::Silkscreen(crate::ecs::Side::Bottom) |
                        crate::ecs::LayerType::Soldermask(crate::ecs::Side::Bottom) |
                        crate::ecs::LayerType::Paste(crate::ecs::Side::Bottom) => {
                            !self.display_manager.showing_top
                        },
                        crate::ecs::LayerType::MechanicalOutline => {
                            // Leave outline visibility unchanged, get current state from ECS
                            crate::ecs::get_layer_visibility(&mut self.ecs_world, layer_type)
                        }
                    };
                    crate::ecs::set_layer_visibility(&mut self.ecs_world, layer_type, visible);
                }
                
                let view_name = if self.display_manager.showing_top { "top" } else { "bottom" };
                let logger = ReactiveEventLogger::with_colors(&self.logger_state, &self.log_colors);
                logger.log_info(&format!("Flipped to {} view (F key)", view_name));
                // Mark coordinates as dirty since view changed
                crate::ecs::mark_coordinates_dirty_ecs(&mut self.ecs_world);
            }
            
            // U key - toggle units (mm/mils)
            if i.key_pressed(egui::Key::U) {
                self.global_units_mils = !self.global_units_mils;
                self.sync_units_to_ecs(); // Sync to ECS units system
                let units_name = if self.global_units_mils { "mils" } else { "mm" };
                let logger = ReactiveEventLogger::with_colors(&self.logger_state, &self.log_colors);
                logger.log_info(&format!("Toggled units to {} (U key)", units_name));
            }
            
            // R key - rotate board 90 degrees clockwise
            if i.key_pressed(egui::Key::R) {
                // Update rotation
                self.rotation_degrees = (self.rotation_degrees + 90.0) % 360.0;
                
                // Don't reset view - just mark coordinates as dirty to update rotation
                // This keeps the view centered on the current origin
                crate::ecs::mark_coordinates_dirty_ecs(&mut self.ecs_world);
                
                let logger = ReactiveEventLogger::with_colors(&self.logger_state, &self.log_colors);
                logger.log_custom(
                    project::constants::LOG_TYPE_ROTATION,
                    &format!("Rotated board to {:.0}° (R key)", self.rotation_degrees)
                );
                }
            
            // A key - align view to grid
            if i.key_pressed(egui::Key::A) {
                display::align_to_grid(&mut self.view_state, &self.grid_settings);
                
                let logger = ReactiveEventLogger::with_colors(&self.logger_state, &self.log_colors);
                logger.log_info("Aligned view to grid (A key)");
                }
            
            // M key - toggle ruler mode with latched measurement support
            if i.key_pressed(egui::Key::M) {
                if self.ruler_active {
                    // Exiting measurement mode - latch the current measurement if complete
                    if self.ruler_start.is_some() && self.ruler_end.is_some() {
                        self.latched_measurement_start = self.ruler_start;
                        self.latched_measurement_end = self.ruler_end;
                    }
                    
                    // Clear ruler when deactivated
                    self.ruler_active = false;
                    self.ruler_start = None;
                    self.ruler_end = None;
                    self.ruler_dragging = false;
                    
                    let logger = ReactiveEventLogger::with_colors(&self.logger_state, &self.log_colors);
                    logger.log_info("Ruler mode deactivated (M key) - measurement latched");
                } else {
                    // Starting new measurement mode - clear previous latched measurement
                    self.latched_measurement_start = None;
                    self.latched_measurement_end = None;
                    
                    self.ruler_active = true;
                    
                    let logger = ReactiveEventLogger::with_colors(&self.logger_state, &self.log_colors);
                    logger.log_info("Ruler mode activated (M key) - previous measurement cleared");
                }
                }
            
            // ESC key - cancel measurement mode with latching support
            if i.key_pressed(egui::Key::Escape) && self.ruler_active {
                // Latch the current measurement if complete
                if self.ruler_start.is_some() && self.ruler_end.is_some() {
                    self.latched_measurement_start = self.ruler_start;
                    self.latched_measurement_end = self.ruler_end;
                    
                    // Debug log the latched values
                    let logger = ReactiveEventLogger::with_colors(&self.logger_state, &self.log_colors);
                    if let (Some(start), Some(end)) = (self.ruler_start, self.ruler_end) {
                        logger.log_info(&format!("Latching measurement - Start: ({:.6}, {:.6}), End: ({:.6}, {:.6})", 
                                                start.x, start.y, end.x, end.y));
                        let dx = end.x - start.x;
                        let dy = end.y - start.y;
                        logger.log_info(&format!("Latching deltas - ΔX: {:.6}, ΔY: {:.6}", dx, dy));
                    }
                }
                
                // Clear ruler when deactivated
                self.ruler_active = false;
                self.ruler_start = None;
                self.ruler_end = None;
                self.ruler_dragging = false;
                
                let logger = ReactiveEventLogger::with_colors(&self.logger_state, &self.log_colors);
                logger.log_info("Ruler mode cancelled (ESC key) - measurement latched");
                }
            });
        }
        
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
                    
                    ui.horizontal(|ui| {
                        ui.label("A");
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            ui.label("Align view to grid");
                        });
                    });
                    
                    ui.horizontal(|ui| {
                        ui.label("M");
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            ui.label("Toggle ruler/measurement mode");
                        });
                    });
                    
                    ui.horizontal(|ui| {
                        ui.label("ESC");
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            ui.label("Cancel measurement mode");
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
                        ui.label("Left-click + drag");
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            ui.label("Pan view");
                        });
                    });
                    
                    ui.horizontal(|ui| {
                        ui.label("Escape");
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            ui.label("Cancel zoom selection / measurement mode");
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
            .show_add_buttons(true)
            .show_close_buttons(true)
            .show(ctx, &mut tab_viewer);
            
        self.dock_state = dock_state;
        
        // Show About modal if requested
        if self.show_about_modal {
            egui::Window::new("About CopperForge")
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


