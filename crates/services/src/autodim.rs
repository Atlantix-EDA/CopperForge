//! Board-edge auto-dimensioning.
//!
//! Input: the Edge.Cuts layer as closed contours (mm, any planar frame;
//! output geometry is emitted in the same frame). Output: dimension callouts
//! — extension lines, dimension line, label anchor.
//!
//! Method: the outer contour (largest |signed area|) is cleaned (duplicate
//! points dropped, collinear runs merged), then classified. Circular boards
//! get a diameter callout, elliptical boards get major/minor axis callouts,
//! polygonal boards get overall width/height plus a callout for every edge
//! that terminates in a recognized 90/45/30° corner. Curved outlines arrive
//! flattened (gerber has no arc records after tessellation), so circles are
//! detected by radial fit, not arc metadata.

use serde::{Deserialize, Serialize};

/// Points closer than this are the same point, mm.
const EPS_POINT: f64 = 1e-6;
/// Extension lines overshoot the dimension line by this much, mm.
const EXT_OVERSHOOT: f64 = 0.7;
/// Label sits this far off the dimension line, mm.
const TEXT_LIFT: f64 = 1.2;
/// An edge spanning the full bbox extent within this tolerance is covered
/// by the overall dimension, mm.
const EPS_SPAN: f64 = 1e-3;

/// Auto-dimension tunables. Defaults suit a typical 50–200 mm board.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AutodimOptions {
    /// Distance from an edge to its dimension line, mm.
    pub offset_mm: f64,
    /// Edges shorter than this are not individually dimensioned, mm.
    pub min_edge_mm: f64,
    /// Max deviation when classifying a corner as 90/45/30, degrees.
    pub angle_tol_deg: f64,
    /// Holes at least this large (diameter, mm) are treated as mounting holes
    /// and dimensioned; smaller holes (vias, component pads) are ignored.
    pub mounting_hole_min_mm: f64,
}

impl Default for AutodimOptions {
    fn default() -> Self {
        Self {
            offset_mm: 4.0,
            min_edge_mm: 0.5,
            angle_tol_deg: 1.5,
            mounting_hole_min_mm: 2.0,
        }
    }
}

/// A drilled hole for mounting-hole dimensioning: center + diameter, mm, in
/// the same planar frame as the contours.
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct HoleInput {
    pub center: [f64; 2],
    pub diameter_mm: f64,
}

/// Corner classification at a contour vertex: the deflection between the
/// incoming and outgoing edge directions (a square corner deflects 90°, a
/// 45° chamfer deflects 45°, a shallow 30° chamfer deflects 30°).
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CornerClass {
    Deg90,
    Deg45,
    Deg30,
    Other,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DimensionKind {
    OverallWidth,
    OverallHeight,
    Edge,
    /// Diameter callout for a circular outline (drawn across the center).
    Diameter,
    /// Diameter callout for a mounting hole (leader out to the label).
    HoleDiameter,
    /// Mounting-hole X position from the board's datum (left edge).
    HolePosX,
    /// Mounting-hole Y position from the board's datum (bottom edge).
    HolePosY,
}

/// Recognized overall shape of the board outline.
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq)]
#[serde(tag = "shape", rename_all = "snake_case")]
pub enum OutlineKind {
    Circular { diameter_mm: f64 },
    /// Axis-aligned ellipse (rotated ellipses classify as polygonal).
    Elliptical { major_mm: f64, minor_mm: f64 },
    Polygonal,
}

/// One dimension callout, ready to draw.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DimensionLine {
    pub kind: DimensionKind,
    /// Measured length, mm. The client formats the label text.
    pub value_mm: f64,
    pub ext1: [[f64; 2]; 2],
    pub ext2: [[f64; 2]; 2],
    pub line: [[f64; 2]; 2],
    pub text_pos: [f64; 2],
    /// Label rotation, degrees CCW from +X in the input frame, in (-90, 90].
    pub text_angle_deg: f64,
    /// Corner classification at each end of the measured edge (overall
    /// width/height and diameter dims report `Other`).
    pub corner_start: CornerClass,
    pub corner_end: CornerClass,
}

/// Full auto-dimension result.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AutodimResponse {
    /// What the outer contour is.
    pub outline: OutlineKind,
    pub dimensions: Vec<DimensionLine>,
    /// Input contours ignored (fewer than 3 usable points after cleanup).
    pub skipped_contours: usize,
}

