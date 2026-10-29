//! Exposed copper = mask openings ∩ copper (real 2D boolean).
//!
//! The 3D view's "exposed copper" sheet used to be the mask openings
//! tessellated verbatim — correct for an 0603 pad where opening ≈ pad, but
//! wrong everywhere the opening outruns the metal: mounting-hole openings
//! painted solid copper over the bore, oversized openings (mask expansion)
//! painted copper over bare laminate, and guard-band reliefs painted copper
//! over nothing. The fix is the actual intersection: copper colour appears
//! exactly where an opening overlaps copper, and nowhere else.
//!
//! lyon can only tessellate — it has no clipping. The boolean runs on
//! `i_overlay` (the engine behind georust/geo's booleans): both inputs
//! under `FillRule::NonZero` so overlapping pads / pours union instead of
//! cancelling, `OverlayRule::Intersect` for the clip. The resulting shapes
//! (outer contours CCW, holes CW, mutually disjoint) are tessellated with
//! lyon under even-odd — exact for disjoint rings, winding-agnostic.

use gerber_viewer::BoundingBox;
use i_overlay::core::fill_rule::FillRule as OverlayFillRule;
use i_overlay::core::overlay_rule::OverlayRule;
use i_overlay::float::single::SingleFloatOverlay;
use lyon::math::Point as LyonPoint;
use lyon::path::Path as LyonPath;
use lyon::tessellation::{
    BuffersBuilder, FillOptions, FillRule, FillTessellator, VertexBuffers,
};
use nalgebra::Point2;

use super::drill::Hole;

/// Max chord error (mm) for hole punch discs and via barrels. 4 µm keeps a
/// bore rim visually round even zoomed to a single pad, without exploding
/// the vertex count on hole-dense boards.
const HOLE_CHORD_TOL_MM: f32 = 0.004;

/// Chord-error-adaptive segment count for a hole of `radius` mm. The punch
/// disc and the 3D barrel share this (same angular sampling), so the barrel
/// wall meets the punched sheet rim vertex-for-vertex. A fixed low count
/// renders as a visible polygon the moment the camera zooms to pad scale —
/// the segment count must scale with radius, not be a constant.
pub fn hole_segments(radius: f32) -> usize {
    if radius <= HOLE_CHORD_TOL_MM * 2.0 {
        return 24;
    }
    // Sagitta e = r·(1 − cos(θ/2)) → chord angle θ for error e.
    let theta = 2.0 * (1.0 - HOLE_CHORD_TOL_MM / radius).acos();
    ((std::f32::consts::TAU / theta).ceil() as usize).clamp(24, 96)
}

/// Intersect mask-opening contours with copper contours and tessellate the
/// result. Both inputs are raw gerber-space loops (mm, uncentered) from the
/// same `walk_copper` walker, so they share a frame by construction; the
/// output vertices are centered on `outline_bbox` to match the board /
/// copper / mask meshes. Returns empty buffers when nothing overlaps (e.g.
/// openings only over bare laminate) — the caller treats that as "no
/// exposed copper".
pub fn exposed_copper_mesh(
    openings: &[Vec<Point2<f32>>],
    copper: &[Vec<Point2<f32>>],
    holes: &[Hole],
    outline_bbox: &BoundingBox,
) -> (Vec<[f32; 2]>, Vec<u32>) {
    let subj = to_overlay_contours(openings);
    let clip = to_overlay_contours(copper);
    if subj.is_empty() || clip.is_empty() {
        return (Vec::new(), Vec::new());
    }

    let mut shapes = subj.overlay(&clip, OverlayRule::Intersect, OverlayFillRule::NonZero);
    shapes = punch_holes(shapes, holes, outline_bbox);
    tessellate_shapes(&shapes, outline_bbox)
}

