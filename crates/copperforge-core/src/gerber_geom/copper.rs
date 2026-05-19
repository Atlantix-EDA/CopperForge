//! Copper layer geometry extraction (Phase 4a).
//!
//! Sibling to `extract_outline`: consumes the same modal command stream
//! but routes the primitives differently. Copper gerbers expose three
//! geometry kinds the 3D extruder cares about:
//!
//! - **Flashed apertures** (D03) — SMD pads, through-hole pad rings,
//!   fiducials. The aperture shape (circle / rect / obround / polygon)
//!   is stamped at the flash's coordinates.
//! - **Stroked paths** (D01 outside a region) — copper traces. Each
//!   segment becomes a fat line with the current aperture's width;
//!   circle apertures produce a stadium polygon, other shapes are
//!   deferred to Phase 4b (rare enough in practice).
//! - **Region fills** (G36/G37) — zone fills, ground planes, power
//!   planes. The boundary polygon is tessellated as-is.
//!
//! Every primitive emitted here is already a closed contour, so there's
//! no stitching pass — unlike the outline extractor, which assembles
//! zero-width strokes into loops. The full contour list feeds lyon
//! under the NonZero fill rule.
//!
//! Output mesh is transformed using the *outline's* bbox (not the
//! copper's own), so the copper layer lands pixel-aligned on top of
//! the FR-4 board mesh. Callers pass the outline bbox in.

use std::fs::File;
use std::io::BufReader;
use std::path::Path;

use gerber_parser::parse;
use gerber_types::{
    Aperture, Command, CoordinateNumber, Coordinates, DCode, FunctionCode, GCode,
    InterpolationMode, MacroDecimal, Operation, Unit,
};
use gerber_viewer::BoundingBox;
use lyon::math::Point as LyonPoint;
use lyon::path::Path as LyonPath;
use lyon::tessellation::{
    BuffersBuilder, FillOptions, FillRule, FillTessellator, VertexBuffers,
};
use nalgebra::Point2;

/// Tessellated copper mesh for one side of the board (F.Cu or B.Cu).
/// Vertices are in world coords — origin at the *board's* center, Y-flipped
/// — using the outline's bbox passed in at `extract_copper`. That means
/// the copper mesh and the board mesh share a coordinate frame exactly.
#[derive(Debug, Clone)]
pub struct CopperData {
    pub mesh_vertices_2d: Vec<[f32; 2]>,
    pub mesh_indices: Vec<u32>,
}

/// Per-layer counts for logging. Broken out by primitive so we can spot
/// when a particular shape or case isn't being handled. Shared with the
/// soldermask extractor (same gerber primitives; mask.rs re-uses the
/// copper walker wholesale).
#[derive(Debug, Default, Clone, Copy)]
pub struct CopperCounts {
    pub flashed_circles: usize,
    pub flashed_rectangles: usize,
    pub flashed_obrounds: usize,
    pub flashed_polygons: usize,
    /// KiCad-style `RoundRect` macro flashes, expanded to a rounded-rect
    /// contour. Broken out from `flashed_macros_skipped` because this path
    /// IS rendered.
    pub flashed_roundrects: usize,
    /// Flashed macro apertures that the expander didn't recognize.
    /// Currently anything that isn't KiCad's `RoundRect`.
    pub flashed_macros_skipped: usize,
    pub linear_strokes: usize,
    /// Arc strokes (G02/G03 outside a region with an aperture width) —
    /// deferred to Phase 4b.
    pub arc_strokes_skipped: usize,
    /// Strokes with a non-circle aperture — rare but possible on some
    /// layers; Phase 4b will handle rect/obround stroke shapes.
    pub non_circle_strokes_skipped: usize,
    pub region_polygons: usize,
}

