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
use lyon::math::Point as LyonPoint;
use lyon::path::Path as LyonPath;
use lyon::tessellation::{
    BuffersBuilder, FillOptions, FillRule, FillTessellator, VertexBuffers,
};
use nalgebra::Point2;

use super::copper::{walk_copper, CopperCounts};

/// Tessellated soldermask mesh for one side of the board. Structurally
/// identical to `CopperData`; kept as a distinct type so the renderer can't
/// accidentally cross-wire a mask mesh into a copper slot (or vice versa).
#[derive(Debug, Clone)]
pub struct MaskData {
    pub mesh_vertices_2d: Vec<[f32; 2]>,
    pub mesh_indices: Vec<u32>,
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
        },
        counts,
    ))
}

/// Lyon tessellation with `FillRule::EvenOdd`. Outline contours are pushed
/// first, then opening contours — winding order is irrelevant under
/// even-odd, so we don't need to force outer-CCW / hole-CW.
///
/// World transform matches `extract_copper`: shift by outline-bbox center,
/// no Y-flip (gerber is Y-up per §4.2.3). This locks the mask mesh into
/// exact pixel-alignment with the copper + board meshes.
fn tessellate_with_holes(
    outline: &[Vec<Point2<f32>>],
    openings: &[Vec<Point2<f32>>],
    outline_bbox: &BoundingBox,
) -> Option<(Vec<[f32; 2]>, Vec<u32>)> {
    let mut builder = LyonPath::builder();
    for c in outline.iter().chain(openings.iter()) {
        if c.len() < 3 {
            continue;
        }
        builder.begin(LyonPoint::new(c[0].x, c[0].y));
        for p in &c[1..] {
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
    )
    .ok()?;
    if geometry.indices.is_empty() {
        return None;
    }

    let cx = ((outline_bbox.min.x + outline_bbox.max.x) * 0.5) as f32;
    let cy = ((outline_bbox.min.y + outline_bbox.max.y) * 0.5) as f32;
    for v in &mut geometry.vertices {
        v[0] -= cx;
        v[1] -= cy;
    }

    Some((geometry.vertices, geometry.indices))
}
