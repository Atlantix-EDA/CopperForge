mod cli;

use std::process::ExitCode;

use clap::Parser;

/// Decode the embedded PNG mark into the RGBA form eframe wants. Falls back
/// to the default (generic) icon rather than refusing to start if the asset
/// is ever malformed.
fn app_icon() -> egui::IconData {
    let png = include_bytes!("../assets/media/copperforge-icon.png");
    match image::load_from_memory(png) {
        Ok(img) => {
            let rgba = img.into_rgba8();
            let (width, height) = rgba.dimensions();
            egui::IconData {
                rgba: rgba.into_raw(),
                width,
                height,
            }
        }
        Err(e) => {
            log::warn!("embedded app icon failed to decode: {e}");
            egui::IconData::default()
        }
    }
}

fn main() -> ExitCode {
    use copperforge_core::CopperForgeApp;
    use copperforge_core::platform::parameters::gui::APPLICATION_NAME;

    // env_logger is shared between GUI + CLI; suppress gerber_parser
    // warnings which would otherwise spam the terminal during release
    // packaging.
    env_logger::Builder::from_default_env()
        .filter_module("gerber_parser::parser", log::LevelFilter::Off)
        .init();

    let args = cli::Cli::parse();
    if args.command.is_some() {
        // Headless subcommand (e.g. `copperforge release …`) — no GUI.
        return cli::run(args);
    }

    // No subcommand: launch the GUI exactly as before.
    let result = eframe::run_native(
        APPLICATION_NAME,
        eframe::NativeOptions {
            viewport: egui::ViewportBuilder::default()
                .with_inner_size([1280.0, 768.0])
                // Window/taskbar icon (X11 _NET_WM_ICON). Rasterized from
                // assets/media/copperforge-mark.svg; re-run
                //   convert -background none assets/media/copperforge-mark.svg \
                //     -resize 128x128 assets/media/copperforge-icon.png
                // if the mark changes. app_id is the Wayland analogue —
                // compositors match it against a copperforge.desktop entry.
                .with_icon(app_icon())
                .with_app_id("copperforge"),
            // Without this, eframe defaults to depth_buffer: 0 — no
            // depth attachment allocated, gl.enable(DEPTH_TEST) silently
            // does nothing, and every layer renders in draw order
            // regardless of Z. Symptom: looking at the board from
            // underneath, top copper bleeds through B.Cu + FR-4 because
            // nothing rejects the further-away fragments.
            depth_buffer: 24,
            // 4× MSAA on the window surface. The 3D paint callback draws
            // raw triangles into it, so without this every copper/board
            // edge is a hard stair-step — the "grainy" look. egui's own UI
            // geometry is feathered and doesn't care either way.
            multisampling: 4,
            ..Default::default()
        },
        Box::new(|cc| {
            egui_extras::install_image_loaders(&cc.egui_ctx);
            copperforge_core::theme::apply_visuals(&cc.egui_ctx);
            Ok(Box::new(CopperForgeApp::new()))
        }),
    );
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("eframe error: {e}");
            ExitCode::FAILURE
        }
    }
}