/// Copper sheet with the drill holes punched out: union of the layer's
/// contours minus every hole disc. A flashed pad is a FULL disc in the
/// copper gerber — the bore only exists in the drill files — so without the
/// punch, copper paints over the hole mouth and the barrel can't read as a
/// hole. Raw-frame input contours, bbox-centered output (like everything
/// else here).
pub fn punched_copper_mesh(
    copper: &[Vec<Point2<f32>>],
    holes: &[Hole],
    outline_bbox: &BoundingBox,
) -> (Vec<[f32; 2]>, Vec<u32>) {
    let subj = to_overlay_contours(copper);
    if subj.is_empty() {
        return (Vec::new(), Vec::new());
    }
    // Union-normalize even when there's nothing to punch, so the output is
    // winding-clean either way. NOTE: an overlay against an EMPTY clip
    // returns nothing — normalization without a clip must go through
    // `simplify_shape`, not `overlay`.
    let shapes = if holes.is_empty() {
        use i_overlay::float::simplify::SimplifyShape;
        subj.simplify_shape(OverlayFillRule::NonZero)
    } else {
        subj.overlay(
            &hole_discs_raw(holes, outline_bbox),
            OverlayRule::Difference,
            OverlayFillRule::NonZero,
        )
    };
    tessellate_shapes(&shapes, outline_bbox)
}

/// Board-outline sheet (caps) with drill holes punched out. Outline loop 0
/// is the outer boundary (largest-first per the stitcher's contract),
/// remaining loops are board cutouts.
pub fn punched_outline_mesh(
    outline: &[Vec<Point2<f32>>],
    holes: &[Hole],
    outline_bbox: &BoundingBox,
) -> (Vec<[f32; 2]>, Vec<u32>) {
    let mut subj: Vec<Vec<[f32; 2]>> = Vec::new();
    for (i, c) in outline.iter().enumerate() {
        if c.len() < 3 {
            continue;
        }
        let mut ring: Vec<[f32; 2]> = c.iter().map(|p| [p.x, p.y]).collect();
        let ccw = signed_area(&ring) >= 0.0;
        if (i == 0) != ccw {
            ring.reverse();
        }
        subj.push(ring);
    }
    if subj.is_empty() {
        return (Vec::new(), Vec::new());
    }
    let shapes = if holes.is_empty() {
        vec![subj]
    } else {
        subj.overlay(
            &hole_discs_raw(holes, outline_bbox),
            OverlayRule::Difference,
            OverlayFillRule::NonZero,
        )
    };
    tessellate_shapes(&shapes, outline_bbox)
}

/// Subtract hole discs from already-clean boolean output shapes.
fn punch_holes(
    shapes: Vec<Vec<Vec<[f32; 2]>>>,
    holes: &[Hole],
    outline_bbox: &BoundingBox,
) -> Vec<Vec<Vec<[f32; 2]>>> {
    if holes.is_empty() || shapes.is_empty() {
        return shapes;
    }
    shapes.overlay(
        &hole_discs_raw(holes, outline_bbox),
        OverlayRule::Difference,
        OverlayFillRule::NonZero,
    )
}

/// Hole punch discs as CCW polygons in the RAW gerber frame. `Hole.center`
/// is stored bbox-centred (world frame), while the contours being punched
/// are raw — shift the centres back.
fn hole_discs_raw(holes: &[Hole], outline_bbox: &BoundingBox) -> Vec<Vec<[f32; 2]>> {
    let cx = ((outline_bbox.min.x + outline_bbox.max.x) * 0.5) as f32;
    let cy = ((outline_bbox.min.y + outline_bbox.max.y) * 0.5) as f32;
    holes
        .iter()
        .filter(|h| h.radius > 0.0)
        .map(|h| {
            let seg = hole_segments(h.radius);
            (0..seg)
                .map(|i| {
                    let a = (i as f32 / seg as f32) * std::f32::consts::TAU;
                    [
                        h.center[0] + cx + h.radius * a.cos(),
                        h.center[1] + cy + h.radius * a.sin(),
                    ]
                })
                .collect()
        })
        .collect()
}

/// Walker contours → i_overlay contours, every loop normalized CCW.
///
/// NonZero semantics union overlapping contours ONLY when they wind the
/// same way; a flashed pad and a G36 zone region can come out of the
/// gerber with opposite windings, and then their overlap sums to winding
/// zero — i.e. a pad intersecting a pour would read as a HOLE. The walker
/// emits only solid filled outlines (no intentional hole loops), so
/// forcing every loop CCW is lossless and makes the union winding-proof.
fn to_overlay_contours(contours: &[Vec<Point2<f32>>]) -> Vec<Vec<[f32; 2]>> {
    contours
        .iter()
        .filter(|c| c.len() >= 3)
        .map(|c| {
            let mut ring: Vec<[f32; 2]> = c.iter().map(|p| [p.x, p.y]).collect();
            if signed_area(&ring) < 0.0 {
                ring.reverse();
            }
            ring
        })
        .collect()
}

