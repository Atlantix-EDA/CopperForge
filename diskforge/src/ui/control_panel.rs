use eframe::egui;
use std::sync::Arc;
use std::thread;
use std::time::Duration;
use egui_lens::{LogColors, ReactiveEventLogger, ReactiveEventLoggerState};
use egui_mobius_widgets::StatefulButton;
use egui_mobius_reactive::Dynamic;
use once_cell;

#[allow(dead_code)]
pub struct ControlPanel<'a> {
    selected_option: &'a mut usize,
    is_running: &'a mut bool,
    is_formatted: &'a mut bool,
    colors: &'a Dynamic<LogColors>,
    reactive_logger_state: &'a Dynamic<ReactiveEventLoggerState>,
    volume_label: &'a mut String,
}

impl<'a> ControlPanel<'a> {
    pub fn new(
        selected_option: &'a mut usize,
        is_running: &'a mut bool,
        is_formatted: &'a mut bool,
        colors: &'a Dynamic<LogColors>,
        reactive_logger_state: &'a Dynamic<ReactiveEventLoggerState>,
        volume_label: &'a mut String,
    ) -> Self {
        Self { 
            selected_option,
            is_running,
            is_formatted,
            colors,
            reactive_logger_state,
            volume_label,
        }
    }

    pub fn render(
        ui: &mut egui::Ui, 
        selected_option: &'a mut usize,
        is_running: &'a mut bool,
        is_formatted: &'a mut bool,
        colors: &'a Dynamic<LogColors>,
        reactive_logger_state: &'a Dynamic<ReactiveEventLoggerState>,
        volume_label: &'a mut String,
    ) {
        let mut panel = Self::new(
            selected_option,
            is_running,
            is_formatted,
            colors,
            reactive_logger_state,
            volume_label,
        );
        panel.ui(ui);
    }

    pub fn ui(&mut self, ui: &mut egui::Ui) {
        ui.vertical(|ui| {
            ui.heading("Control Panel");
            ui.add_space(8.0);

            // Version label
            ui.label("Version: 0.1.0");
            ui.add_space(16.0);
            
            // Show current format selection
            let format_type = if *self.selected_option == 0 { "Fat32" } else { "ExFat" };
            ui.label(format!("Selected Format: {}", format_type));
            ui.add_space(8.0);
            
            // Show volume label
            ui.label(format!("Volume Label: {}", self.volume_label));
            ui.add_space(16.0);
            
            // Format button using StatefulButton
            let mut stateful_button = StatefulButton::new()
                .margin(egui::Vec2::new(4.0, 2.0))
                .rounding(5.0)
                .min_size(egui::vec2(120.0, 24.0))
                .run_color(egui::Color32::from_rgb(100, 200, 100))
                .stop_color(egui::Color32::from_rgb(200, 100, 100));
            
            stateful_button.set_started(*self.is_running);

            // Set up a static reference to hold our formatting flag
            static FORMAT_FLAG: once_cell::sync::Lazy<Arc<std::sync::atomic::AtomicBool>> = 
                once_cell::sync::Lazy::new(|| Arc::new(std::sync::atomic::AtomicBool::new(false)));
                
            // Update is_formatted based on the shared flag if running
            if *self.is_running && FORMAT_FLAG.load(std::sync::atomic::Ordering::SeqCst) {
                *self.is_formatted = true;
            }
                
            if stateful_button.show(ui).clicked() {
                *self.is_running = !*self.is_running;
                let format_type = if *self.selected_option == 0 { "Fat32" } else { "ExFat" };
                
                // Create a logger with colors for better formatting
                let reactive_logger = ReactiveEventLogger::with_colors(self.reactive_logger_state, self.colors);
                
                // Log the operation to the reactive logger
                let msg = format!("Format {} operation {}", 
                    format_type,
                    if *self.is_running { "started" } else { "stopped" });
                reactive_logger.log_info(&msg);
                
                if *self.is_running {
                    // Reset the shared flag when starting
                    FORMAT_FLAG.store(false, std::sync::atomic::Ordering::SeqCst);
                    reactive_logger.log_info(&format!("Starting the format process for {} with volume label: {}", format_type, self.volume_label));
                    
                    // Log all steps at once instead of in a thread
                    reactive_logger.log_info("[INFO] Starting SD card provisioning process");
                    
                    // Use a default device for demonstration
                    let device = "/dev/mmcblk0";
                    reactive_logger.log_info(&format!("[INFO] Device {} selected", device));
                    
                    // We'll simulate the formatting process with a background thread that just updates the UI state
                    let format_type = if *self.selected_option == 0 { "FAT32" } else { "ExFat" };
                    let ctx = ui.ctx().clone();
                    
                    // We use the static FORMAT_FLAG to communicate between threads
                    let reactive_logger_state_clone = self.reactive_logger_state.clone();
                    
                    // Create a new dynamic for the thread
                    let colors_clone = Dynamic::new(self.colors.get().clone());
                    
                    let format_flag = FORMAT_FLAG.clone();
                    
                    // Set the formatted flag to false initially
                    *self.is_formatted = false;
                    
                    // Start a background thread just to simulate the delay
                    thread::spawn(move || {
                        // Simulate all the formatting steps with delays
                        thread::sleep(Duration::from_millis(500));
                        
                        // Create a new logger for the thread
                        let logger = ReactiveEventLogger::with_colors(&reactive_logger_state_clone, &colors_clone);
                        
                        // First step
                        logger.log_info("[INFO] Wiping first 1MB (secure erase)");
                        thread::sleep(Duration::from_millis(500));
                        
                        // Second step
                        logger.log_info("[INFO] Partition table written (MBR)");
                        thread::sleep(Duration::from_millis(500));
                        
                        // Third step
                        logger.log_info(&format!("[INFO] {} filesystem created", format_type));
                        thread::sleep(Duration::from_millis(500));
                        
                        // Fourth step
                        logger.log_info("[INFO] Directory tree /project initialized");
                        thread::sleep(Duration::from_millis(500));
                        
                        // Fifth step
                        logger.log_info("[INFO] Pedigree metadata written");
                        thread::sleep(Duration::from_millis(500));
                        
                        // Final step
                        logger.log_info("[SUCCESS] SD card provisioning complete");
                        
                        // Update the format flag using thread-safe AtomicBool
                        format_flag.store(true, std::sync::atomic::Ordering::SeqCst);
                        
                        // Log that the indicator was updated
                        logger.log_info("SD Card visual indicator updated - formatting complete");
                        
                        // Request a repaint to make the UI update
                        ctx.request_repaint();
                    });
                } else {
                    reactive_logger.log_error("Stopping the format process!");
                }
            }
        });
    }
}