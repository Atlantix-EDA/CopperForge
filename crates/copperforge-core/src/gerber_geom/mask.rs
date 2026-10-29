//! Solder-mask layer geometry extraction (Phase 4b).
//!
//! Sibling to `extract_copper`. Re-uses the same gerber walker — F.Mask /
//! B.Mask gerbers expose the identical primitive set (flashed apertures,
//! stroked paths, G36/G37 regions). What differs is the *interpretation*:
//!
//! - Copper gerber: filled areas = copper present. Tessellate directly.
//! - Mask gerber: filled areas = mask *absent* (pad / via openings).
//!   The 3D mesh we want is "board-outline-shaped green sheet with those
//!   openings punched out."
//!
//! To get that in one tessellation pass, we feed lyon:
//!
//! 1. The board outline's stitched contours (outer boundary + any board
//!    cutouts / slots).
//! 2. The mask gerber's opening contours.
//!
//! ...with `FillRule::EvenOdd`. Under even-odd, a point is filled iff it
//! lies inside an odd number of contours: inside the outline + not inside
//! any hole → filled (mask coverage); inside the outline + inside an
//! opening → unfilled (hole in mask). No boolean op, no CSG pass.

use std::fs::File;
use std::io::{BufReader, Cursor, Read};
use std::path::Path;

use gerber_parser::parse;
use gerber_types::Unit;
use gerber_viewer::BoundingBox;
use nalgebra::Point2;

use super::copper::{walk_copper, CopperCounts};

/// Tessellated soldermask mesh for one side of the board. Structurally
/// identical to `CopperData`; kept as a distinct type so the renderer can't
/// accidentally cross-wire a mask mesh into a copper slot (or vice versa).
#[derive(Debug, Clone)]
pub struct MaskData {
    pub mesh_vertices_2d: Vec<[f32; 2]>,
    pub mesh_indices: Vec<u32>,
    /// The mask *openings* (pad / via apertures) as raw closed contours
    /// (mm, gerber coord space, uncentered — the `walk_copper` output).
    /// The 3D view intersects these against the copper layer's contours
    /// (`gerber_geom::exposed`) to build the exposed-copper sheet: copper
    /// colour exactly where an opening overlaps metal, and nowhere else.
    /// Empty when the mask has no openings (fully tented).
    pub opening_contours: Vec<Vec<Point2<f32>>>,
}

/// Primitive counts from the mask gerber walk. Same breakdown as
/// `CopperCounts` — re-exported here so call-site logging reads as
/// "mask circles / mask rects / ..." rather than a leaky copper type.
pub type MaskCounts = CopperCounts;

/// Parse + tessellate a soldermask gerber into a green-sheet-with-holes
/// mesh. `outline_contours` are the board-outline stitched loops (largest
/// first) and `outline_bbox` is their bbox — we re-use the outline's world
/// transform so mask, copper, and board mesh all share coordinates.
///
/// Returns `None` if the file can't be opened, the parse is catastrophically
/// broken, or (no outline + no openings) would produce an empty mesh. A
/// zero-opening mask is still valid and returns a full green slab.
pub fn extract_mask(
    gerber_path: &Path,
    outline_contours: &[Vec<Point2<f32>>],
    outline_bbox: &BoundingBox,
) -> Option<(MaskData, MaskCounts)> {
    let file = File::open(gerber_path).ok()?;
    extract_mask_from_reader(BufReader::new(file), outline_contours, outline_bbox)
}

/// In-memory variant for wasm32 — see [`super::extract_outline_from_bytes`].
pub fn extract_mask_from_bytes(
    bytes: &[u8],
    outline_contours: &[Vec<Point2<f32>>],
    outline_bbox: &BoundingBox,
) -> Option<(MaskData, MaskCounts)> {
    extract_mask_from_reader(BufReader::new(Cursor::new(bytes)), outline_contours, outline_bbox)
}

/// Shared core: parse from any buffered reader, walk openings, tessellate
/// the green-sheet-with-holes against the board outline.
fn extract_mask_from_reader<R: Read>(
    reader: BufReader<R>,
    outline_contours: &[Vec<Point2<f32>>],
    outline_bbox: &BoundingBox,
) -> Option<(MaskData, MaskCounts)> {
    let doc = match parse(reader) {
        Ok(d) => d,
        Err((d, _)) => d,
    };
    let unit_scale = match doc.units {
        Some(Unit::Millimeters) => 1.0_f64,
        Some(Unit::Inches) => 25.4_f64,
        None => 1.0_f64,
    };

    let (opening_contours, counts) = walk_copper(&doc, unit_scale);

    if outline_contours.is_empty() {
        // No outer boundary → no sheet to punch holes in. The copper-style
        // "just tessellate the openings" would draw isolated green disks at
        // every pad, which isn't soldermask — it's the inverse.
        return None;
    }

    let (mesh_vertices_2d, mesh_indices) =
        tessellate_with_holes(outline_contours, &opening_contours, outline_bbox)?;

    Some((
        MaskData {
            mesh_vertices_2d,
            mesh_indices,
            opening_contours,
        },
        counts,
    ))
}

