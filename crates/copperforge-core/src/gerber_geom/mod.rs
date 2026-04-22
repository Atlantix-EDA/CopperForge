//! Gerber-driven geometry extraction for the 3D viewer.
//!
//! Reads a gerber file with `gerber_parser` and walks the command stream as
//! a modal state machine — tracking the current (x, y) position, active
//! aperture, interpolation mode, and region state. Produces a triangle mesh
//! in world-space coordinates ready for GPU upload.
//!
//! Scope (Phase 3): board outline only. `extract_outline()` consumes a
//! mechanical-outline gerber (typically `*-Edge_Cuts.gbr` from kicad-cli)
//! and returns stitched closed contours + a tessellated flat mesh. Later
//! phases extend this to copper, soldermask, and drill layers.
//!
//! # Why a state machine
//!
//! Gerber coordinates are modal: `X25000*` with no Y carries the previous
//! Y value over. `gerber_parser` surfaces this faithfully — `Coordinates.x`
//! and `.y` are `Option<CoordinateNumber>` — but does **not** maintain the
//! current-position register for you. Any walker that reads `coords.x`
//! without a modal fallback will silently produce broken geometry wherever
//! a file omits X or Y (which KiCad exports do routinely, to keep file
//! size reasonable).

use std::fs::File;
use std::io::BufReader;
use std::path::Path;

use gerber_parser::parse;
use gerber_types::{
    Command, CoordinateNumber, CoordinateOffset, Coordinates, DCode, FunctionCode, GCode,
    InterpolationMode, Operation, Unit,
};
use gerber_viewer::BoundingBox;
use lyon::math::Point as LyonPoint;
use lyon::path::Path as LyonPath;
use lyon::tessellation::{
    BuffersBuilder, FillOptions, FillRule, FillTessellator, VertexBuffers,
};
use nalgebra::Point2;

/// Tessellated mesh + stitched contours for one layer's geometry. Coords are
/// mm in world-space (origin at bbox.min, Y-flipped) so the 3D panel can
/// upload directly without per-frame math.
#[derive(Debug, Clone)]
pub struct OutlineData {
    /// Stitched closed contours in the gerber's original coord space (mm,
    /// Y-down). Largest-area first. Kept for debug inspection + later
    /// phases that want the raw geometry.
    pub contours: Vec<Vec<Point2<f32>>>,
    /// Axis-aligned bbox of all contours in original coord space.
    pub bbox: BoundingBox,
    /// Triangle soup ready for the 3D renderer. Origin at (0, 0), Y-flipped
    /// so a top-down camera matches the 2D gerber viewer's orientation.
    pub mesh_vertices_2d: Vec<[f32; 2]>,
    pub mesh_indices: Vec<u32>,
}

/// Per-layer counts surfaced alongside the parse so the caller can log.
#[derive(Debug, Default, Clone, Copy)]
pub struct OutlineCounts {
    pub linear_strokes: usize,
    pub arc_strokes: usize,
    pub region_polygons: usize,
    pub stitched_contours: usize,
}

/// Top-level entry. Parse `gerber_path` and return tessellated outline
/// geometry. Returns `None` if the file can't be opened, the parse fails
/// catastrophically, or no closed contours can be recovered.
pub fn extract_outline(gerber_path: &Path) -> Option<(OutlineData, OutlineCounts)> {
    let file = File::open(gerber_path).ok()?;
    let reader = BufReader::new(file);
    // parse() returns Result<GerberDoc, (GerberDoc, ParseError)>. Even on the
    // error arm the partial doc is usable — the error is typically a late-
    // file issue (bad trailer) that doesn't invalidate earlier geometry.
    let doc = match parse(reader) {
        Ok(d) => d,
        Err((d, _)) => d,
    };

    // Unit scale: file declares mm or inches, we want mm everywhere.
    let unit_scale = match doc.units {
        Some(Unit::Millimeters) => 1.0_f64,
        Some(Unit::Inches) => 25.4_f64,
        // No units declared — rare in modern gerbers but let it through
        // as mm. Caller will see a bbox log and can spot scaling errors.
        None => 1.0_f64,
    };

    let (segments, closed_regions, counts) = walk_commands(&doc, unit_scale);
    let mut contours = stitch_segments(&segments);
    contours.extend(closed_regions);

    if contours.is_empty() {
        return None;
    }

    let mut counts = counts;
    let (contours_sorted, bbox) = sort_and_bbox(contours);
    counts.stitched_contours = contours_sorted.len();
    let (mesh_vertices_2d, mesh_indices) = tessellate(&contours_sorted, &bbox)?;
    Some((
        OutlineData {
            contours: contours_sorted,
            bbox,
            mesh_vertices_2d,
            mesh_indices,
        },
        counts,
    ))
}

