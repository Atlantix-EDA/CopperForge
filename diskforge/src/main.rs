//! Framework Template for egui_mobius
//! 
//! This is a template for creating applications using egui_mobius
//! libraries, with a focus on modular architecture as the ui elements
//! are located in /ui and the core application logic is in /main
//! There are some macros in /logging_macros.rs that are used to 
//! handle logging in a more efficient way. The terminal widget is 
//! located in /ui/logger_panel.rs, which references the terminal widget
//! define in the root of this project, at /src/lib.rs. 
//! 
//! 
// egui_mobius and template crates
mod payloads;
mod logging_macros;
mod platform; 
mod ui;
use platform::parameters::gui;
use ui::{settings_panel, control_panel, TaffyPanel, sd_panel};
// Import the ReactiveEventLogger from egui_lens
use egui_lens::{ReactiveEventLogger, ReactiveEventLoggerState, LogColors};

use egui_mobius_reactive::Dynamic;




// egui and egui_dock crates
use eframe::egui;
use egui_dock::{DockArea, DockState, NodeIndex, Style, SurfaceIndex};

// Standard library

/// TabKind
/// 
/// Define the tabs for the DockArea - where each one holds 
/// a different panel of the application. 
enum TabKind {
    Settings,
    Control,
    About,
    ReactiveLogger, // New tab for reactive event logging
    Taffy,   // Demo of egui_taffy layout
    SD,      // New tab for SD card visualization
}

/// Tab
/// 
/// Define the overall container struct for the tabs of the application.
/// 
/// Define the content for each TabKind in the Tab struct.
/// 
/// Note that the terminal widget is passed to the primary tabs which 
/// are Settings, Control, and Logger. These tabs are the primary tabs
/// that have either events or data to log. If one were to extend the 
/// taffy tab, it would also need to pass the terminal widget.
struct Tab {
    kind      : TabKind,
    _surface  : SurfaceIndex,
    _node     : NodeIndex,
}

/// Parameters for tab content rendering
struct TabParams<'a> {
    reactive_logger_state: &'a Dynamic<ReactiveEventLoggerState>,
    slider_value: &'a mut f32,
    selected_option: &'a mut usize,
    is_running: &'a mut bool,
    is_formatted: &'a mut bool,
    colors: &'a Dynamic<LogColors>,
    volume_label: &'a mut String,
}
impl Tab {
    fn new(kind: TabKind, _surface: SurfaceIndex, _node: NodeIndex) -> Self {
        Self { kind, _surface, _node }
    }
    fn title(&self) -> String {
        match self.kind {
            TabKind::ReactiveLogger => "Reactive Logger".to_string(),
            TabKind::Settings => "Settings".to_string(),
            TabKind::Control => "Control".to_string(),
            TabKind::About => "About".to_string(),
            TabKind::Taffy => "Taffy Layout".to_string(),
            TabKind::SD => "SD".to_string(),
        }
    }
    fn content(&self, ui: &mut egui::Ui, params: &mut TabParams<'_>) {
        match self.kind {
            TabKind::Settings => {
                settings_panel::SettingsPanel::render(
                    ui,
                    params.slider_value,
                    params.selected_option,
                    params.is_running,
                    params.colors,
                    params.reactive_logger_state,
                    params.volume_label,
                );
            }

            TabKind::ReactiveLogger => {
                // Create a ReactiveEventLogger using the state directly and pass the colors
                let reactive_logger = ReactiveEventLogger::with_colors(params.reactive_logger_state, params.colors);
                reactive_logger.show(ui);
            }

            TabKind::Control => {
                control_panel::ControlPanel::render(
                    ui, 
                    params.selected_option,
                    params.is_running,
                    params.is_formatted,
                    params.colors,
                    params.reactive_logger_state,
                    params.volume_label,
                );
            }
            TabKind::About => {
                crate::ui::about_panel::AboutPanel::render(ui);
            }

            TabKind::Taffy => {
                TaffyPanel::render(ui);
            }
            TabKind::SD => {
                sd_panel::SDPanel::render(ui, params.selected_option, params.volume_label, params.is_formatted);
            }
        }
    }
}

/// Tab viewer for DockArea
/// 
/// Construct the view for the tabs of the application.
/// 
struct TabViewer<'a> {
    reactive_logger_state: &'a Dynamic<ReactiveEventLoggerState>,
    slider_value     : &'a mut f32,
    selected_option  : &'a mut usize,
    is_running       : &'a mut bool,
    is_formatted     : &'a mut bool,
    colors           : &'a Dynamic<LogColors>,
    volume_label     : &'a mut String,
}

impl egui_dock::TabViewer for TabViewer<'_> {
    type Tab = Tab;

    fn title(&mut self, tab: &mut Self::Tab) -> egui::WidgetText {
        tab.title().into()
    }

    fn ui(&mut self, ui: &mut egui::Ui, tab: &mut Self::Tab) {
        let mut params = TabParams {
            reactive_logger_state: self.reactive_logger_state,
            slider_value: self.slider_value,
            selected_option: self.selected_option,
            is_running: self.is_running,
            is_formatted: self.is_formatted,
            colors: self.colors,
            volume_label: self.volume_label,
        };
        tab.content(ui, &mut params);
    }
}

