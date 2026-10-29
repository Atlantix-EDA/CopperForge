//! Headless check of the STEP → shaded-triangle-soup pipeline.
//!
//! Usage: cargo run -p copperforge-core --example load_step -- <file.step>...
//!
//! Prints body/triangle/warning counts and the mesh bbox per file — the
//! same data the 3D view uploads, minus the GL context.

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.is_empty() {
        eprintln!("usage: load_step <file.step>...");
        std::process::exit(2);
    }
    let mut failed = false;
    for path in &args {
        let name = std::path::Path::new(path)
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| path.clone());
        let text = match std::fs::read_to_string(path) {
            Ok(t) => t,
            Err(e) => {
                eprintln!("{path}: read failed: {e}");
                failed = true;
                continue;
            }
        };
        let start = std::time::Instant::now();
        match copperforge_core::step_model::load_step_component(&text, &name) {
            Ok(mesh) => {
                let dt = start.elapsed();
                println!(
                    "{name}: {} tris in {dt:.2?}, bbox [{:.2} {:.2} {:.2}]..[{:.2} {:.2} {:.2}] mm, {} warning(s)",
                    mesh.triangle_count(),
                    mesh.bbox_min[0],
                    mesh.bbox_min[1],
                    mesh.bbox_min[2],
                    mesh.bbox_max[0],
                    mesh.bbox_max[1],
                    mesh.bbox_max[2],
                    mesh.warnings.len(),
                );
                for w in mesh.warnings.iter().take(5) {
                    println!("  warn: {w}");
                }
            }
            Err(e) => {
                eprintln!("{name}: FAILED: {e}");
                failed = true;
            }
        }
    }
    std::process::exit(if failed { 1 } else { 0 });
}