/// Shoelace signed area: positive = CCW.
pub(crate) fn signed_area(ring: &[[f32; 2]]) -> f32 {
    let mut a = 0.0f32;
    for i in 0..ring.len() {
        let p = ring[i];
        let q = ring[(i + 1) % ring.len()];
        a += p[0] * q[1] - q[0] * p[1];
    }
    a * 0.5
}

/// Tessellate i_overlay boolean output (disjoint shapes: outer CCW, holes
/// CW) with lyon under even-odd — exact for disjoint rings and agnostic to
/// their winding. Vertices come out centered on the outline bbox, matching
/// the board / copper / mask meshes.
pub(crate) fn tessellate_shapes(
    shapes: &[Vec<Vec<[f32; 2]>>],
    outline_bbox: &BoundingBox,
) -> (Vec<[f32; 2]>, Vec<u32>) {
    let mut builder = LyonPath::builder();
    let mut any = false;
    for shape in shapes {
        for ring in shape {
            if ring.len() < 3 {
                continue;
            }
            builder.begin(LyonPoint::new(ring[0][0], ring[0][1]));
            for p in &ring[1..] {
                builder.line_to(LyonPoint::new(p[0], p[1]));
            }
            builder.close();
            any = true;
        }
    }
    if !any {
        return (Vec::new(), Vec::new());
    }
    let path = builder.build();

    let mut geometry: VertexBuffers<[f32; 2], u32> = VertexBuffers::new();
    let mut tess = FillTessellator::new();
    if tess
        .tessellate_path(
            &path,
            &FillOptions::default().with_fill_rule(FillRule::EvenOdd),
            &mut BuffersBuilder::new(&mut geometry, |v: lyon::tessellation::FillVertex| {
                [v.position().x, v.position().y]
            }),
        )
        .is_err()
    {
        return (Vec::new(), Vec::new());
    }

    let cx = ((outline_bbox.min.x + outline_bbox.max.x) * 0.5) as f32;
    let cy = ((outline_bbox.min.y + outline_bbox.max.y) * 0.5) as f32;
    for v in &mut geometry.vertices {
        v[0] -= cx;
        v[1] -= cy;
    }

    (geometry.vertices, geometry.indices)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn square(cx: f32, cy: f32, half: f32) -> Vec<Point2<f32>> {
        vec![
            Point2::new(cx - half, cy - half),
            Point2::new(cx + half, cy - half),
            Point2::new(cx + half, cy + half),
            Point2::new(cx - half, cy + half),
        ]
    }

    fn bbox() -> BoundingBox {
        BoundingBox {
            min: nalgebra::Point2::new(0.0, 0.0),
            max: nalgebra::Point2::new(0.0, 0.0),
        }
    }

    /// Unsigned area of the tessellated mesh.
    fn mesh_area(verts: &[[f32; 2]], indices: &[u32]) -> f32 {
        indices
            .chunks_exact(3)
            .map(|t| {
                let a = verts[t[0] as usize];
                let b = verts[t[1] as usize];
                let c = verts[t[2] as usize];
                ((b[0] - a[0]) * (c[1] - a[1]) - (c[0] - a[0]) * (b[1] - a[1])).abs() * 0.5
            })
            .sum()
    }

    #[test]
    fn opening_clipped_to_copper() {
        // 10×10 opening over a 6×6 pad: exposed = the pad, not the opening.
        let openings = vec![square(0.0, 0.0, 5.0)];
        let copper = vec![square(0.0, 0.0, 3.0)];
        let (v, i) = exposed_copper_mesh(&openings, &copper, &[], &bbox());
        assert!((mesh_area(&v, &i) - 36.0).abs() < 1e-3);
    }

    #[test]
    fn opening_over_bare_laminate_is_empty() {
        // Opening with no copper beneath (guard-band relief, NPTH window):
        // nothing to expose.
        let openings = vec![square(0.0, 0.0, 5.0)];
        let copper = vec![square(20.0, 20.0, 3.0)];
        let (v, i) = exposed_copper_mesh(&openings, &copper, &[], &bbox());
        assert!(v.is_empty() && i.is_empty());
    }

    #[test]
    fn overlapping_pads_union_under_one_opening() {
        // Two overlapping 4×4 pads under a big opening: exposed area is the
        // union (16 + 16 − 8 overlap = 24), not double-counted or cancelled.
        let openings = vec![square(1.0, 0.0, 6.0)];
        let copper = vec![square(0.0, 0.0, 2.0), square(2.0, 0.0, 2.0)];
        let (v, i) = exposed_copper_mesh(&openings, &copper, &[], &bbox());
        assert!((mesh_area(&v, &i) - 24.0).abs() < 1e-3);
    }

    #[test]
    fn opposite_winding_contours_still_union() {
        // A flashed pad (CCW) and a zone region (CW) overlapping under one
        // big opening: NonZero would cancel their overlap if windings were
        // trusted — normalization must make this identical to the same-
        // winding case (union area 24).
        let openings = vec![square(1.0, 0.0, 6.0)];
        let mut zone = square(2.0, 0.0, 2.0);
        zone.reverse(); // CW
        let copper = vec![square(0.0, 0.0, 2.0), zone];
        let (v, i) = exposed_copper_mesh(&openings, &copper, &[], &bbox());
        assert!((mesh_area(&v, &i) - 24.0).abs() < 1e-3);
    }

    /// Area of the n-gon punch disc for a hole of radius r.
    fn disc_area(r: f32) -> f32 {
        let n = hole_segments(r) as f32;
        0.5 * n * r * r * (std::f32::consts::TAU / n).sin()
    }

    #[test]
    fn drill_holes_punched_from_exposed() {
        // 6×6 pad fully exposed, 1 mm-radius hole in the middle: the bore is
        // a real void (an n-gon from the adaptive segment count).
        let openings = vec![square(0.0, 0.0, 5.0)];
        let copper = vec![square(0.0, 0.0, 3.0)];
        let holes = [Hole { center: [0.0, 0.0], radius: 1.0, plated: true }];
        let (v, i) = exposed_copper_mesh(&openings, &copper, &holes, &bbox());
        assert!((mesh_area(&v, &i) - (36.0 - disc_area(1.0))).abs() < 1e-2);
    }

    #[test]
    fn punched_copper_and_outline() {
        let copper = vec![square(0.0, 0.0, 3.0)];
        let outline = vec![square(0.0, 0.0, 10.0)];
        let holes = [Hole { center: [0.0, 0.0], radius: 1.0, plated: false }];
        let disc = disc_area(1.0);
        let (v, i) = punched_copper_mesh(&copper, &holes, &bbox());
        assert!((mesh_area(&v, &i) - (36.0 - disc)).abs() < 1e-2);
        let (v, i) = punched_outline_mesh(&outline, &holes, &bbox());
        assert!((mesh_area(&v, &i) - (400.0 - disc)).abs() < 1e-2);
    }

    #[test]
    fn hole_segments_scale_with_radius() {
        // Small via stays at the floor; a mounting hole gets visibly more
        // segments; a huge bore caps at 96.
        assert_eq!(hole_segments(0.01), 24);
        assert!(hole_segments(0.15) >= 24);
        let mounting = hole_segments(1.6);
        assert!(mounting > hole_segments(0.3));
        assert!(mounting >= 32);
        assert_eq!(hole_segments(500.0), 96);
    }

    #[test]
    fn centered_on_outline_bbox() {
        let openings = vec![square(10.0, 10.0, 2.0)];
        let copper = vec![square(10.0, 10.0, 2.0)];
        let b = BoundingBox {
            min: nalgebra::Point2::new(0.0, 0.0),
            max: nalgebra::Point2::new(20.0, 20.0),
        };
        let (v, _) = exposed_copper_mesh(&openings, &copper, &[], &b);
        // bbox centre (10,10) maps to origin.
        let max_abs = v
            .iter()
            .flat_map(|p| p.iter())
            .fold(0.0f32, |m, &x| m.max(x.abs()));
        assert!((max_abs - 2.0).abs() < 1e-4);
    }
}
