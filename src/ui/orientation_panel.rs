use crate::{DemoLensApp, project::constants::{LOG_TYPE_ROTATION, LOG_TYPE_MIRROR, LOG_TYPE_CENTER_OFFSET, LOG_TYPE_DESIGN_OFFSET}};
use egui_lens::{ReactiveEventLogger, ReactiveEventLoggerState, LogColors};
use egui_mobius_reactive::Dynamic;
use crate::display::VectorOffset;

pub fn show_orientation_panel<'a>(    
    ui: &mut egui::Ui,
    app: &'a mut DemoLensApp,
    logger_state: &'a Dynamic<ReactiveEventLoggerState>,
    log_colors: &'a Dynamic<LogColors>,
) {
    // Orientation panel is now empty - all controls moved to main toolbar
}

/// Export layers from quadrant view to PNG files
pub fn export_quadrant_layers_to_png(app: &mut DemoLensApp, logger: &ReactiveEventLogger) {
    if !app.display_manager.quadrant_view_enabled {
        logger.log_error("Quadrant view must be enabled to export layers as PNG");
        return;
    }
    
    // Use the downloads directory for exports
    let export_dir = if let Some(downloads_dir) = dirs::download_dir() {
        downloads_dir.join("KiForge_Layer_Exports")
    } else {
        std::env::current_dir().unwrap_or_default().join("layer_exports")
    };
    
    // Standard resolution for PCB layer exports (300 DPI equivalent)
    let width = 2048;
    let height = 2048;
    
    logger.log_info(&format!("Starting PNG export to: {}", export_dir.display()));
    
    match crate::export::PngExporter::export_quadrant_layers(app, &export_dir, width, height) {
        Ok(exported_files) => {
            logger.log_info(&format!("Successfully exported {} layer files:", exported_files.len()));
            for file_path in exported_files {
                if let Some(filename) = file_path.file_name() {
                    logger.log_info(&format!("  • {}", filename.to_string_lossy()));
                }
            }
            logger.log_info(&format!("Export directory: {}", export_dir.display()));
        },
        Err(error) => {
            logger.log_error(&format!("PNG export failed: {}", error));
        }
    }
}