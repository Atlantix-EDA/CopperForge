fn main() -> eframe::Result<()> {
    use kiforge_core::DemoLensApp;
    use kiforge_core::platform::parameters::gui::APPLICATION_NAME;
    
    // Configure env_logger to filter out gerber_parser warnings
    env_logger::Builder::from_default_env()
        .filter_module("gerber_parser::parser", log::LevelFilter::Off)
        .init();
    eframe::run_native(
        APPLICATION_NAME,
        eframe::NativeOptions {
            viewport: egui::ViewportBuilder::default().with_inner_size([1280.0, 768.0]),
            ..Default::default()
        },
        Box::new(|cc|{
            egui_extras::install_image_loaders(&cc.egui_ctx);
            Ok(Box::new(DemoLensApp::new()))
        }))
}