/// Parse + tessellate copper geometry from a single gerber file. `outline_bbox`
/// is the bbox of the board outline, used to place the copper mesh in the
/// same world frame as the board. Returns `None` if the file can't be
/// opened, the parse is totally broken, or no geometry is recovered.
pub fn extract_copper(
    gerber_path: &Path,
    outline_bbox: &BoundingBox,
) -> Option<(CopperData, CopperCounts)> {
    let file = File::open(gerber_path).ok()?;
    let reader = BufReader::new(file);
    let doc = match parse(reader) {
        Ok(d) => d,
        Err((d, _)) => d,
    };
    let unit_scale = match doc.units {
        Some(Unit::Millimeters) => 1.0_f64,
        Some(Unit::Inches) => 25.4_f64,
        None => 1.0_f64,
    };

    let (contours, counts) = walk_copper(&doc, unit_scale);
    if contours.is_empty() {
        return None;
    }
    let (mesh_vertices_2d, mesh_indices) = tessellate(&contours, outline_bbox)?;
    Some((
        CopperData {
            mesh_vertices_2d,
            mesh_indices,
        },
        counts,
    ))
}

// ────────────────────────────────────────────────────────────────────────
// State machine walker (copper-specific)
// ────────────────────────────────────────────────────────────────────────

struct State {
    /// Current pen position in mm (modal carry — see `gerber_geom::mod` for
    /// the full explanation; gerber_parser does not maintain this).
    pos_mm: Point2<f64>,
    /// G01 / G02 / G03.
    mode: InterpolationMode,
    /// G36 region-mode flag.
    in_region: bool,
    /// Boundary vertices of the region currently being assembled.
    region_pts: Vec<Point2<f32>>,
    /// First-coord flag — until we've seen one, the "current position" is
    /// undefined and we can't honestly emit a stroke segment.
    pos_initialized: bool,
    /// Currently-selected aperture D-code. Zero until the file selects one
    /// via `DCode::SelectAperture`.
    aperture: i32,
}

impl State {
    fn new() -> Self {
        Self {
            pos_mm: Point2::new(0.0, 0.0),
            mode: InterpolationMode::Linear,
            in_region: false,
            region_pts: Vec::new(),
            pos_initialized: false,
            aperture: 0,
        }
    }
}

/// Walk a parsed gerber doc and emit closed contours for every flash,
/// stroke, and region encountered. Used by both `extract_copper` (this
/// module) and `extract_mask` (sibling `mask.rs`) — copper and soldermask
/// gerbers expose the same primitive set; only the post-tessellation
/// interpretation differs.
pub(super) fn walk_copper(
    doc: &gerber_parser::GerberDoc,
    unit_scale: f64,
) -> (Vec<Vec<Point2<f32>>>, CopperCounts) {
    let mut s = State::new();
    let mut contours = Vec::new();
    let mut counts = CopperCounts::default();

    for cmd in doc.commands() {
        match cmd {
            Command::FunctionCode(FunctionCode::DCode(DCode::SelectAperture(n))) => {
                s.aperture = *n;
            }
            Command::FunctionCode(FunctionCode::GCode(GCode::InterpolationMode(m))) => {
                s.mode = *m;
            }
            Command::FunctionCode(FunctionCode::GCode(GCode::RegionMode(on))) => {
                if *on {
                    s.in_region = true;
                    s.region_pts.clear();
                } else {
                    s.in_region = false;
                    if s.region_pts.len() >= 3 {
                        contours.push(std::mem::take(&mut s.region_pts));
                        counts.region_polygons += 1;
                    } else {
                        s.region_pts.clear();
                    }
                }
            }
            Command::FunctionCode(FunctionCode::DCode(DCode::Operation(op))) => {
                handle_op(op, &mut s, unit_scale, &doc.apertures, &mut contours, &mut counts);
            }
            _ => {}
        }
    }

    (contours, counts)
}