// ────────────────────────────────────────────────────────────────────────
// State machine walker
// ────────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
struct Segment {
    a: Point2<f32>,
    b: Point2<f32>,
}

struct WalkState {
    /// Running position in mm (the modal carry that gerber_parser does NOT
    /// maintain for you — see module docs).
    pos_mm: Point2<f64>,
    /// G01 / G02 / G03 — set by InterpolationMode commands; persists.
    mode: InterpolationMode,
    /// G36 / G37 — inside a region block the interpolate ops don't draw
    /// strokes, they trace a polygon boundary. Start fresh vertex list on
    /// G36, close and emit on G37.
    in_region: bool,
    region_pts: Vec<Point2<f32>>,
    /// Have we ever seen a coord? Until we have, the "current position" is
    /// undefined; emitting a segment from (0,0) to the first Move produces
    /// a spurious edge. Gate on this flag.
    pos_initialized: bool,
}

impl WalkState {
    fn new() -> Self {
        Self {
            pos_mm: Point2::new(0.0, 0.0),
            mode: InterpolationMode::Linear,
            in_region: false,
            region_pts: Vec::new(),
            pos_initialized: false,
        }
    }
}

fn walk_commands(
    doc: &gerber_parser::GerberDoc,
    unit_scale: f64,
) -> (Vec<Segment>, Vec<Vec<Point2<f32>>>, OutlineCounts) {
    let mut state = WalkState::new();
    let mut segments = Vec::new();
    let mut regions = Vec::new();
    let mut counts = OutlineCounts::default();

    for cmd in doc.commands() {
        match cmd {
            Command::FunctionCode(FunctionCode::GCode(GCode::InterpolationMode(m))) => {
                state.mode = *m;
            }
            Command::FunctionCode(FunctionCode::GCode(GCode::RegionMode(on))) => {
                if *on {
                    // G36 — starts a new region. Any in-flight points from a
                    // malformed file get dropped.
                    state.in_region = true;
                    state.region_pts.clear();
                } else {
                    // G37 — close this region.
                    state.in_region = false;
                    if state.region_pts.len() >= 3 {
                        regions.push(std::mem::take(&mut state.region_pts));
                        counts.region_polygons += 1;
                    } else {
                        state.region_pts.clear();
                    }
                }
            }
            Command::FunctionCode(FunctionCode::DCode(DCode::Operation(op))) => {
                handle_operation(op, &mut state, unit_scale, &mut segments, &mut counts);
            }
            // DCode::SelectAperture(n) — Edge.Cuts doesn't care about
            // aperture size for path extraction. Later phases (copper)
            // will read this for stroke widths.
            _ => {}
        }
    }

    (segments, regions, counts)
}

fn handle_operation(
    op: &Operation,
    state: &mut WalkState,
    unit_scale: f64,
    segments: &mut Vec<Segment>,
    counts: &mut OutlineCounts,
) {
    match op {
        Operation::Move(coords_opt) => {
            // D02. Moves the pen without drawing. For regions this is the
            // start vertex of a new sub-path.
            if let Some(coords) = coords_opt {
                let new_pos = apply_coords(state, coords, unit_scale);
                if state.in_region {
                    state.region_pts.push(new_pos);
                }
            }
        }
        Operation::Interpolate(coords_opt, offset_opt) => {
            // D01. Draws from current position to new position.
            let start = if state.pos_initialized {
                Point2::new(state.pos_mm.x as f32, state.pos_mm.y as f32)
            } else {
                // No prior position — treat as a move.
                if let Some(coords) = coords_opt {
                    let new_pos = apply_coords(state, coords, unit_scale);
                    if state.in_region {
                        state.region_pts.push(new_pos);
                    }
                }
                return;
            };
            let Some(coords) = coords_opt else { return };
            let end = apply_coords(state, coords, unit_scale);

            if state.in_region {
                // Region mode: the draw contributes a boundary vertex.
                // For arcs inside a region we still need the intermediate
                // chord vertices so the boundary reads as a smooth polygon.
                match state.mode {
                    InterpolationMode::Linear => {
                        state.region_pts.push(end);
                    }
                    InterpolationMode::ClockwiseCircular
                    | InterpolationMode::CounterclockwiseCircular => {
                        let flat = flatten_arc(start, end, offset_opt, state.mode, unit_scale);
                        // First point equals start (already in list); skip it.
                        state.region_pts.extend(flat.into_iter().skip(1));
                        counts.arc_strokes += 1;
                    }
                }
            } else {
                // Outside a region: this is a drawn stroke (e.g. Edge.Cuts
                // traces). Emit line segments for the path; width is ignored
                // for board-outline extraction.
                match state.mode {
                    InterpolationMode::Linear => {
                        segments.push(Segment { a: start, b: end });
                        counts.linear_strokes += 1;
                    }
                    InterpolationMode::ClockwiseCircular
                    | InterpolationMode::CounterclockwiseCircular => {
                        let flat = flatten_arc(start, end, offset_opt, state.mode, unit_scale);
                        for w in flat.windows(2) {
                            segments.push(Segment { a: w[0], b: w[1] });
                        }
                        counts.arc_strokes += 1;
                    }
                }
            }
        }
        Operation::Flash(coords_opt) => {
            // D03. Places the currently selected aperture at `coords`. For
            // board-outline extraction this is a no-op — Edge.Cuts files
            // rarely flash apertures, and when they do (e.g. a round
            // fiducial) they don't belong in the board outline polygon.
            if let Some(coords) = coords_opt {
                apply_coords(state, coords, unit_scale);
            }
        }
    }
}

