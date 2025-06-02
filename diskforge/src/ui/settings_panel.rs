use eframe::egui;
use egui_lens::{LogColors, ReactiveEventLogger, ReactiveEventLoggerState};
use egui_mobius_widgets::{StatefulButton, StyledButton};
use egui_mobius_reactive::{Dynamic, ReactiveWidgetRef};


pub struct SettingsPanel<'a> {
    slider_value: &'a mut f32,
    selected_option: &'a mut usize,
    is_running: &'a mut bool,
    colors: &'a Dynamic<LogColors>,
    reactive_logger_state: &'a Dynamic<ReactiveEventLoggerState>,
    volume_label: &'a mut String,
}

impl<'a> SettingsPanel<'a> {
    pub fn new(
        slider_value: &'a mut f32,
        selected_option: &'a mut usize,
        is_running: &'a mut bool,
        colors: &'a Dynamic<LogColors>,
        reactive_logger_state: &'a Dynamic<ReactiveEventLoggerState>,
        volume_label: &'a mut String,
    ) -> Self {
        Self {
            slider_value,
            selected_option,
            is_running,
            colors,
            reactive_logger_state,
            volume_label,
        }
    }

    pub fn render(
        ui: &mut egui::Ui,
        slider_value: &'a mut f32,
        selected_option: &'a mut usize,
        is_running: &'a mut bool,
        colors: &'a Dynamic<LogColors>,
        reactive_logger_state: &'a Dynamic<ReactiveEventLoggerState>,
        volume_label: &'a mut String,
    ) {
        let mut panel = Self::new(
            slider_value, 
            selected_option, 
            is_running, 
            colors, 
            reactive_logger_state,
            volume_label
        );
        panel.ui(ui);
    }