/// Compute dimension callouts for a board outline plus its mounting holes.
pub fn autodimension(
    contours: &[Vec<[f64; 2]>],
    holes: &[HoleInput],
    opts: &AutodimOptions,
) -> AutodimResponse {
    let mut skipped = 0usize;
    let mut cleaned: Vec<Vec<[f64; 2]>> = Vec::new();
    for c in contours {
        let cc = clean_contour(c);
        if cc.len() < 3 {
            skipped += 1;
        } else {
            cleaned.push(cc);
        }
    }
    let Some(outer) = cleaned
        .into_iter()
        .max_by(|a, b| signed_area(a).abs().total_cmp(&signed_area(b).abs()))
    else {
        return AutodimResponse {
            outline: OutlineKind::Polygonal,
            dimensions: Vec::new(),
            skipped_contours: skipped,
        };
    };

    // Mounting-hole dimensions are appended for every outline kind.
    let hole_dims = dimension_holes(&outer, holes, opts);

    let kind = classify_outline(&outer);
    match kind {
        OutlineKind::Circular { diameter_mm } => {
            let (min, max) = bbox(&outer);
            let c = [(min[0] + max[0]) / 2.0, (min[1] + max[1]) / 2.0];
            let r = diameter_mm / 2.0;
            let u = std::f64::consts::FRAC_1_SQRT_2;
            let p = [c[0] - r * u, c[1] - r * u];
            let q = [c[0] + r * u, c[1] + r * u];
            let mut d = linear_dim(
                DimensionKind::Diameter,
                p,
                q,
                [u, -u],
                opts.offset_mm,
                CornerClass::Other,
                CornerClass::Other,
            );
            d.value_mm = diameter_mm;
            let mut dims = vec![d];
            dims.extend(hole_dims);
            return AutodimResponse {
                outline: kind,
                dimensions: dims,
                skipped_contours: skipped,
            };
        }
        OutlineKind::Elliptical { .. } => {
            let (min, max) = bbox(&outer);
            let ov = opts.offset_mm * 2.0;
            let dims = vec![
                linear_dim(
                    DimensionKind::OverallWidth,
                    [min[0], max[1]],
                    [max[0], max[1]],
                    [0.0, 1.0],
                    ov,
                    CornerClass::Other,
                    CornerClass::Other,
                ),
                linear_dim(
                    DimensionKind::OverallHeight,
                    [max[0], min[1]],
                    [max[0], max[1]],
                    [1.0, 0.0],
                    ov,
                    CornerClass::Other,
                    CornerClass::Other,
                ),
            ];
            let mut dims = dims;
            dims.extend(hole_dims);
            return AutodimResponse {
                outline: kind,
                dimensions: dims,
                skipped_contours: skipped,
            };
        }
        OutlineKind::Polygonal => {}
    }

    let n = outer.len();
    let area = signed_area(&outer);
    let corners: Vec<CornerClass> = (0..n)
        .map(|i| {
            classify_corner(
                outer[(i + n - 1) % n],
                outer[i],
                outer[(i + 1) % n],
                opts.angle_tol_deg,
            )
        })
        .collect();

    let (min, max) = bbox(&outer);
    let mut dims: Vec<DimensionLine> = Vec::new();

    let ov = opts.offset_mm * 2.0;
    dims.push(linear_dim(
        DimensionKind::OverallWidth,
        [min[0], max[1]],
        [max[0], max[1]],
        [0.0, 1.0],
        ov,
        CornerClass::Other,
        CornerClass::Other,
    ));
    dims.push(linear_dim(
        DimensionKind::OverallHeight,
        [max[0], min[1]],
        [max[0], max[1]],
        [1.0, 0.0],
        ov,
        CornerClass::Other,
        CornerClass::Other,
    ));

    for i in 0..n {
        let p = outer[i];
        let q = outer[(i + 1) % n];
        let (dx, dy) = (q[0] - p[0], q[1] - p[1]);
        let len = (dx * dx + dy * dy).sqrt();
        if len < opts.min_edge_mm {
            continue;
        }
        let (cs, ce) = (corners[i], corners[(i + 1) % n]);
        if cs == CornerClass::Other && ce == CornerClass::Other {
            continue;
        }
        let axis_x = dy.abs() < EPS_SPAN && (len - (max[0] - min[0])).abs() < EPS_SPAN;
        let axis_y = dx.abs() < EPS_SPAN && (len - (max[1] - min[1])).abs() < EPS_SPAN;
        if axis_x || axis_y {
            continue;
        }
        // Outward normal from winding: CCW (area > 0) keeps interior on the
        // left of travel, so outward is the right normal (dy, -dx).
        let s = if area > 0.0 { 1.0 } else { -1.0 };
        let normal = [s * dy / len, s * -dx / len];
        dims.push(linear_dim(
            DimensionKind::Edge,
            p,
            q,
            normal,
            opts.offset_mm,
            cs,
            ce,
        ));
    }

    dims.extend(hole_dims);
    AutodimResponse {
        outline: OutlineKind::Polygonal,
        dimensions: dims,
        skipped_contours: skipped,
    }
}

