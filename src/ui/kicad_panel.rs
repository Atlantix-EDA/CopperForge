use crate::{DemoLensApp, kicad_api};
use egui_lens::{ReactiveEventLogger, ReactiveEventLoggerState, LogColors};
use egui_mobius_reactive::Dynamic;

pub fn show_kicad_panel(
    ui: &mut egui::Ui, 
    app: &mut DemoLensApp,
    logger_state: &Dynamic<ReactiveEventLoggerState>,
    log_colors: &Dynamic<LogColors>
) {
    let logger = ReactiveEventLogger::with_colors(logger_state, log_colors);
    
    ui.heading("KiCad Connection");
    ui.separator();
    
    // Connection status
    kicad_api::show_kicad_status(ui, &mut app.kicad_monitor, logger_state, log_colors);
    
    ui.separator();
    
    // Connection info
    if app.kicad_monitor.is_connected() {
        ui.label("Connection established");
        ui.small("Real-time sync active");
        
        // Options when connected
        ui.group(|ui| {
            ui.label("Sync Options:");
            
            ui.checkbox(&mut app.kicad_auto_refresh, "Auto-refresh on changes");
            
            ui.horizontal(|ui| {
                ui.label("Refresh interval:");
                ui.add(egui::Slider::new(&mut app.kicad_refresh_interval, 0.1..=5.0)
                    .suffix(" sec")
                    );
            });
            
            if ui.button("Export All Layers as Gerber").clicked() {
                logger.log_info("Requesting Gerber export from KiCad...");
                export_gerbers_from_kicad(app, &logger);
            }
        });
        
        // Layer sync status
        ui.separator();
        ui.label("Layer Sync Status:");
        
        egui::ScrollArea::vertical()
            .max_height(200.0)
            .show(ui, |ui| {
                for layer_type in crate::layers::LayerType::all() {
                    ui.horizontal(|ui| {
                        let synced = app.kicad_synced_layers.contains(&layer_type);
                        
                        ui.label(if synced { "✓" } else { "○" });
                        ui.label(layer_type.display_name());
                        
                        if synced {
                            ui.small("(synced)");
                        }
                    });
                }
            });
    } else {
        ui.label("Not connected to KiCad");
        ui.small("Click 'Connect' to establish connection");
        
        ui.separator();
        
        ui.label("Prerequisites:");
        ui.small("• KiCad 9.0 or later must be running");
        ui.small("• PCB Editor must be open");
        ui.small("• IPC API must be enabled in KiCad");
    }
}

fn export_gerbers_from_kicad(_app: &mut DemoLensApp, logger: &ReactiveEventLogger) {
    // TODO: When KiCad API is available, implement actual export
    // For now, this is a placeholder
    
    logger.log_warning("Gerber export from KiCad not yet implemented");
    logger.log_info("This will export each layer as Gerber data via IPC API");
    
    // Pseudo-code for future implementation:
    /*
    if let Some(connection) = app.kicad_monitor.get_connection() {
        let conn = connection.lock().unwrap();
        
        for layer in &app.active_pcb_layers {
            match conn.export_layer_as_gerber(layer) {
                Ok(gerber_data) => {
                    // Parse gerber data and add to app.layers
                    logger.log_info(&format!("Exported layer: {}", layer));
                }
                Err(e) => {
                    logger.log_error(&format!("Failed to export {}: {}", layer, e));
                }
            }
        }
    }
    */
}