/// The green sheet = board outline MINUS the union of the openings — a real
/// boolean (`i_overlay`), not the old "outline + openings under even-odd"
/// lyon trick. Even-odd counts containment *parity*, so wherever two
/// openings overlapped (a mounting-hole pad opening intersecting the
/// guard-band zone opening, overlapping connector pads…) the parity flipped
/// back to "filled" and the mask repainted itself over what the fab would
/// leave bare. The boolean unions the openings first, so N overlapping
/// openings subtract exactly once.
///
/// Winding is normalized explicitly: outline loop 0 (the outer boundary,
/// largest first per the stitcher's contract) CCW, remaining outline loops
/// (board cutouts) CW so NonZero reads them as holes, and every opening CCW
/// so overlapping openings union instead of cancelling.
///
/// World transform matches `extract_copper`: shift by outline-bbox center,
/// no Y-flip (gerber is Y-up per §4.2.3). This locks the mask mesh into
/// exact pixel-alignment with the copper + board meshes.
fn tessellate_with_holes(
    outline: &[Vec<Point2<f32>>],
    openings: &[Vec<Point2<f32>>],
    outline_bbox: &BoundingBox,
) -> Option<(Vec<[f32; 2]>, Vec<u32>)> {
    use i_overlay::core::fill_rule::FillRule as OverlayFillRule;
    use i_overlay::core::overlay_rule::OverlayRule;
    use i_overlay::float::single::SingleFloatOverlay;

    let mut subj: Vec<Vec<[f32; 2]>> = Vec::new();
    for (i, c) in outline.iter().enumerate() {
        if c.len() < 3 {
            continue;
        }
        let mut ring: Vec<[f32; 2]> = c.iter().map(|p| [p.x, p.y]).collect();
        let ccw = super::exposed::signed_area(&ring) >= 0.0;
        // Outer boundary CCW; cutout loops CW (NonZero holes).
        if (i == 0) != ccw {
            ring.reverse();
        }
        subj.push(ring);
    }
    if subj.is_empty() {
        return None;
    }

    let mut clip: Vec<Vec<[f32; 2]>> = Vec::new();
    for c in openings {
        if c.len() < 3 {
            continue;
        }
        let mut ring: Vec<[f32; 2]> = c.iter().map(|p| [p.x, p.y]).collect();
        if super::exposed::signed_area(&ring) < 0.0 {
            ring.reverse();
        }
        clip.push(ring);
    }

    let shapes = if clip.is_empty() {
        // Fully-tented mask: the sheet is just the (normalized) outline.
        vec![subj]
    } else {
        subj.overlay(&clip, OverlayRule::Difference, OverlayFillRule::NonZero)
    };

    let (vertices, indices) = super::exposed::tessellate_shapes(&shapes, outline_bbox);
    if indices.is_empty() {
        return None;
    }
    Some((vertices, indices))
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

    fn bbox(half: f64) -> BoundingBox {
        BoundingBox {
            min: nalgebra::Point2::new(-half, -half),
            max: nalgebra::Point2::new(half, half),
        }
    }

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

    /// The mounting-hole / guard-band case: two OVERLAPPING openings. The
    /// old even-odd parity trick repainted mask over the overlap; the
    /// boolean must subtract the union exactly once.
    #[test]
    fn overlapping_openings_subtract_once() {
        let outline = vec![square(0.0, 0.0, 10.0)]; // area 400
        // Two 4×4 openings overlapping by 2×4=8 → union 24 → mask 376.
        let openings = vec![square(0.0, 0.0, 2.0), square(2.0, 0.0, 2.0)];
        let (v, i) = tessellate_with_holes(&outline, &openings, &bbox(10.0)).unwrap();
        assert!((mesh_area(&v, &i) - 376.0).abs() < 1e-2);
    }

    /// Same, with the second opening wound CW (zone-region style): the
    /// normalization must make winding irrelevant.
    #[test]
    fn overlapping_openings_mixed_winding() {
        let outline = vec![square(0.0, 0.0, 10.0)];
        let mut zone = square(2.0, 0.0, 2.0);
        zone.reverse();
        let openings = vec![square(0.0, 0.0, 2.0), zone];
        let (v, i) = tessellate_with_holes(&outline, &openings, &bbox(10.0)).unwrap();
        assert!((mesh_area(&v, &i) - 376.0).abs() < 1e-2);
    }

    /// Fully-tented mask (no openings) stays a full slab.
    #[test]
    fn tented_mask_is_full_slab() {
        let outline = vec![square(0.0, 0.0, 10.0)];
        let (v, i) = tessellate_with_holes(&outline, &[], &bbox(10.0)).unwrap();
        assert!((mesh_area(&v, &i) - 400.0).abs() < 1e-2);
    }

    /// Board cutout (second outline loop) still punches through the sheet.
    #[test]
    fn outline_cutout_respected() {
        let outline = vec![square(0.0, 0.0, 10.0), square(5.0, 5.0, 1.0)]; // 400 − 4
        let (v, i) = tessellate_with_holes(&outline, &[], &bbox(10.0)).unwrap();
        assert!((mesh_area(&v, &i) - 396.0).abs() < 1e-2);
    }
}