/// Dimension every mounting hole (drill diameter ≥ `mounting_hole_min_mm`):
/// a diameter callout across the hole plus its X/Y position from the board
/// datum (bbox min corner). Small holes (vias, pads) are ignored.
fn dimension_holes(
    outer: &[[f64; 2]],
    holes: &[HoleInput],
    opts: &AutodimOptions,
) -> Vec<DimensionLine> {
    let (min, _max) = bbox(outer);
    let mut out = Vec::new();
    for h in holes {
        if h.diameter_mm < opts.mounting_hole_min_mm {
            continue;
        }
        let c = h.center;
        let r = h.diameter_mm / 2.0;
        // Diameter callout: a short 45° line across the hole.
        let u = std::f64::consts::FRAC_1_SQRT_2;
        let p = [c[0] - r * u, c[1] - r * u];
        let q = [c[0] + r * u, c[1] + r * u];
        let mut d = linear_dim(
            DimensionKind::HoleDiameter,
            p,
            q,
            [u, -u],
            opts.offset_mm,
            CornerClass::Other,
            CornerClass::Other,
        );
        d.value_mm = h.diameter_mm;
        out.push(d);

        // X position: horizontal from the left datum to the hole center.
        let mut dx = linear_dim(
            DimensionKind::HolePosX,
            [min[0], c[1]],
            [c[0], c[1]],
            [0.0, -1.0],
            opts.offset_mm,
            CornerClass::Other,
            CornerClass::Other,
        );
        dx.value_mm = c[0] - min[0];
        out.push(dx);

        // Y position: vertical from the bottom datum to the hole center.
        let mut dy = linear_dim(
            DimensionKind::HolePosY,
            [c[0], min[1]],
            [c[0], c[1]],
            [-1.0, 0.0],
            opts.offset_mm,
            CornerClass::Other,
            CornerClass::Other,
        );
        dy.value_mm = c[1] - min[1];
        out.push(dy);
    }
    out
}

/// Drop consecutive duplicates (and a repeated closing point), then merge
/// collinear runs so a subdivided straight edge dimensions as one edge.
fn clean_contour(points: &[[f64; 2]]) -> Vec<[f64; 2]> {
    let mut pts: Vec<[f64; 2]> = Vec::with_capacity(points.len());
    for &p in points {
        if pts.last().map_or(true, |&l| dist(l, p) > EPS_POINT) {
            pts.push(p);
        }
    }
    if pts.len() >= 2 && dist(pts[0], *pts.last().unwrap()) <= EPS_POINT {
        pts.pop();
    }
    loop {
        let n = pts.len();
        if n < 3 {
            return pts;
        }
        let mut keep: Vec<[f64; 2]> = Vec::with_capacity(n);
        for i in 0..n {
            let a = pts[(i + n - 1) % n];
            let b = pts[i];
            let c = pts[(i + 1) % n];
            let (ux, uy) = (b[0] - a[0], b[1] - a[1]);
            let (vx, vy) = (c[0] - b[0], c[1] - b[1]);
            let cross = ux * vy - uy * vx;
            let dot = ux * vx + uy * vy;
            let scale = (ux * ux + uy * uy).sqrt() * (vx * vx + vy * vy).sqrt();
            if scale > 0.0 && cross.abs() / scale < 1e-9 && dot > 0.0 {
                continue;
            }
            keep.push(b);
        }
        let stable = keep.len() == pts.len();
        pts = keep;
        if stable {
            return pts;
        }
    }
}

/// Deflection-based corner class at vertex `b` between edges a→b and b→c.
fn classify_corner(a: [f64; 2], b: [f64; 2], c: [f64; 2], tol_deg: f64) -> CornerClass {
    let (ux, uy) = (b[0] - a[0], b[1] - a[1]);
    let (vx, vy) = (c[0] - b[0], c[1] - b[1]);
    let lu = (ux * ux + uy * uy).sqrt();
    let lv = (vx * vx + vy * vy).sqrt();
    if lu <= 0.0 || lv <= 0.0 {
        return CornerClass::Other;
    }
    let cos = ((ux * vx + uy * vy) / (lu * lv)).clamp(-1.0, 1.0);
    let deflection = cos.acos().to_degrees();
    for (target, class) in [
        (90.0, CornerClass::Deg90),
        (45.0, CornerClass::Deg45),
        (30.0, CornerClass::Deg30),
    ] {
        if (deflection - target).abs() <= tol_deg {
            return class;
        }
    }
    CornerClass::Other
}

