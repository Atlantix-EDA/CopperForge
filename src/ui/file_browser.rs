use crate::DemoLensApp;
use egui_lens::{ReactiveEventLogger, ReactiveEventLoggerState, LogColors};
use egui_mobius_reactive::Dynamic;

#[derive(Debug, Clone, PartialEq)]
pub enum FileType {
    Gerber,
    KicadPcb,
    KicadSymbol,
}

pub fn show_file_menu(ui: &mut egui::Ui, app: &mut DemoLensApp, logger_state: &Dynamic<ReactiveEventLoggerState>, log_colors: &Dynamic<LogColors>) {
    if ui.button("Open Gerber Files").clicked() {
        let logger = ReactiveEventLogger::with_colors(logger_state, log_colors);
        logger.log_info("Opening Gerber file dialog");
        open_gerber_files(app, logger_state, log_colors);
    }
    
    if ui.button("Open KiCad PCB").clicked() {
        let logger = ReactiveEventLogger::with_colors(logger_state, log_colors);
        logger.log_info("Opening KiCad PCB file dialog");
        open_kicad_pcb(app, logger_state, log_colors);
    }
    
    if ui.button("Open KiCad Symbols").clicked() {
        let logger = ReactiveEventLogger::with_colors(logger_state, log_colors);
        logger.log_info("Opening KiCad Symbol file dialog");
        open_kicad_symbols(app, logger_state, log_colors);
    }
}

fn open_gerber_files(app: &mut DemoLensApp, logger_state: &Dynamic<ReactiveEventLoggerState>, log_colors: &Dynamic<LogColors>) {
    let logger = ReactiveEventLogger::with_colors(logger_state, log_colors);
    
    if let Some(paths) = rfd::FileDialog::new()
        .add_filter("Gerber Files", &["gbr", "gko", "gtl", "gbl", "gts", "gbs", "gto", "gbo"])
        .set_directory("/")
        .pick_files()
    {
        for path in paths {
            logger.log_info(&format!("Loading Gerber file: {:?}", path.file_name().unwrap_or_default()));
            
            // For now, we'll keep the existing gerber loading logic
            // In a full implementation, you'd load the file here
        }
        
        app.file_type = FileType::Gerber;
    }
}

fn open_kicad_pcb(app: &mut DemoLensApp, logger_state: &Dynamic<ReactiveEventLoggerState>, log_colors: &Dynamic<LogColors>) {
    let logger = ReactiveEventLogger::with_colors(logger_state, log_colors);
    
    if let Some(path) = rfd::FileDialog::new()
        .add_filter("KiCad PCB", &["kicad_pcb"])
        .set_directory("/")
        .pick_file() 
    {
        logger.log_info(&format!("Loading KiCad PCB: {:?}", path.file_name().unwrap_or_default()));
        
        match crate::kicad::parse_pcb_for_cam(path.to_str().unwrap()) {
            Ok(pcb) => {
                app.pcb_data = Some(pcb);
                app.file_type = FileType::KicadPcb;
                setup_pcb_layers(app, logger_state, log_colors);
                
                logger.log_info("KiCad PCB loaded successfully");
            }
            Err(e) => {
                logger.log_error(&format!("Failed to parse PCB: {}", e));
            }
        }
    }
}

fn open_kicad_symbols(app: &mut DemoLensApp, logger_state: &Dynamic<ReactiveEventLoggerState>, log_colors: &Dynamic<LogColors>) {
    let logger = ReactiveEventLogger::with_colors(logger_state, log_colors);
    
    if let Some(path) = rfd::FileDialog::new()
        .add_filter("KiCad Symbol Library", &["kicad_sym"])
        .set_directory("/")
        .pick_file() 
    {
        logger.log_info(&format!("Loading KiCad Symbols: {:?}", path.file_name().unwrap_or_default()));
        
        match crate::kicad::parse_symbol_lib(path.to_str().unwrap()) {
            Ok(symbols) => {
                app.symbol_data = Some(symbols);
                app.file_type = FileType::KicadSymbol;
                
                logger.log_info("KiCad Symbols loaded successfully");
            }
            Err(e) => {
                logger.log_error(&format!("Failed to parse symbols: {}", e));
            }
        }
    }
}

fn setup_pcb_layers(app: &mut DemoLensApp, logger_state: &Dynamic<ReactiveEventLoggerState>, log_colors: &Dynamic<LogColors>) {
    if let Some(pcb) = &app.pcb_data {
        app.active_pcb_layers.clear();
        
        // Enable common layers by default
        let default_layers = ["F.Cu", "B.Cu", "F.SilkS", "B.SilkS", "Edge.Cuts"];
        
        for (_, layer) in &pcb.layers {
            if default_layers.contains(&layer.name.as_str()) {
                app.active_pcb_layers.push(layer.name.clone());
            }
        }
        
        let logger = ReactiveEventLogger::with_colors(logger_state, log_colors);
        logger.log_info(&format!("Loaded {} PCB layers", pcb.layers.len()));
    }
}