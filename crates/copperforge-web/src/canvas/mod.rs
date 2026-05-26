//! Gerber canvas — model + viewport + CPU paint pass.
//!
//! Architectural shape mirrors egui_grafica:
//! - `model.rs` is pure data — parsed `GerberLayer`s with kind/color/visibility
//! - the **viewport** is `gerber_viewer::ViewState` reused verbatim — same
//!   pan/zoom math, same world↔screen transform as the native viewer
//! - `render.rs` is the CPU paint pass — dispatches each visible layer to
//!   `gerber_viewer::GerberRenderer::paint_layer`, which internally
//!   tessellates and emits `egui::Painter` calls
//!
//! The renderer is the same one the native CopperForge uses — wasm just
//! mounts it through eframe's WebGL2 backend. No special wasm pipeline.
//! If a layer ever needs the zicad-style `egui_glow::CallbackFn`
//! treatment for performance, we can swap it for that single layer
//! without disturbing the rest of the canvas.

pub mod model;
pub mod render;

// Top-level re-exports for the two things app.rs needs directly.
// `LayerKind` / `RenderLayer` stay accessible as `canvas::model::*` for
// any caller that needs to pattern-match or construct one.
pub use model::GerberScene;
pub use render::paint;