fn handle_op(
    op: &Operation,
    s: &mut State,
    unit_scale: f64,
    apertures: &std::collections::HashMap<i32, Aperture>,
    contours: &mut Vec<Vec<Point2<f32>>>,
    counts: &mut CopperCounts,
) {
    match op {
        Operation::Move(coords_opt) => {
            if let Some(coords) = coords_opt {
                let new_pos = apply_coords(s, coords, unit_scale);
                if s.in_region {
                    s.region_pts.push(new_pos);
                }
            }
        }
        Operation::Interpolate(coords_opt, _offset_opt) => {
            let start = if s.pos_initialized {
                Point2::new(s.pos_mm.x as f32, s.pos_mm.y as f32)
            } else {
                if let Some(coords) = coords_opt {
                    let new_pos = apply_coords(s, coords, unit_scale);
                    if s.in_region {
                        s.region_pts.push(new_pos);
                    }
                }
                return;
            };
            let Some(coords) = coords_opt else { return };
            let end = apply_coords(s, coords, unit_scale);

            if s.in_region {
                // Inside a region: collect boundary vertices for later flush
                // on G37. Arc boundaries in regions would need chord-
                // flattening — for Phase 4a, treat them as linear (rare on
                // zone fills, KiCad exports straight-line boundaries almost
                // exclusively).
                match s.mode {
                    InterpolationMode::Linear => s.region_pts.push(end),
                    _ => s.region_pts.push(end), // TODO Phase 4b: flatten arc
                }
            } else {
                // Drawn stroke — the aperture's shape at its "width" decides
                // what the fat-line contour looks like. Circle apertures give
                // a stadium polygon; non-circle strokes defer to Phase 4b.
                match s.mode {
                    InterpolationMode::Linear => {
                        match apertures.get(&s.aperture) {
                            Some(Aperture::Circle(c)) => {
                                let width = (c.diameter * unit_scale) as f32;
                                if width > 0.0 {
                                    contours.push(stadium_contour(start, end, width));
                                    counts.linear_strokes += 1;
                                }
                            }
                            Some(_) => {
                                counts.non_circle_strokes_skipped += 1;
                            }
                            None => {} // no aperture selected — ignore
                        }
                    }
                    InterpolationMode::ClockwiseCircular
                    | InterpolationMode::CounterclockwiseCircular => {
                        counts.arc_strokes_skipped += 1;
                    }
                }
            }
        }
        Operation::Flash(coords_opt) => {
            if let Some(coords) = coords_opt {
                let pos = apply_coords(s, coords, unit_scale);
                if let Some(ap) = apertures.get(&s.aperture) {
                    emit_aperture_flash(ap, pos, unit_scale, contours, counts);
                }
            }
        }
    }
}

fn apply_coords(s: &mut State, coords: &Coordinates, unit_scale: f64) -> Point2<f32> {
    let x = coords.x.map(|n| cn_to_mm(n, unit_scale)).unwrap_or(s.pos_mm.x);
    let y = coords.y.map(|n| cn_to_mm(n, unit_scale)).unwrap_or(s.pos_mm.y);
    s.pos_mm = Point2::new(x, y);
    s.pos_initialized = true;
    Point2::new(x as f32, y as f32)
}

fn cn_to_mm(n: CoordinateNumber, unit_scale: f64) -> f64 {
    let raw: f64 = n.into();
    raw * unit_scale
}

// ────────────────────────────────────────────────────────────────────────
// Aperture shape → contour
// ────────────────────────────────────────────────────────────────────────

