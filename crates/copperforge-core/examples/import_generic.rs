//! Headless check of the generic gerber import path.
//!
//! Usage: cargo run -p copperforge-core --example import_generic -- <gerber-dir> [board.kicad_pcb]
//!
//! Drives the same LayerStore load the "Import Folder…" button uses, then
//! smoke-tests the geometry pipeline: outline bbox, copper tessellation,
//! and Excellon drill recovery. With a `.kicad_pcb` argument it also feeds
//! the board's layer renames into detection, mirroring the KiCad project
//! path (`load_gerbers`) — exercises boards whose layers were renamed by
//! an EasyEDA-Pro import ("Edge.Cuts" → "Multi-Layer", …).

use copperforge_core::gerber_geom;
use copperforge_core::layer_store::{LayerStore, LayerType};

fn main() {
    let dir = std::env::args().nth(1).expect("usage: import_generic <gerber-dir> [board.kicad_pcb]");
    let dir = std::path::PathBuf::from(dir);

    let mut store = LayerStore::default();
    if let Some(pcb) = std::env::args().nth(2) {
        let pcb = std::path::PathBuf::from(pcb);
        store.user_layer_names =
            copperforge_core::project_manager::kicad_metadata::read_user_layer_names(&pcb)
                .expect("user layer names");
        store.canonical_renames =
            copperforge_core::project_manager::kicad_metadata::read_canonical_layer_renames(&pcb)
                .expect("canonical renames");
        println!(
            "renames from pcb: {} user-slot, {} canonical",
            store.user_layer_names.len(),
            store.canonical_renames.len()
        );
    }
    let cc = store.detect_copper_count_in_directory(&dir);
    store.set_copper_count(cc);
    let (loaded, unassigned) = match store.load_from_directory(&dir) {
        Ok(x) => x,
        Err(e) => {
            eprintln!("load failed: {e}");
            std::process::exit(1);
        }
    };
    println!("copper_count={cc}  loaded={loaded}  unassigned={unassigned}");
    for l in &store.layers {
        println!(
            "  {:28} <- {}",
            format!("{:?}", l.layer_type),
            l.file_path
                .as_ref()
                .and_then(|p| p.file_name())
                .map(|f| f.to_string_lossy().into_owned())
                .unwrap_or_default(),
        );
    }
    for u in &store.unassigned {
        println!("  UNASSIGNED                  {}", u.filename);
    }
    for d in &store.drill_sources {
        println!("  DRILL (excellon)            {} ({} bytes)", d.filename, d.bytes.len());
    }

    // Geometry smoke test: outline → copper → drills.
    let Some(outline_layer) = store.find(LayerType::MechanicalOutline) else {
        eprintln!("no mechanical outline detected — 3D view would stay empty");
        std::process::exit(1);
    };
    let path = outline_layer.file_path.clone().expect("outline has a path");
    let Some((outline, _)) = gerber_geom::extract_outline(&path) else {
        eprintln!("outline extraction failed");
        std::process::exit(1);
    };
    println!(
        "outline: {} contour(s), bbox {:.2}×{:.2} mm",
        outline.contours.len(),
        outline.bbox.max.x - outline.bbox.min.x,
        outline.bbox.max.y - outline.bbox.min.y,
    );

    for (ty, label) in [
        (LayerType::Copper(1), "F.Cu"),
        (store.bottom_copper_type(), "B.Cu"),
    ] {
        if let Some(layer) = store.find(ty) {
            if let Some(p) = layer.file_path.as_ref() {
                match gerber_geom::extract_copper(p, &outline.bbox) {
                    Some((cu, _)) => println!(
                        "{label}: {} contours, {} tris",
                        cu.contours.len(),
                        cu.mesh_indices.len() / 3
                    ),
                    None => println!("{label}: extraction FAILED"),
                }
            }
        } else {
            println!("{label}: not found");
        }
    }

    let mut total_holes = 0usize;
    for src in &store.drill_sources {
        match gerber_geom::extract_drill_excellon_from_bytes(&src.bytes, &outline.bbox) {
            Some(d) => {
                println!("{}: {} hole(s)", src.filename, d.holes.len());
                total_holes += d.holes.len();
            }
            None => println!("{}: no holes recovered", src.filename),
        }
    }
    println!("total drill holes: {total_holes}");
}
