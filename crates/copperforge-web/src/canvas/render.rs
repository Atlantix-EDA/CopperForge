//! Canvas paint pass — egui::Painter dispatch via
//! `gerber_viewer::GerberRenderer::paint_layer`.
//!
//! One paint call per visible layer, in the layer ordering already
//! baked into `GerberScene::layers` (sorted by z-order at parse time).
//! No special wasm pipeline — exactly the same code path the native
//! viewer uses, just running through eframe's WebGL2 backend.

use egui::Painter;
use gerber_viewer::{GerberRenderer, GerberTransform, RenderConfiguration, ViewState};

use super::model::GerberScene;

/// Paint every visible layer in the scene. `view_state` carries the
/// pan/zoom transform; `transform` is per-layer image transform
/// (identity here — we don't apply mirroring/rotation in the demo).
pub fn paint(painter: &Painter, scene: &GerberScene, view_state: ViewState) {
    let config = RenderConfiguration::default();
    let transform = GerberTransform::default();

    for layer in scene.layers.iter().filter(|l| l.visible) {
        let renderer = GerberRenderer::new(&config, view_state, &transform, &layer.gerber);
        renderer.paint_layer(painter, layer.color);
    }
}