fn emit_aperture_flash(
    ap: &Aperture,
    pos: Point2<f32>,
    unit_scale: f64,
    contours: &mut Vec<Vec<Point2<f32>>>,
    counts: &mut CopperCounts,
) {
    match ap {
        Aperture::Circle(c) => {
            let d = (c.diameter * unit_scale) as f32;
            if d > 0.0 {
                contours.push(circle_contour(pos, d * 0.5));
                counts.flashed_circles += 1;
            }
        }
        Aperture::Rectangle(r) => {
            let w = (r.x * unit_scale) as f32;
            let h = (r.y * unit_scale) as f32;
            if w > 0.0 && h > 0.0 {
                contours.push(rect_contour(pos, w, h));
                counts.flashed_rectangles += 1;
            }
        }
        Aperture::Obround(r) => {
            let w = (r.x * unit_scale) as f32;
            let h = (r.y * unit_scale) as f32;
            if w > 0.0 && h > 0.0 {
                contours.push(obround_contour(pos, w, h));
                counts.flashed_obrounds += 1;
            }
        }
        Aperture::Polygon(p) => {
            let d = (p.diameter * unit_scale) as f32;
            let rotation_deg = p.rotation.unwrap_or(0.0) as f32;
            if d > 0.0 && p.vertices >= 3 {
                contours.push(polygon_contour(pos, d * 0.5, p.vertices, rotation_deg));
                counts.flashed_polygons += 1;
            }
        }
        Aperture::Macro(name, args_opt) => {
            if name == "RoundRect" {
                if let Some(args) = args_opt.as_ref() {
                    if let Some(contour) = roundrect_contour_from_macro(pos, args, unit_scale) {
                        contours.push(contour);
                        counts.flashed_roundrects += 1;
                        return;
                    }
                }
            }
            counts.flashed_macros_skipped += 1;
        }
    }
}

/// Adaptive step count: ~5 µm chord error, clamped so tiny pads stay round
/// and mounting-hole-sized flashes don't balloon the triangle count.
fn circle_steps(radius: f32) -> usize {
    let tol = 0.005_f32;
    ((std::f32::consts::TAU * radius / (8.0 * tol).sqrt()).ceil() as usize).clamp(32, 128)
}

fn circle_contour(center: Point2<f32>, radius: f32) -> Vec<Point2<f32>> {
    let steps = circle_steps(radius);
    (0..steps)
        .map(|i| {
            let a = (i as f32 / steps as f32) * std::f32::consts::TAU;
            Point2::new(center.x + radius * a.cos(), center.y + radius * a.sin())
        })
        .collect()
}

fn rect_contour(center: Point2<f32>, w: f32, h: f32) -> Vec<Point2<f32>> {
    let (hw, hh) = (w * 0.5, h * 0.5);
    vec![
        Point2::new(center.x - hw, center.y - hh),
        Point2::new(center.x + hw, center.y - hh),
        Point2::new(center.x + hw, center.y + hh),
        Point2::new(center.x - hw, center.y + hh),
    ]
}

/// Stadium: rectangle with semicircular ends. The long axis is the larger
/// dimension; if w ≈ h the obround collapses to a circle.
fn obround_contour(center: Point2<f32>, w: f32, h: f32) -> Vec<Point2<f32>> {
    if (w - h).abs() < 1e-4 {
        return circle_contour(center, w * 0.5);
    }
    let short = w.min(h) * 0.5;
    let end_steps = (circle_steps(short) / 2).max(12);
    let mut pts = Vec::with_capacity(end_steps * 2 + 4);
    if w > h {
        let r = h * 0.5;
        let cx_right = center.x + (w - h) * 0.5;
        let cx_left = center.x - (w - h) * 0.5;
        for i in 0..=end_steps {
            let a = -std::f32::consts::FRAC_PI_2
                + (i as f32 / end_steps as f32) * std::f32::consts::PI;
            pts.push(Point2::new(cx_right + r * a.cos(), center.y + r * a.sin()));
        }
        for i in 0..=end_steps {
            let a = std::f32::consts::FRAC_PI_2
                + (i as f32 / end_steps as f32) * std::f32::consts::PI;
            pts.push(Point2::new(cx_left + r * a.cos(), center.y + r * a.sin()));
        }
    } else {
        let r = w * 0.5;
        let cy_top = center.y + (h - w) * 0.5;
        let cy_bot = center.y - (h - w) * 0.5;
        for i in 0..=end_steps {
            let a = (i as f32 / end_steps as f32) * std::f32::consts::PI;
            pts.push(Point2::new(center.x + r * a.cos(), cy_top + r * a.sin()));
        }
        for i in 0..=end_steps {
            let a = std::f32::consts::PI
                + (i as f32 / end_steps as f32) * std::f32::consts::PI;
            pts.push(Point2::new(center.x + r * a.cos(), cy_bot + r * a.sin()));
        }
    }
    pts
}

