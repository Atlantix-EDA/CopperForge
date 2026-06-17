//! CopperForge — wasm browser entrypoint.
//!
//! Phase B of `develop/wasm-demo-plan.md`. The user uploads a CopperForge
//! release `.zip` (or any gerber+drill zip) via the browser's native file
//! picker; the archive is decompressed in memory and the `.gbr` / `.drl`
//! entries are stashed in `WebApp.loaded` for downstream rendering (next
//! commit wires them into a 2D viewport reusing copperforge-core's gerber
//! pipeline).
//!
//! Architecture notes:
//! - rfd::AsyncFileDialog opens an `<input type="file">` via web-sys.
//! - The zip is unpacked via the `zip` crate using a `Cursor<Vec<u8>>` —
//!   no filesystem touched; works under the browser sandbox.
//! - update() is sync, the file load is async — they communicate via an
//!   Arc<Mutex<Option<Result<...>>>>. wasm32 is single-threaded so the
//!   mutex is never actually contended.
//!
//! Native build: prints a hint and exits. The top-level `copperforge`
//! binary is the native entry; this crate is wasm-only.

#[cfg(not(target_arch = "wasm32"))]
fn main() {
    eprintln!("copperforge-web is the browser entrypoint — build with:");
    eprintln!("    cd crates/copperforge-web && trunk serve");
    eprintln!();
    eprintln!("For the native desktop app: cargo run --bin copperforge");
}

#[cfg(target_arch = "wasm32")]
mod app;
#[cfg(target_arch = "wasm32")]
mod board3d;
#[cfg(target_arch = "wasm32")]
mod board_weight;
#[cfg(target_arch = "wasm32")]
mod bom;
#[cfg(target_arch = "wasm32")]
mod canvas;
#[cfg(target_arch = "wasm32")]
mod centroid;
#[cfg(target_arch = "wasm32")]
mod logo;
#[cfg(target_arch = "wasm32")]
mod manufacturability;
#[cfg(target_arch = "wasm32")]
mod pad_count;
#[cfg(target_arch = "wasm32")]
mod release_pkg;
#[cfg(target_arch = "wasm32")]
mod state;
#[cfg(target_arch = "wasm32")]
mod tabs;

#[cfg(target_arch = "wasm32")]
fn main() {
    use wasm_bindgen::JsCast;

    // Route Rust panics through console.error so they surface in DevTools.
    console_error_panic_hook::set_once();
    // Route the log crate through console.log via eframe's WebLogger.
    eframe::WebLogger::init(log::LevelFilter::Info).ok();

    let web_options = eframe::WebOptions::default();

    wasm_bindgen_futures::spawn_local(async {
        let canvas = web_sys::window()
            .expect("no window")
            .document()
            .expect("no document")
            .get_element_by_id("the_canvas_id")
            .expect("no canvas with id 'the_canvas_id'")
            .dyn_into::<web_sys::HtmlCanvasElement>()
            .expect("element with id 'the_canvas_id' is not a canvas");

        let result = eframe::WebRunner::new()
            .start(
                canvas,
                web_options,
                Box::new(|cc| {
                    // Register egui_extras's image loaders so
                    // `Image::from_bytes(...)` can decode PNGs (used
                    // by the About modal's hero) at runtime.
                    egui_extras::install_image_loaders(&cc.egui_ctx);
                    Ok(Box::new(app::WebApp::default()))
                }),
            )
            .await;

        // Remove the boot loading spinner once eframe has taken over.
        let document = web_sys::window().and_then(|w| w.document());
        if let Some(loading) = document.and_then(|d| d.get_element_by_id("loading_text")) {
            let _ = loading.remove();
        }

        if let Err(e) = result {
            log::error!("Failed to start eframe: {e:?}");
        }
    });
}
