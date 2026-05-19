fn main() -> eframe::Result<()> {
    use copperforge_core::CopperForgeApp;
    use copperforge_core::platform::parameters::gui::APPLICATION_NAME;
    
    // Configure env_logger to filter out gerber_parser warnings
    env_logger::Builder::from_default_env()
        .filter_module("gerber_parser::parser", log::LevelFilter::Off)
        .init();
    eframe::run_native(
        APPLICATION_NAME,
        eframe::NativeOptions {
            viewport: egui::ViewportBuilder::default().with_inner_size([1280.0, 768.0]),
            // Without this, eframe defaults to depth_buffer: 0 — no
            // depth attachment allocated, gl.enable(DEPTH_TEST) silently
            // does nothing, and every layer renders in draw order
            // regardless of Z. Symptom: looking at the board from
            // underneath, top copper bleeds through B.Cu + FR-4 because
            // nothing rejects the further-away fragments.
            depth_buffer: 24,
            ..Default::default()
        },
        Box::new(|cc|{
            egui_extras::install_image_loaders(&cc.egui_ctx);
            copperforge_core::theme::apply_visuals(&cc.egui_ctx);
            Ok(Box::new(CopperForgeApp::new()))
        }))
}