/// Main application
pub struct MyApp {
    dock_state       : DockState<Tab>,
    reactive_logger_state: Dynamic<ReactiveEventLoggerState>,
    slider_value     : f32,
    selected_option  : usize,
    is_running       : bool,
    is_formatted     : bool,
    colors           : Dynamic<LogColors>,
    banner           : platform::banner::Banner,
    details          : platform::details::Details,
    volume_label     : String,
}

/// Drop implementation for MyApp
/// 
/// Drop implementation is used to save application data when 
/// the application is closed.
impl Drop for MyApp {
    fn drop(&mut self) {
        // Save colors when app is dropped
        let colors = self.colors.get();
        colors.save();
    }
}
impl eframe::App for MyApp {
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
            let logger = ReactiveEventLogger::with_colors(&self.reactive_logger_state, &self.colors);
            
            // Display system details first
            let details_text = self.details.format_os();
            logger.log_info(&details_text);
            
            // Then display banner (so it appears above the details in the log)
            logger.log_info(&self.banner.message);
        }
        
        // Check if we need to update the UI state from the thread
        DockArea::new(&mut self.dock_state)
            .show_add_buttons(true)
            .style(Style::from_egui(ctx.style().as_ref()))
            .show(
                ctx,
                &mut TabViewer {
                    reactive_logger_state: &self.reactive_logger_state, 
                    slider_value: &mut self.slider_value,
                    selected_option: &mut self.selected_option,
                    is_running: &mut self.is_running,
                    is_formatted: &mut self.is_formatted,
                    colors: &self.colors,
                    volume_label: &mut self.volume_label,
                },
            );
        
        // Request continuous repaints if formatting is in progress
        if self.is_running {
            ctx.request_repaint_after(std::time::Duration::from_millis(100));
        }
    }
}

/// Main function
/// 
/// The main function is the entry point for the application.
/// 
/// It is responsible for creating the application window and running
/// the application. Also note that it handles loading the colors from
/// the config file and saving them when the application is closed.
/// 
fn main() -> Result<(), eframe::Error> {
    let native_options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_titlebar_buttons_shown(true)
            .with_inner_size([gui::VIEWPORT_X, gui::VIEWPORT_Y])
            .with_min_inner_size([gui::VIEWPORT_X, gui::VIEWPORT_Y])
            .with_resizable(true),
        ..Default::default()
    };

    eframe::run_native(
        "DiskForge SD Utility Platform",
        native_options,
        Box::new(|cc| {
            egui_extras::install_image_loaders(&cc.egui_ctx);
        
            // This application was originally using a different LogColors structure
            // Let's instead create a new LogColors from reactive_event_logger that matches our needs
            
            // Create a default set of colors from the egui_lens crate
            let logger_colors = LogColors::default();
            
            // We'll use default colors since we can't access the old format easily
            
            // Create reactive containers for state
            let reactive_logger_state = Dynamic::new(ReactiveEventLoggerState::new());
            let colors_dynamic = Dynamic::new(logger_colors);
            
            // Create dock state with Control and Taffy in the left panel
            let mut dock_state = DockState::new(vec![
                Tab::new(TabKind::Control, SurfaceIndex::main(), NodeIndex(0)),
                Tab::new(TabKind::Taffy, SurfaceIndex::main(), NodeIndex(1)),
            ]);
            
            // First split into left panel (Control) and right panel (SD + About)
            let [left, right] = dock_state.main_surface_mut().split_right(
                NodeIndex::root(),
                0.35, // Left panel takes 35% of width
                vec![
                    Tab::new(TabKind::SD, SurfaceIndex::main(), NodeIndex(2)),
                    Tab::new(TabKind::About, SurfaceIndex::main(), NodeIndex(3)),
                ],
            );
            
            // Split left panel into top (Control+Taffy) and bottom (Settings)
            let [_, _] = dock_state.main_surface_mut().split_below(
                left,
                0.35, // Control panel at top takes 35% of height
                vec![Tab::new(TabKind::Settings, SurfaceIndex::main(), NodeIndex(4))],
            );
            
            // Split right panel into top (SD+About) and bottom (ReactiveLogger only)
            let [_, _] = dock_state.main_surface_mut().split_below(
                right,
                0.5, // SD panel takes 50% of height
                vec![Tab::new(TabKind::ReactiveLogger, SurfaceIndex::main(), NodeIndex(5))],
            );
            
            // Create banner and details instances
            let mut banner = platform::banner::Banner::new();
            let mut details = platform::details::Details::new();
            
            // Format banner and get OS information
            banner.format();
            details.get_os();
            
            // Log system details and banner at startup
            {
                // Create a temporary logger for initial output
                let logger = ReactiveEventLogger::with_colors(&reactive_logger_state, &colors_dynamic);
                
                // Display system details first
                let details_text = details.format_os();
                logger.log_info(&details_text);
                
                // Then display banner (so it appears above the details)
                logger.log_info(&banner.message);
            }
            
            // Create app with loaded colors and initialized dock state
            Ok(Box::new(MyApp {
                dock_state,
                reactive_logger_state,
                slider_value    : 1.0,
                selected_option : 0,
                is_running      : false,
                is_formatted    : false,
                colors           : colors_dynamic,
                banner,
                details,
                volume_label: String::from("DISKFORGE"),
            }))
        })
    )
}