/// Expand a KiCad `RoundRect` macro flash to a rounded-rectangle contour.
///
/// KiCad's macro layout (see `%AMRoundRect*` in a KiCad-exported gerber):
/// args = `[r, x1, y1, x2, y2, x3, y3, x4, y4, rotation]`. The four `(x,y)`
/// pairs are the *inner* rect corners — the centers of the four corner
/// arcs — and `r` is the arc radius. KiCad always emits these axis-aligned;
/// pad rotation (90° / 180° / 270°) is handled by permuting which corner
/// lands in which slot, not by rotating the points. So the outer contour
/// is just "axis-aligned inner bbox, inflated by `r`, with radius-`r` arcs
/// at the four corners" — no trig needed.
///
/// Returns `None` if args aren't resolvable to concrete decimals (macro
/// variables / expressions) or the 10th arg encodes a non-zero rotation
/// we're not attempting to honour yet. In those cases the caller falls
/// back to counting as skipped.
fn roundrect_contour_from_macro(
    pos: Point2<f32>,
    args: &[MacroDecimal],
    unit_scale: f64,
) -> Option<Vec<Point2<f32>>> {
    if args.len() < 9 {
        return None;
    }
    // Resolve the first 9 args as concrete f32 values (in gerber units,
    // converted to mm). A variable/expression means the flash was written
    // without inlining — shouldn't happen on KiCad output but bail cleanly.
    let mut vals = [0.0_f32; 9];
    for (i, slot) in vals.iter_mut().enumerate() {
        match args[i] {
            MacroDecimal::Value(v) => *slot = (v * unit_scale) as f32,
            _ => return None,
        }
    }
    // Tolerate a rotation arg only if it resolves to ~0. Non-zero rotated
    // roundrects are rare in KiCad output and we'd need real trig — defer.
    if let Some(rot) = args.get(9) {
        match rot {
            MacroDecimal::Value(v) if v.abs() < 1e-3 => {}
            MacroDecimal::Value(_) => return None,
            _ => return None,
        }
    }
    let r = vals[0];
    if r <= 0.0 {
        return None;
    }
    // Inner-corner extremes. KiCad lists the corners CCW in some rotation
    // — for axis-aligned roundrects any permutation collapses to the same
    // min/max here.
    let xs = [vals[1], vals[3], vals[5], vals[7]];
    let ys = [vals[2], vals[4], vals[6], vals[8]];
    let xmin = xs.iter().cloned().fold(f32::INFINITY, f32::min);
    let xmax = xs.iter().cloned().fold(f32::NEG_INFINITY, f32::max);
    let ymin = ys.iter().cloned().fold(f32::INFINITY, f32::min);
    let ymax = ys.iter().cloned().fold(f32::NEG_INFINITY, f32::max);
    if !(xmax > xmin && ymax > ymin) {
        return None;
    }

    // CCW quarter-arcs at each inner corner, starting bottom-right:
    //   BR: angle sweeps  -π/2 → 0    (outward = right / down-right)
    //   TR:                0   → π/2
    //   TL:                π/2 → π
    //   BL:                π   → 3π/2
    let corners = [
        (xmax, ymin, -std::f32::consts::FRAC_PI_2),
        (xmax, ymax, 0.0_f32),
        (xmin, ymax, std::f32::consts::FRAC_PI_2),
        (xmin, ymin, std::f32::consts::PI),
    ];
    let steps_per_arc = (circle_steps(r) / 4).max(4);
    let mut pts = Vec::with_capacity(steps_per_arc * 4 + 4);
    for &(cx, cy, a0) in &corners {
        for k in 0..=steps_per_arc {
            let t = k as f32 / steps_per_arc as f32;
            let a = a0 + t * std::f32::consts::FRAC_PI_2;
            pts.push(Point2::new(
                pos.x + cx + r * a.cos(),
                pos.y + cy + r * a.sin(),
            ));
        }
    }
    Some(pts)
}