/// Build one linear dimension for the segment p→q, offset along `normal`.
/// Build an aligned linear dimension between two arbitrary points — the
/// manual-dimensioning constructor. The dimension line is parallel to `p`–`q`,
/// offset to one side; a zero offset keeps it on the measured line and the
/// user drags it out.
pub fn aligned_dimension(p: [f64; 2], q: [f64; 2], offset_mm: f64) -> DimensionLine {
    let (dx, dy) = (q[0] - p[0], q[1] - p[1]);
    let l = (dx * dx + dy * dy).sqrt();
    let normal = if l < 1e-9 { [0.0, 1.0] } else { [dy / l, -dx / l] };
    linear_dim(
        DimensionKind::Edge,
        p,
        q,
        normal,
        offset_mm,
        CornerClass::Other,
        CornerClass::Other,
    )
}

fn linear_dim(
    kind: DimensionKind,
    p: [f64; 2],
    q: [f64; 2],
    normal: [f64; 2],
    offset: f64,
    corner_start: CornerClass,
    corner_end: CornerClass,
) -> DimensionLine {
    let off = |pt: [f64; 2], k: f64| [pt[0] + normal[0] * k, pt[1] + normal[1] * k];
    let d1 = off(p, offset);
    let d2 = off(q, offset);
    let mid = [(d1[0] + d2[0]) / 2.0, (d1[1] + d2[1]) / 2.0];
    let mut angle = (q[1] - p[1]).atan2(q[0] - p[0]).to_degrees();
    if angle <= -90.0 {
        angle += 180.0;
    } else if angle > 90.0 {
        angle -= 180.0;
    }
    DimensionLine {
        kind,
        value_mm: dist(p, q),
        ext1: [p, off(p, offset + EXT_OVERSHOOT)],
        ext2: [q, off(q, offset + EXT_OVERSHOOT)],
        line: [d1, d2],
        text_pos: off(mid, TEXT_LIFT),
        text_angle_deg: angle,
        corner_start,
        corner_end,
    }
}

/// Radial-fit shape classification of a cleaned outline.
fn classify_outline(pts: &[[f64; 2]]) -> OutlineKind {
    const MIN_ARC_POINTS: usize = 16;
    const REL_TOL: f64 = 0.01;
    if pts.len() < MIN_ARC_POINTS {
        return OutlineKind::Polygonal;
    }
    let (min, max) = bbox(pts);
    let c = [(min[0] + max[0]) / 2.0, (min[1] + max[1]) / 2.0];
    let (a, b) = ((max[0] - min[0]) / 2.0, (max[1] - min[1]) / 2.0);
    if a <= 0.0 || b <= 0.0 {
        return OutlineKind::Polygonal;
    }
    let mut worst = 0.0f64;
    for p in pts {
        let nx = (p[0] - c[0]) / a;
        let ny = (p[1] - c[1]) / b;
        let r = (nx * nx + ny * ny).sqrt();
        worst = worst.max((r - 1.0).abs());
    }
    if worst > REL_TOL {
        return OutlineKind::Polygonal;
    }
    if ((a - b).abs() / a.max(b)) <= REL_TOL {
        OutlineKind::Circular {
            diameter_mm: a + b, // mean diameter
        }
    } else {
        OutlineKind::Elliptical {
            major_mm: 2.0 * a.max(b),
            minor_mm: 2.0 * a.min(b),
        }
    }
}

fn dist(a: [f64; 2], b: [f64; 2]) -> f64 {
    ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2)).sqrt()
}

fn signed_area(pts: &[[f64; 2]]) -> f64 {
    let n = pts.len();
    let mut s = 0.0;
    for i in 0..n {
        let p = pts[i];
        let q = pts[(i + 1) % n];
        s += p[0] * q[1] - q[0] * p[1];
    }
    s / 2.0
}