    pub fn ui(&mut self, ui: &mut egui::Ui) {
        ui.vertical(|ui| {
            ui.heading("Settings");
            ui.add_space(8.0);

            // Color settings
            ui.push_id("log_colors_section", |ui| {
                // Check if the log colors section should be expanded from ReactiveLogger
                let should_expand = ui.ctx().memory(|mem| {
                    mem.data.get_temp(egui::Id::new("settings_log_colors_expanded"))
                        .unwrap_or(false)
                });
                
                // Clear the flag after reading it
                if should_expand {
                    ui.ctx().memory_mut(|mem| {
                        mem.data.remove::<bool>(egui::Id::new("settings_log_colors_expanded"));
                    });
                }
                
                let header = egui::CollapsingHeader::new("🎨 Log Colors")
                    .default_open(should_expand);
                
                header.show(ui, |ui| {
                    // Get a copy of the colors to modify
                    let mut colors = self.colors.get().clone();
                    let mut changed = false;

                    ui.horizontal(|ui| {
                        ui.label("Info:");
                        changed |= ui.color_edit_button_srgba(&mut colors.info).changed();
                    });

                    ui.horizontal(|ui| {
                        ui.label("Warning:");
                        changed |= ui.color_edit_button_srgba(&mut colors.warning).changed();
                    });

                    ui.horizontal(|ui| {
                        ui.label("Error:");
                        changed |= ui.color_edit_button_srgba(&mut colors.error).changed();
                    });

                    ui.horizontal(|ui| {
                        ui.label("Debug:");
                        changed |= ui.color_edit_button_srgba(&mut colors.debug).changed();
                    });

                    ui.horizontal(|ui| {
                        ui.label("Timestamp:");
                        changed |= ui.color_edit_button_srgba(&mut colors.timestamp).changed();
                    });

                    ui.horizontal(|ui| {
                        ui.label("System:");
                        changed |= ui.color_edit_button_srgba(&mut colors.system).changed();
                    });

                    ui.horizontal(|ui| {
                        ui.label("User Action:");
                        changed |= ui.color_edit_button_srgba(&mut colors.user_action).changed();
                    });
                    
                    ui.horizontal(|ui| {
                        ui.label("Custom 1:");
                        
                        // Get custom color from the HashMap instead of accessing a direct field
                        if let Some(custom_color) = colors.custom_colors.get_mut("custom1") {
                            changed |= ui.color_edit_button_srgba(&mut custom_color.level_color).changed();
                        } else {
                            // If the custom color doesn't exist, create it with default values
                            let mut new_color = egui::Color32::from_rgb(255, 200, 200);
                            if ui.color_edit_button_srgba(&mut new_color).changed() {
                                colors.set_custom_color("custom1", new_color);
                                changed = true;
                            }
                        }
                    });

                    if changed {
                        // Update the shared dynamic colors
                        self.colors.set(colors);
                        
                        // Request a repaint
                        ui.ctx().request_repaint();
                    }
                });
            });
            ui.add_space(8.0);

            // System Info button has been moved to the Reactive Logger panel
            ui.add_space(16.0);

            // Clear Logger button
            ui.horizontal(|ui| {
                if ui.button("Clear Logger").clicked() {
                    // Create a reactive logger and clear its logs
                    let reactive_logger_state = self.reactive_logger_state;
                    if let Some(state_arc) = ReactiveWidgetRef::from_dynamic(reactive_logger_state).weak_ref.upgrade() {
                        let mut state = state_arc.lock().unwrap();
                        state.clear_logs();
                    }
                }
            });
            ui.add_space(16.0);

            // Combo box with options
            ui.label("Select an option:");
            ui.horizontal(|ui| {
                for (idx, label) in ["Fat32", "ExFat"].iter().enumerate() {
                    if ui.selectable_label(*self.selected_option == idx, *label).clicked() {
                        *self.selected_option = idx;
                        
                        // Create a logger with colors for better formatting
                        let reactive_logger = ReactiveEventLogger::with_colors(self.reactive_logger_state, self.colors);
                        
                        // Log the selection to the reactive logger
                        match idx {
                            0 => reactive_logger.log_info("Selected Fat32 format (max 4GB file size)"),
                            1 => reactive_logger.log_info("Selected ExFat format (max 16EB theoretical file size limit)"),
                            _ => {}
                        }
                    }
                }
            });
            ui.add_space(16.0);
            
            // Volume Label input field with temp buffer
            ui.vertical(|ui| {
                ui.label("Volume Label:");
                
                // Use a local buffer that persists between frames using egui's memory
                let mut temp_label = ui.memory_mut(|mem| 
                    mem.data.get_persisted_mut_or_default::<String>(
                        egui::Id::new("volume_label_buffer")
                    ).clone()
                );
                
                // Initialize with current value if empty
                if temp_label.is_empty() {
                    temp_label = self.volume_label.clone();
                }
                
                // Handle text input with submit action
                let response = ui.horizontal(|ui| {
                    let edit_response = ui.text_edit_singleline(&mut temp_label);
                    
                    // Only update on Enter key press or when focus is lost after changes
                    let enter_pressed = ui.input(|i| i.key_pressed(egui::Key::Enter));
                    let submitted = enter_pressed || 
                                   (edit_response.lost_focus() && edit_response.changed());
                    
                    if submitted && temp_label != *self.volume_label {
                        // Create a logger with colors for better formatting
                        let reactive_logger = ReactiveEventLogger::with_colors(self.reactive_logger_state, self.colors);
                        
                        // Format the temp label according to rules
                        temp_label = temp_label.to_uppercase();
                        
                        // Remove invalid characters for FAT32 volume labels
                        temp_label = temp_label
                            .chars()
                            .filter(|c| c.is_ascii_alphanumeric() || *c == '_' || *c == '-')
                            .collect();
                        
                        // Limit to 11 characters for FAT32 compatibility
                        if temp_label.len() > 11 {
                            temp_label = temp_label[..11].to_string();
                            reactive_logger.log_warning("Volume label truncated to 11 characters for FAT32 compatibility");
                        }
                        
                        // Apply changes to the main volume label
                        *self.volume_label = temp_label.clone();
                        
                        // Log the volume label change
                        reactive_logger.log_info(&format!("Volume label changed to: {}", self.volume_label));
                    }
                    
                    // Store the current value back in memory
                    ui.memory_mut(|mem| {
                        mem.data.insert_persisted(
                            egui::Id::new("volume_label_buffer"),
                            temp_label.clone()
                        );
                    });
                    
                    edit_response
                }).inner;
                
                // If this field has focus, notify user to press Enter
                if response.has_focus() {
                    ui.label(
                        egui::RichText::new("Press Enter to apply changes")
                        .size(12.0)
                        .strong()
                        .color(egui::Color32::from_rgb(220, 220, 120))
                    );
                }
                
                ui.label(
                    egui::RichText::new("Volume label can be up to 11 characters (A-Z, 0-9, _, -)")
                    .size(12.0)
                    .weak()
                    .color(ui.visuals().weak_text_color())
                );
            });
            ui.add_space(16.0);

            // Custom event button with styled appearance
            let event_button = StyledButton::new("Wipe (dd)")
                .hover_color(egui::Color32::from_rgb(100, 200, 255))
                .normal_color(egui::Color32::from_rgb(150, 150, 255))
                .rounding(5.0)
                .margin(egui::Vec2::new(4.0, 2.0))
                .min_size(egui::vec2(120.0, 24.0));

            if event_button.show(ui).clicked() {
                // Create a logger with colors for better formatting
                let reactive_logger = ReactiveEventLogger::with_colors(self.reactive_logger_state, self.colors);
                
                // Log the wipe operation
                let msg = format!("Wipe (dd) operation started with slider={:.1}", self.slider_value);
                reactive_logger.log_warning(&msg);
                reactive_logger.log_warning("Starting wiping the disk with dd ...");
            }
            ui.add_space(8.0);



            // Format button using StatefulButton
            let mut stateful_button = StatefulButton::new()
                .margin(egui::Vec2::new(4.0, 2.0))
                .rounding(5.0)
                .min_size(egui::vec2(120.0, 24.0))
                .run_color(egui::Color32::from_rgb(100, 200, 100))
                .stop_color(egui::Color32::from_rgb(200, 100, 100));
            
            stateful_button.set_started(*self.is_running);

            if stateful_button.show(ui).clicked() {
                *self.is_running = !*self.is_running;
                let format_type = if *self.selected_option == 0 { "Fat32" } else { "ExFat" };
                
                // Create a logger with colors for better formatting
                let reactive_logger = ReactiveEventLogger::with_colors(self.reactive_logger_state, self.colors);
                
                // Log the operation
                let msg = format!("Format {} operation {}", 
                    format_type,
                    if *self.is_running { "started" } else { "stopped" });
                reactive_logger.log_info(&msg);
                
                if *self.is_running {
                    reactive_logger.log_info("Starting the format process");
                } else {
                    reactive_logger.log_error("Stopping the format process!");
                }
            }
        });
    }
}