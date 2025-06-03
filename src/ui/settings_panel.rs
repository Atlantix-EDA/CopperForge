use crate::DemoLensApp;
use egui_lens::{ReactiveEventLogger, ReactiveEventLoggerState, LogColors};
use egui_mobius_reactive::Dynamic;
use chrono_tz::Tz;
use chrono::Local;

pub fn show_settings_panel<'a>(
    ui: &mut egui::Ui,
    app: &'a mut DemoLensApp,
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
            let prev_units = app.global_units_mils;
            ui.selectable_value(&mut app.global_units_mils, false, "Millimeters (mm)");
            ui.selectable_value(&mut app.global_units_mils, true, "Mils (1/1000 inch)");
            
            if prev_units != app.global_units_mils {
                let units_name = if app.global_units_mils { "mils" } else { "mm" };
                logger.log_info(&format!("Changed global units to {}", units_name));
            }
        });
        ui.label("Affects: Grid spacing, board dimensions, cursor position, zoom selection");
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
        
        // Show current time in selected timezone
        if let Some(tz_name) = &app.user_timezone {
            if let Ok(tz) = tz_name.parse::<Tz>() {
                let now = Local::now().with_timezone(&tz);
                ui.label(format!("Current time: {}", now.format("%Y-%m-%d %H:%M:%S %Z")));
            }
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
                    ui.selectable_label(true, "English");
                    ui.add_enabled(false, egui::SelectableLabel::new(false, "Français (coming soon)"));
                    ui.add_enabled(false, egui::SelectableLabel::new(false, "Deutsch (coming soon)"));
                    ui.add_enabled(false, egui::SelectableLabel::new(false, "中文 (coming soon)"));
                    ui.add_enabled(false, egui::SelectableLabel::new(false, "日本語 (coming soon)"));
                });
        });
    });
    
}