fn bbox(pts: &[[f64; 2]]) -> ([f64; 2], [f64; 2]) {
    let mut min = [f64::INFINITY; 2];
    let mut max = [f64::NEG_INFINITY; 2];
    for p in pts {
        for k in 0..2 {
            min[k] = min[k].min(p[k]);
            max[k] = max[k].max(p[k]);
        }
    }
    (min, max)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn opts() -> AutodimOptions {
        AutodimOptions::default()
    }
    fn kinds(r: &AutodimResponse) -> Vec<DimensionKind> {
        r.dimensions.iter().map(|d| d.kind).collect()
    }
    fn arc(cx: f64, cy: f64, rx: f64, ry: f64, n: usize) -> Vec<[f64; 2]> {
        (0..n)
            .map(|i| {
                let t = i as f64 / n as f64 * std::f64::consts::TAU;
                [cx + rx * t.cos(), cy + ry * t.sin()]
            })
            .collect()
    }

    #[test]
    fn rectangle_gets_overall_dims_only() {
        let rect = vec![[0.0, 0.0], [100.0, 0.0], [100.0, 60.0], [0.0, 60.0]];
        let r = autodimension(&[rect], &[], &opts());
        assert_eq!(
            kinds(&r),
            vec![DimensionKind::OverallWidth, DimensionKind::OverallHeight]
        );
        assert!((r.dimensions[0].value_mm - 100.0).abs() < 1e-9);
        assert!((r.dimensions[1].value_mm - 60.0).abs() < 1e-9);
    }

    #[test]
    fn mounting_holes_dimensioned_small_holes_ignored() {
        let rect = vec![[0.0, 0.0], [100.0, 0.0], [100.0, 60.0], [0.0, 60.0]];
        let holes = [
            HoleInput { center: [5.0, 5.0], diameter_mm: 3.2 }, // M3 mounting hole
            HoleInput { center: [50.0, 30.0], diameter_mm: 0.3 }, // via — ignored
        ];
        let r = autodimension(&[rect], &holes, &opts());
        // 2 overall + 3 for the one mounting hole (⌀, X, Y).
        let hole_dims: Vec<_> = r
            .dimensions
            .iter()
            .filter(|d| {
                matches!(
                    d.kind,
                    DimensionKind::HoleDiameter
                        | DimensionKind::HolePosX
                        | DimensionKind::HolePosY
                )
            })
            .collect();
        assert_eq!(hole_dims.len(), 3);
        let dia = hole_dims
            .iter()
            .find(|d| d.kind == DimensionKind::HoleDiameter)
            .unwrap();
        assert!((dia.value_mm - 3.2).abs() < 1e-9);
        let px = hole_dims
            .iter()
            .find(|d| d.kind == DimensionKind::HolePosX)
            .unwrap();
        assert!((px.value_mm - 5.0).abs() < 1e-9);
    }

    #[test]
    fn chamfer_45_is_detected() {
        let c = vec![
            [0.0, 0.0],
            [90.0, 0.0],
            [100.0, 10.0],
            [100.0, 60.0],
            [0.0, 60.0],
        ];
        let r = autodimension(&[c], &[], &opts());
        let chamfer = r
            .dimensions
            .iter()
            .find(|d| (d.value_mm - 200.0f64.sqrt()).abs() < 1e-6)
            .expect("chamfer edge dimensioned");
        assert_eq!(chamfer.corner_start, CornerClass::Deg45);
        assert_eq!(chamfer.corner_end, CornerClass::Deg45);
    }

    #[test]
    fn circular_outline_gets_diameter_only() {
        let r = autodimension(&[arc(40.0, 40.0, 40.0, 40.0, 64)], &[], &opts());
        match r.outline {
            OutlineKind::Circular { diameter_mm } => {
                assert!((diameter_mm - 80.0).abs() < 0.4)
            }
            other => panic!("expected circular, got {other:?}"),
        }
        assert_eq!(r.dimensions.len(), 1);
        assert_eq!(r.dimensions[0].kind, DimensionKind::Diameter);
    }

    #[test]
    fn elliptical_outline_gets_axis_dims() {
        let r = autodimension(&[arc(0.0, 0.0, 50.0, 30.0, 64)], &[], &opts());
        match r.outline {
            OutlineKind::Elliptical { major_mm, minor_mm } => {
                assert!((major_mm - 100.0).abs() < 0.5);
                assert!((minor_mm - 60.0).abs() < 0.5);
            }
            other => panic!("expected elliptical, got {other:?}"),
        }
        assert_eq!(r.dimensions.len(), 2);
    }

    #[test]
    fn octagon_is_polygonal_not_circular() {
        let r = autodimension(&[arc(0.0, 0.0, 40.0, 40.0, 8)], &[], &opts());
        assert_eq!(r.outline, OutlineKind::Polygonal);
    }

    #[test]
    fn degenerate_contours_are_skipped() {
        let r = autodimension(&[vec![[0.0, 0.0], [1.0, 1.0]], vec![]], &[], &opts());
        assert_eq!(r.dimensions.len(), 0);
        assert_eq!(r.skipped_contours, 2);
    }
}
