use crate::CopperForgeApp;
use crate::event_logger::{ReactiveEventLogger, ReactiveEventLoggerState, LogColors};
use egui_mobius_reactive::Dynamic;
use chrono_tz::Tz;
use chrono::Local;

pub fn show_settings_panel<'a>(
    ui: &mut egui::Ui,
    app: &'a mut CopperForgeApp,
    logger_state: &'a Dynamic<ReactiveEventLoggerState>,
    log_colors: &'a Dynamic<LogColors>,
) {
    let logger = ReactiveEventLogger::with_colors(logger_state, log_colors);

    ui.heading("Application Settings");
    ui.separator();
    
    // Units Section
    ui.group(|ui| {
        ui.label("Display Units");
        ui.horizontal(|ui| {
            ui.label("Global Units:");
            
            // Get current units from layer store
            let _current_unit = app.layer_store.units.display_unit;
            
            // Track if units changed
            let mut units_changed = false;
            let prev_units = app.global_units_mils;
            
            // Update legacy global_units_mils based on selection
            if ui.selectable_value(&mut app.global_units_mils, false, "Millimeters (mm)").clicked() {
                units_changed = true;
            }
            if ui.selectable_value(&mut app.global_units_mils, true, "Mils (1/1000 inch)").clicked() {
                units_changed = true;
            }
            
            // Additional units options (disabled for now)
            ui.add_enabled(false, egui::Button::new("Micrometers (µm)"));
            ui.add_enabled(false, egui::Button::new("Nanometers (nm)"));
            
            if units_changed || prev_units != app.global_units_mils {
                // Sync to ECS
                app.sync_units_to_ecs();
                
                let units_name = if app.global_units_mils { "mils" } else { "mm" };
                logger.log_info(&format!("Changed global units to {}", units_name));
            }
        });
        ui.label("Affects: Grid spacing, board dimensions, cursor position, zoom selection");
        ui.label("Internal precision: 1 nanometer (integer-based like KiCad)");
    });
    
    ui.add_space(20.0);
    
    // Timezone Section
    ui.group(|ui| {
        ui.label("Time & Localization");
        ui.horizontal(|ui| {
            ui.label("Timezone:");
            
            // Get current timezone name or use UTC as default
            let current_tz_name = app.user_timezone.as_ref()
                .map(|s| s.as_str())
                .unwrap_or("UTC");
            
            egui::ComboBox::from_id_salt("timezone_selector")
                .selected_text(current_tz_name)
                .width(300.0)
                .show_ui(ui, |ui| {
                    // Common timezones first
                    ui.label("Common Timezones:");
                    for tz_name in &[
                        "UTC",
                        "US/Eastern", 
                        "US/Central",
                        "US/Mountain", 
                        "US/Pacific",
                        "Europe/London",
                        "Europe/Paris",
                        "Europe/Berlin",
                        "Asia/Tokyo",
                        "Asia/Shanghai",
                        "Australia/Sydney",
                    ] {
                        if ui.selectable_value(&mut app.user_timezone, Some(tz_name.to_string()), *tz_name).clicked() {
                            logger.log_info(&format!("Changed timezone to {}", tz_name));
                        }
                    }
                    
                    ui.separator();
                    ui.label("All Timezones:");
                    
                    // All timezones
                    for tz in chrono_tz::TZ_VARIANTS {
                        let tz_name = tz.name();
                        if ui.selectable_value(&mut app.user_timezone, Some(tz_name.to_string()), tz_name).clicked() {
                            logger.log_info(&format!("Changed timezone to {}", tz_name));
                        }
                    }
                });
        });
        
        ui.add_space(10.0);
        
        // Clock format selection
        ui.horizontal(|ui| {
            ui.label("Clock Format:");
            let prev_format = app.use_24_hour_clock;
            ui.selectable_value(&mut app.use_24_hour_clock, true, "24-hour (13:30:45)");
            ui.selectable_value(&mut app.use_24_hour_clock, false, "12-hour (1:30:45 PM)");
            
            if prev_format != app.use_24_hour_clock {
                let format_name = if app.use_24_hour_clock { "24-hour" } else { "12-hour" };
                logger.log_info(&format!("Changed clock format to {}", format_name));
            }
        });
        
        // Show current time in selected timezone with chosen format
        let time_format = if app.use_24_hour_clock { "%Y-%m-%d %H:%M:%S %Z" } else { "%Y-%m-%d %I:%M:%S %p %Z" };
        
        if let Some(tz_name) = &app.user_timezone {
            if let Ok(tz) = tz_name.parse::<Tz>() {
                let now = Local::now().with_timezone(&tz);
                ui.label(format!("Current time: {}", now.format(time_format)));
            }
        } else {
            let now = Local::now();
            ui.label(format!("Current time: {}", now.format(if app.use_24_hour_clock { "%Y-%m-%d %H:%M:%S" } else { "%Y-%m-%d %I:%M:%S %p" })));
        }
    });
    
    ui.add_space(20.0);

    // Project Directories Section
    ui.group(|ui| {
        ui.label("Project Directories");

        // Preferred projects directory
        ui.horizontal(|ui| {
            ui.label("Preferred PCB Projects Directory:");
        });

        let current_dir_text = if let Some(ref dir) = app.project_manager.config.preferred_projects_directory {
            dir.display().to_string()
        } else {
            "Not set (will use home directory)".to_string()
        };

        ui.label(egui::RichText::new(&current_dir_text).small().monospace());

        ui.horizontal(|ui| {
            if ui.button("📂 Browse...").clicked() {
                use std::mem;

                // Set initial directory to current preference if available
                let dialog = mem::replace(&mut app.projects_directory_dialog, egui_file_dialog::FileDialog::new());
                app.projects_directory_dialog = if let Some(ref current_dir) = app.project_manager.config.preferred_projects_directory {
                    dialog.initial_directory(current_dir.clone())
                } else {
                    dialog
                };
                app.projects_directory_dialog.pick_directory();
            }

            if app.project_manager.config.preferred_projects_directory.is_some() {
                if ui.button("Clear").clicked() {
                    app.project_manager.config.preferred_projects_directory = None;
                    app.save_settings();
                    logger.log_info("Cleared preferred projects directory");
                }
            }
        });

        // Handle directory selection
        let picked_path = app.projects_directory_dialog.update(ui.ctx()).picked().map(|p| p.to_path_buf());
        if let Some(path) = picked_path {
            // Only process if this is a NEW directory selection (not already processed)
            let should_process = app.last_picked_projects_directory.as_ref() != Some(&path);

            if should_process {
                app.last_picked_projects_directory = Some(path.clone());
                let path_display = path.display().to_string();
                app.project_manager.config.preferred_projects_directory = Some(path);
                app.save_settings();
                logger.log_info(&format!("Set preferred projects directory to: {}", path_display));
            }
        }

        ui.label(egui::RichText::new("💡 This directory will be used as the starting location when browsing for KiCad projects").small().italics());
    });

    ui.add_space(20.0);

    // Library Configuration Section
    ui.group(|ui| {
        ui.label("KiCad Library Configuration");
        ui.label(egui::RichText::new("💡 These settings apply when creating new KiCad projects").small().italics());
        ui.separator();

        // Initialize project manager state if needed to access library settings
        if app.project_manager_state.is_none() {
            let mut state = crate::project_manager::ProjectManagerState::with_config(&app.project_manager.config);
            let db_path = app.config_path.join("projects.db");
            let _ = state.initialize_database(&db_path);
            app.project_manager_state = Some(state);
        }

        if let Some(ref mut manager_state) = app.project_manager_state {
            ui.checkbox(&mut manager_state.include_kiverse, "Include KiVerse Symbol Library");
            ui.checkbox(&mut manager_state.include_atlantix_resistors, "Include Atlantix-EDA Resistor Library");

            ui.add_space(5.0);

            ui.label("KiVerse Library Path:");
            ui.horizontal(|ui| {
                let kiverse_text = manager_state.kiverse_path.display().to_string();
                ui.label(egui::RichText::new(&kiverse_text).small().monospace());
            });
            ui.label(egui::RichText::new("Default: ~/kiverse").small().italics());
        }
    });

    ui.add_space(20.0);

    // Language Section (placeholder for future)
    ui.group(|ui| {
        ui.label("Language");
        ui.horizontal(|ui| {
            ui.label("Interface Language:");

            egui::ComboBox::from_id_salt("language_selector")
                .selected_text("English")
                .show_ui(ui, |ui| {
                    let _ = ui.selectable_label(true, "English");
                    ui.add_enabled(false, egui::Button::new("Français (coming soon)"));
                    ui.add_enabled(false, egui::Button::new("Deutsch (coming soon)"));
                    ui.add_enabled(false, egui::Button::new("中文 (coming soon)"));
                    ui.add_enabled(false, egui::Button::new("日本語 (coming soon)"));
                });
        });
    });

}