//! Dump a board's raw layer geometry (copper contours, outline, drills) to
//! JSON for external consumers — e.g. the hyperbench CSG experiment. Runs in
//! this workspace's target dir, so nothing beyond this file compiles.
//!
//! Usage: cargo run --release -p copperforge-core --example dump_geometry -- \
//!            <copper.gbr> <edge_cuts.gbr> <drill.drl> <out.json>

use copperforge_core::gerber_geom::{
    self, copper::extract_copper_contours_from_bytes, drill::extract_drill_excellon_from_bytes,
};
use std::time::Instant;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let [copper, edge, drill, out] = args.as_slice() else {
        eprintln!("usage: dump_geometry <copper.gbr> <edge_cuts.gbr> <drill.drl> <out.json>");
        std::process::exit(1);
    };
    let copper_bytes = std::fs::read(copper).expect("copper gerber");
    let edge_bytes = std::fs::read(edge).expect("edge cuts gerber");
    let drill_bytes = std::fs::read(drill).expect("drill file");

    let t = Instant::now();
    let (contours, counts) = extract_copper_contours_from_bytes(&copper_bytes).expect("walk");
    let walk_ms = t.elapsed().as_secs_f64() * 1e3;

    let (outline, _) = gerber_geom::extract_outline_from_bytes(&edge_bytes).expect("outline");

    // Baseline for the bench report: today's lyon tessellation of the same layer.
    let t = Instant::now();
    let (mesh, _) = gerber_geom::copper::extract_copper_from_bytes(&copper_bytes, &outline.bbox)
        .expect("tessellate");
    let lyon_ms = t.elapsed().as_secs_f64() * 1e3;

    let drills = extract_drill_excellon_from_bytes(&drill_bytes, &outline.bbox).expect("drills");

    let to_lists =
        |cs: &[Vec<nalgebra::Point2<f32>>]| -> Vec<Vec<[f32; 2]>> {
            cs.iter()
                .map(|c| c.iter().map(|p| [p.x, p.y]).collect())
                .collect()
        };
    let json = serde_json::json!({
        "copper_contours": to_lists(&contours),
        "outline_contours": to_lists(&outline.contours),
        "bbox": { "min": [outline.bbox.min.x, outline.bbox.min.y],
                  "max": [outline.bbox.max.x, outline.bbox.max.y] },
        "holes": drills.holes.iter()
            .map(|h| { serde_json::json!({ "center": h.center, "radius": h.radius }) })
            .collect::<Vec<_>>(),
        "baseline": { "walk_ms": walk_ms, "lyon_ms": lyon_ms,
                      "lyon_triangles": mesh.mesh_indices.len() / 3,
                      "counts": format!("{counts:?}") },
    });
    std::fs::write(out, serde_json::to_vec(&json).unwrap()).expect("write json");
    eprintln!(
        "dumped {} copper features, {} outline contours, {} holes -> {out}",
        contours.len(),
        outline.contours.len(),
        drills.holes.len()
    );
    eprintln!("baseline: walk {walk_ms:.1} ms, lyon {lyon_ms:.1} ms, {} tris",
        mesh.mesh_indices.len() / 3);
}