/// Update `state.pos_mm` by treating missing X / Y as "carry from previous
/// position" (the modal gerber rule). Returns the new position as `f32`
/// for storage in the contour lists.
fn apply_coords(
    state: &mut WalkState,
    coords: &Coordinates,
    unit_scale: f64,
) -> Point2<f32> {
    let x = coords.x.map(|n| cn_to_mm(n, unit_scale)).unwrap_or(state.pos_mm.x);
    let y = coords.y.map(|n| cn_to_mm(n, unit_scale)).unwrap_or(state.pos_mm.y);
    state.pos_mm = Point2::new(x, y);
    state.pos_initialized = true;
    Point2::new(x as f32, y as f32)
}

fn cn_to_mm(n: CoordinateNumber, unit_scale: f64) -> f64 {
    let raw: f64 = n.into();
    raw * unit_scale
}

// ────────────────────────────────────────────────────────────────────────
// Arc flattening
// ────────────────────────────────────────────────────────────────────────

/// Break a circular arc into chord segments with a ~50µm sagitta tolerance.
/// Gerber X3 defaults to multi-quadrant mode (G75), so the center is
/// unambiguously `start + offset` regardless of direction — we don't need
/// to worry about the legacy single-quadrant center-sign ambiguity.
fn flatten_arc(
    start: Point2<f32>,
    end: Point2<f32>,
    offset_opt: &Option<CoordinateOffset>,
    mode: InterpolationMode,
    unit_scale: f64,
) -> Vec<Point2<f32>> {
    let Some(offset) = offset_opt else {
        // No I/J offset — malformed arc. Fall back to straight segment.
        return vec![start, end];
    };
    let i = offset.x.map(|n| cn_to_mm(n, unit_scale) as f32).unwrap_or(0.0);
    let j = offset.y.map(|n| cn_to_mm(n, unit_scale) as f32).unwrap_or(0.0);
    let cx = start.x + i;
    let cy = start.y + j;

    let r = ((start.x - cx).powi(2) + (start.y - cy).powi(2)).sqrt();
    if r <= 0.0 {
        return vec![start, end];
    }

    let a_start = (start.y - cy).atan2(start.x - cx);
    let a_end = (end.y - cy).atan2(end.x - cx);

    // Signed sweep in the direction of interpolation.
    let (total_sweep, step_sign) = match mode {
        InterpolationMode::CounterclockwiseCircular => {
            let mut s = a_end - a_start;
            if s <= 0.0 { s += std::f32::consts::TAU; }
            (s, 1.0_f32)
        }
        InterpolationMode::ClockwiseCircular => {
            let mut s = a_start - a_end;
            if s <= 0.0 { s += std::f32::consts::TAU; }
            (s, -1.0_f32)
        }
        InterpolationMode::Linear => return vec![start, end],
    };

    // Full-circle arc (start == end): the G75 spec says this means a full
    // rotation. Without this, a closed circle with identical start/end would
    // collapse to a single chord.
    let total_sweep = if total_sweep.abs() < 1e-5
        && (start.x - end.x).abs() < 1e-5
        && (start.y - end.y).abs() < 1e-5
    {
        std::f32::consts::TAU
    } else {
        total_sweep
    };

    // Chord tolerance — 50µm. Steps = ceil(sweep * r / sqrt(8 * tol)).
    let tol = 0.05_f32;
    let steps = ((total_sweep * r / (8.0 * tol).sqrt()).ceil() as usize)
        .max(4)
        .min(256);

    let mut out = Vec::with_capacity(steps + 1);
    for i in 0..=steps {
        let t = i as f32 / steps as f32;
        let a = a_start + step_sign * t * total_sweep;
        out.push(Point2::new(cx + r * a.cos(), cy + r * a.sin()));
    }
    // Snap endpoints to the source coordinates so the stitcher sees exact
    // matches with neighbouring primitives.
    if let Some(first) = out.first_mut() { *first = start; }
    if let Some(last) = out.last_mut() { *last = end; }
    out
}

