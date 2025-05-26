use crate::{DemoLensApp, constants::LOG_TYPE_DRC};
use egui_lens::{ReactiveEventLogger, ReactiveEventLoggerState, LogColors};
use egui_mobius_reactive::Dynamic;

pub fn show(ui: &mut egui::Ui, app: &mut DemoLensApp, logger_state: &Dynamic<ReactiveEventLoggerState>, log_colors: &Dynamic<LogColors>) {
    let logger = ReactiveEventLogger::with_colors(logger_state, log_colors);
    // Design Rule Check section
    ui.horizontal(|ui| {
        ui.heading("Design Rule Check");
        
        // Add some spacing to push the button to the right
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui.button("🔍 Run DRC").clicked() {
                // Check if a ruleset is loaded
                if let Some(ref ruleset) = app.current_drc_ruleset {
                    // Simulate DRC process with INFO messages
                    logger.log_info("Starting Design Rule Check");
                    logger.log_info(&format!("Using {} ruleset", ruleset));
                    logger.log_info("Analyzing Gerber files");
                    logger.log_info("Checking trace widths");
                    logger.log_info("Checking via sizes");
                    logger.log_info("Checking spacing rules");
                    logger.log_info("Checking drill sizes");
                    logger.log_info("Issues found: None");
                    logger.log_info("DRC analysis completed successfully");
                } else {
                    logger.log_warning("Cannot run DRC: No ruleset loaded");
                    logger.log_info("Please select a PCB manufacturer ruleset first");
                }
            }
        });
    });
    ui.add_space(4.0);
    
    egui::CollapsingHeader::new("PCB Manufacturer Rules")
        .default_open(false)
        .show(ui, |ui| {
            ui.add_space(4.0);
            
            // Current ruleset display
            if let Some(ref ruleset) = app.current_drc_ruleset {
                ui.horizontal(|ui| {
                    ui.label("Current ruleset:");
                    ui.label(egui::RichText::new(ruleset).strong().color(egui::Color32::from_rgb(46, 204, 113)));
                });
                ui.add_space(4.0);
            } else {
                ui.label(egui::RichText::new("No DRC ruleset loaded").color(egui::Color32::from_rgb(231, 76, 60)));
                ui.add_space(4.0);
            }
            
            // PCB Manufacturer buttons
            ui.vertical(|ui| {
                if ui.button("🏭 JLC PCB Rules").clicked() {
                    app.current_drc_ruleset = Some("JLC PCB".to_string());
                    logger.log_custom(
                        LOG_TYPE_DRC,
                        "Loaded JLC PCB Design Rule Check ruleset"
                    );
                }
                
                if ui.button("🏭 PCB WAY Rules").clicked() {
                    app.current_drc_ruleset = Some("PCB WAY".to_string());
                    logger.log_custom(
                        LOG_TYPE_DRC,
                        "Loaded PCB WAY Design Rule Check ruleset"
                    );
                }
                
                if ui.button("🏭 Advanced Circuits Rules").clicked() {
                    app.current_drc_ruleset = Some("Advanced Circuits".to_string());
                    logger.log_custom(
                        LOG_TYPE_DRC,
                        "Loaded Advanced Circuits Design Rule Check ruleset"
                    );
                }
                
                ui.add_space(4.0);
                
                // Clear ruleset button
                if app.current_drc_ruleset.is_some() {
                    if ui.button("🗑 Clear Ruleset").clicked() {
                        if let Some(ref ruleset) = app.current_drc_ruleset {
                            logger.log_custom(
                                LOG_TYPE_DRC,
                                &format!("Cleared {} Design Rule Check ruleset", ruleset)
                            );
                        }
                        app.current_drc_ruleset = None;
                    }
                }
            });
        });
}