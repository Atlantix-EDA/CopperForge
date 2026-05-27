//! Procedural CopperForge mark — same geometry as
//! `assets/media/copperforge-mark.svg`, drawn via `egui::Painter`
//! primitives so the wasm bundle doesn't have to carry an SVG
//! rasterizer or a PNG fallback. Scales crisply to any size.
//!
//! Source viewBox is 256×256; everything below is parameterised on a
//! target rect and a scale factor derived from rect.width() / 256.

use eframe::egui::{self, Color32, Pos2, Rect, Shape, Stroke, StrokeKind, Vec2};

/// Copper orange — exact match to the SVG fill/stroke (`#C77B3C`).
pub const COPPER: Color32 = Color32::from_rgb(0xC7, 0x7B, 0x3C);

/// Paint the CopperForge mark centered inside `rect`. The mark is
/// square; the smaller of `rect.width()` / `height()` is used so it
/// stays uniform inside non-square allocations.
pub fn paint(painter: &egui::Painter, rect: Rect, background: Color32) {
    let s = rect.width().min(rect.height()) / 256.0;
    let cx = rect.center().x - 128.0 * s;
    let cy = rect.center().y - 128.0 * s;
    // Lift a point from the 256-space viewbox to the screen.
    let p = |x: f32, y: f32| Pos2::new(cx + x * s, cy + y * s);

    // ── Corner dogleg traces ───────────────────────────────────────
    let dogleg_stroke = Stroke::new(9.0 * s, COPPER);
    painter.add(Shape::line(
        vec![p(188.0, 80.0), p(214.0, 80.0), p(238.0, 56.0)],
        dogleg_stroke,
    ));
    painter.add(Shape::line(
        vec![p(188.0, 176.0), p(214.0, 176.0), p(238.0, 200.0)],
        dogleg_stroke,
    ));
    // Endpoints of the corner traces.
    for (x, y) in [(188.0, 80.0), (238.0, 56.0), (188.0, 176.0), (238.0, 200.0)] {
        painter.circle_filled(p(x, y), 9.5 * s, COPPER);
    }

    // ── Three fan-out traces from the hex (120° apart) ────────────
    let fan_stroke = Stroke::new(15.0 * s, COPPER);
    painter.line_segment([p(182.0, 128.0), p(224.0, 128.0)], fan_stroke);
    painter.line_segment([p(101.0, 174.8), p(80.0, 211.1)], fan_stroke);
    painter.line_segment([p(101.0, 81.2), p(80.0, 44.9)], fan_stroke);
    // Landing pads.
    for (x, y) in [(224.0, 128.0), (80.0, 211.1), (80.0, 44.9)] {
        painter.circle_filled(p(x, y), 13.0 * s, COPPER);
    }

    // ── Hexagonal copper pad with drilled via ─────────────────────
    // Hex vertices, in viewbox coords:
    //   M182,128 L155,174.8 L101,174.8 L74,128 L101,81.2 L155,81.2
    let hex = vec![
        p(182.0, 128.0),
        p(155.0, 174.8),
        p(101.0, 174.8),
        p(74.0, 128.0),
        p(101.0, 81.2),
        p(155.0, 81.2),
    ];
    painter.add(Shape::convex_polygon(hex, COPPER, Stroke::NONE));
    // Drill via — punched by overpainting with the background colour.
    painter.circle_filled(p(128.0, 128.0), 18.0 * s, background);
}

/// Convenience: allocate `size`×`size` inside `ui` and paint the
/// mark, returning the allocated `Response` so callers can attach
/// hover/click handlers if they want.
pub fn show(ui: &mut egui::Ui, size: f32, background: Color32) -> egui::Response {
    let (rect, response) =
        ui.allocate_exact_size(Vec2::splat(size), egui::Sense::hover());
    paint(ui.painter(), rect, background);
    let _ = StrokeKind::Inside; // (kept in the use-line for callers that paint borders)
    response
}