fn polygon_contour(
    center: Point2<f32>,
    radius: f32,
    vertices: u8,
    rotation_deg: f32,
) -> Vec<Point2<f32>> {
    let n = vertices.max(3) as usize;
    let rot = rotation_deg.to_radians();
    (0..n)
        .map(|i| {
            let a = rot + (i as f32 / n as f32) * std::f32::consts::TAU;
            Point2::new(center.x + radius * a.cos(), center.y + radius * a.sin())
        })
        .collect()
}

/// Stadium-shaped contour for a stroked linear segment with a circle
/// aperture. Two semicircular end caps + the implicit straight sides
/// between them.
fn stadium_contour(start: Point2<f32>, end: Point2<f32>, width: f32) -> Vec<Point2<f32>> {
    let dx = end.x - start.x;
    let dy = end.y - start.y;
    let len = (dx * dx + dy * dy).sqrt();
    let r = width * 0.5;
    if len < 1e-6 {
        return circle_contour(start, r);
    }
    let cos = dx / len;
    let sin = dy / len;
    let to_world = |lx: f32, ly: f32| {
        Point2::new(
            start.x + lx * cos - ly * sin,
            start.y + lx * sin + ly * cos,
        )
    };
    let end_steps = (circle_steps(r) / 2).max(12);
    let mut pts = Vec::with_capacity(end_steps * 2 + 4);
    // End cap at the far end of the segment (local (len, 0)).
    for i in 0..=end_steps {
        let a = -std::f32::consts::FRAC_PI_2
            + (i as f32 / end_steps as f32) * std::f32::consts::PI;
        pts.push(to_world(len + r * a.cos(), r * a.sin()));
    }
    // End cap at the near end (local (0, 0)).
    for i in 0..=end_steps {
        let a = std::f32::consts::FRAC_PI_2
            + (i as f32 / end_steps as f32) * std::f32::consts::PI;
        pts.push(to_world(r * a.cos(), r * a.sin()));
    }
    pts
}

// ────────────────────────────────────────────────────────────────────────
// Tessellation with outline-aligned world transform
// ────────────────────────────────────────────────────────────────────────

fn tessellate(
    contours: &[Vec<Point2<f32>>],
    outline_bbox: &BoundingBox,
) -> Option<(Vec<[f32; 2]>, Vec<u32>)> {
    let mut builder = LyonPath::builder();
    for contour in contours {
        if contour.len() < 3 {
            continue;
        }
        builder.begin(LyonPoint::new(contour[0].x, contour[0].y));
        for p in &contour[1..] {
            builder.line_to(LyonPoint::new(p.x, p.y));
        }
        builder.close();
    }
    let path = builder.build();

    let mut geometry: VertexBuffers<[f32; 2], u32> = VertexBuffers::new();
    let mut tess = FillTessellator::new();
    tess.tessellate_path(
        &path,
        &FillOptions::default().with_fill_rule(FillRule::NonZero),
        &mut BuffersBuilder::new(&mut geometry, |v: lyon::tessellation::FillVertex| {
            [v.position().x, v.position().y]
        }),
    )
    .ok()?;
    if geometry.indices.is_empty() {
        return None;
    }

    // World transform — identical to the outline's (Stage 6 of the FDD
    // pipeline): shift by bbox center (no Y-flip; gerber is Y-up per spec
    // §4.2.3, same as 3D world). Using the *outline's* bbox here is what
    // locks copper alignment to the board mesh vertex-for-vertex.
    let cx = ((outline_bbox.min.x + outline_bbox.max.x) * 0.5) as f32;
    let cy = ((outline_bbox.min.y + outline_bbox.max.y) * 0.5) as f32;
    for v in &mut geometry.vertices {
        v[0] -= cx;
        v[1] -= cy;
    }

    Some((geometry.vertices, geometry.indices))
}