// ────────────────────────────────────────────────────────────────────────
// Segment stitching: walk segments into closed contours
// ────────────────────────────────────────────────────────────────────────

fn quant(x: f32) -> i64 {
    (x as f64 * 1_000.0).round() as i64
}

fn quant_point(p: Point2<f32>) -> (i64, i64) {
    (quant(p.x), quant(p.y))
}

fn stitch_segments(segments: &[Segment]) -> Vec<Vec<Point2<f32>>> {
    use std::collections::HashMap;

    let mut adj: HashMap<(i64, i64), Vec<usize>> = HashMap::new();
    for (i, s) in segments.iter().enumerate() {
        adj.entry(quant_point(s.a)).or_default().push(i);
        adj.entry(quant_point(s.b)).or_default().push(i);
    }

    let mut used = vec![false; segments.len()];
    let mut contours = Vec::new();

    for seed in 0..segments.len() {
        if used[seed] { continue; }
        let mut contour = vec![segments[seed].a, segments[seed].b];
        used[seed] = true;
        let start_key = quant_point(segments[seed].a);
        let mut current_key = quant_point(segments[seed].b);

        loop {
            if current_key == start_key { break; }
            let Some(neighbours) = adj.get(&current_key) else { break };
            let Some(&next_idx) = neighbours.iter().find(|&&i| !used[i]) else { break };
            used[next_idx] = true;
            let seg = &segments[next_idx];
            let next_pt = if quant_point(seg.a) == current_key { seg.b } else { seg.a };
            contour.push(next_pt);
            current_key = quant_point(next_pt);
        }

        if contour.len() >= 4 && quant_point(contour[0]) == quant_point(*contour.last().unwrap()) {
            contour.pop(); // drop duplicate closing vertex
            contours.push(contour);
        }
    }

    contours
}

// ────────────────────────────────────────────────────────────────────────
// Tessellation: lyon even-odd fill + world-space transform
// ────────────────────────────────────────────────────────────────────────

fn polygon_area(pts: &[Point2<f32>]) -> f32 {
    let mut sum = 0.0_f32;
    for i in 0..pts.len() {
        let a = pts[i];
        let b = pts[(i + 1) % pts.len()];
        sum += a.x * b.y - b.x * a.y;
    }
    sum * 0.5
}

fn sort_and_bbox(mut contours: Vec<Vec<Point2<f32>>>) -> (Vec<Vec<Point2<f32>>>, BoundingBox) {
    contours.sort_by(|a, b| {
        polygon_area(b).abs()
            .partial_cmp(&polygon_area(a).abs())
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    let mut min = Point2::<f64>::new(f64::MAX, f64::MAX);
    let mut max = Point2::<f64>::new(f64::MIN, f64::MIN);
    for c in &contours {
        for p in c {
            min.x = min.x.min(p.x as f64);
            min.y = min.y.min(p.y as f64);
            max.x = max.x.max(p.x as f64);
            max.y = max.y.max(p.y as f64);
        }
    }
    (contours, BoundingBox { min, max })
}

fn tessellate(
    contours: &[Vec<Point2<f32>>],
    bbox: &BoundingBox,
) -> Option<(Vec<[f32; 2]>, Vec<u32>)> {
    let mut builder = LyonPath::builder();
    for contour in contours {
        if contour.len() < 3 { continue; }
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
        &FillOptions::default().with_fill_rule(FillRule::EvenOdd),
        &mut BuffersBuilder::new(&mut geometry, |v: lyon::tessellation::FillVertex| {
            [v.position().x, v.position().y]
        }),
    ).ok()?;
    if geometry.indices.is_empty() {
        return None;
    }

    // World-space transform: translate so the board's *center* lands at
    // (0, 0), Y-flip around the center so a top-down 3D view matches the
    // 2D viewer's orientation. The gerber's original origin (e.g.
    // alpha_filter's (2995, 0) lower-left) disappears here — the 3D scene
    // is always framed relative to the board itself, not the exporting
    // tool's origin.
    //
    // Centering (vs lower-left at origin) is what makes the default orbit
    // camera frame the board correctly without needing a pan target: the
    // camera already looks at world-origin.
    let cx = ((bbox.min.x + bbox.max.x) * 0.5) as f32;
    let cy = ((bbox.min.y + bbox.max.y) * 0.5) as f32;
    for v in &mut geometry.vertices {
        v[0] -= cx;
        v[1] = cy - v[1];
    }

    Some((geometry.vertices, geometry.indices))
}
