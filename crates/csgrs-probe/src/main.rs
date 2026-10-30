//! csgrs/Hyper stack feasibility probe.
//!
//! Probe 1 — STEP import: parse a vendor STEP (de-branded LCSC model from
//! the kiverse tree), find every *_SHAPE_REPRESENTATION entity, and attempt
//! a strict hyperbrep-io import of each. Reports what the current exchange
//! layer accepts and what it rejects (and why).
//!
//! Probe 2 — boolean scale: board-sized slab (205.3 × 135.3 × 1.55 mm)
//! minus a run of drill cylinders through Hypermesh, timed per op.
//! Triangle counts are derived from binary-STL size (84 + 50·n) so the
//! probe doesn't depend on unstable mesh-introspection API.

use std::time::Instant;

use csgrs::io::stl::to_stl_binary;
use csgrs::solid::{self, SolidExt};
use hyperbrep::{OperationContext, WorkBudget};
use hyperbrep_io::step::{ImportLimits, StepFile, StepReadOptions};
use hyperlattice::Real;

fn r(v: f64) -> Real {
    Real::try_from(v).expect("finite probe constant")
}

fn tri_count(mesh: &csgrs::TriangleMesh) -> usize {
    to_stl_binary(mesh, "probe").map(|b| (b.len().saturating_sub(84)) / 50).unwrap_or(0)
}

fn budget() -> OperationContext {
    OperationContext::strict(WorkBudget { refinements: 10_000_000, promotions: 1_000_000 })
}

fn probe_step(path: &str) {
    println!("── probe 1: STEP import — {path}");
    let bytes = match std::fs::read(path) {
        Ok(b) => b,
        Err(e) => {
            println!("  read failed: {e}");
            return;
        }
    };
    let t0 = Instant::now();
    let file = match StepFile::read(&bytes, &StepReadOptions::default()) {
        Ok(f) => f,
        Err(e) => {
            println!("  Part 21 parse FAILED: {e:?}");
            return;
        }
    };
    println!("  Part 21 parse ok: {} bytes in {:?}", bytes.len(), t0.elapsed());

    // Candidate representations, scanned from source text: `#id = KEYWORD(`.
    let src = String::from_utf8_lossy(&bytes);
    let mut candidates: Vec<(u64, String)> = Vec::new();
    for line in src.lines() {
        let line = line.trim();
        let Some(rest) = line.strip_prefix('#') else { continue };
        let Some((id, tail)) = rest.split_once('=') else { continue };
        let Ok(id) = id.trim().parse::<u64>() else { continue };
        let keyword = tail.trim().split('(').next().unwrap_or("").trim().to_string();
        if keyword.contains("SHAPE_REPRESENTATION") {
            candidates.push((id, keyword));
        }
    }
    println!("  {} *_SHAPE_REPRESENTATION candidate(s)", candidates.len());

    let mut imported = 0usize;
    for (id, keyword) in &candidates {
        let t = Instant::now();
        let outcome = file.import_representation(*id, &ImportLimits::default(), &budget());
        match outcome.result {
            Ok(Ok(rep)) => {
                imported += 1;
                let bodies = rep.bodies();
                let (mut faces, mut verts) = (0usize, 0usize);
                for b in bodies {
                    faces += b.faces().len();
                    verts += b.vertices().len();
                }
                println!(
                    "  #{id} {keyword}: IMPORTED in {:?} — {} body(ies), {faces} faces, {verts} vertices, name {:?}",
                    t.elapsed(),
                    bodies.len(),
                    rep.name(),
                );
            }
            Ok(Err(e)) => println!("  #{id} {keyword}: rejected — {e:?}"),
            Err(b) => println!("  #{id} {keyword}: work budget exhausted — {b:?}"),
        }
    }
    println!("  imported {imported}/{} representations", candidates.len());
}

fn probe_booleans() {
    println!("── probe 2: Hypermesh booleans at board scale");
    // Warmup + sanity: the canonical cube ∪ sphere from the csgrs example.
    let t = Instant::now();
    let cube = solid::cube(r(2.0)).expect("cube");
    let sphere = solid::sphere(r(1.35), 32, 16).expect("sphere");
    let sphere = sphere.translated(r(0.55), r(0.25), r(0.2)).expect("translate");
    let u = cube.try_union(&sphere).expect("union");
    println!("  warmup cube∪sphere: {} tris in {:?}", tri_count(&u), t.elapsed());

    // Interface-board-sized slab, drills punched one at a time — the
    // worst-case sequential pattern the viewer would actually use.
    let t_slab = Instant::now();
    let mut slab = solid::cuboid(r(205.3), r(135.3), r(1.55)).expect("slab");
    println!("  slab 205.3×135.3×1.55: {} tris in {:?}", tri_count(&slab), t_slab.elapsed());
    let n = 20usize;
    let t_all = Instant::now();
    for i in 0..n {
        let x = 12.0 + (i as f64) * 9.3;
        let y = 14.0 + ((i % 3) as f64) * 50.0;
        let t1 = Instant::now();
        let drill = solid::cylinder(r(1.6), r(5.0), 24)
            .expect("drill")
            .translated(r(x), r(y), r(-1.7))
            .expect("place");
        slab = slab.try_difference(&drill).expect("difference");
        if i == 0 || i == n - 1 {
            println!("    drill {}/{n}: {:?} ({} tris so far)", i + 1, t1.elapsed(), tri_count(&slab));
        }
    }
    let total = t_all.elapsed();
    println!(
        "  {n} drill differences: {total:?} total, {:?}/op avg, final mesh {} tris",
        total / (n as u32),
        tri_count(&slab)
    );
}

fn main() {
    let path = std::env::args().nth(1);
    match path {
        Some(p) => probe_step(&p),
        None => println!("── probe 1 skipped (usage: csgrs-probe <file.step>)"),
    }
    probe_booleans();
}
