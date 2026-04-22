//! Minimal 3D renderer for CopperForge.
//!
//! Deliberately knows nothing about gerbers or `layer_store` types — inputs
//! are plain float vertex buffers. Callers in `copperforge-core` do the
//! gerber → vertices conversion and hand down `ColoredMesh` / (later) `LitMesh`.
//!
//! Backend: glow (OpenGL 3.3) via `egui_glow::CallbackFn`.
//! See `develop/task3d-plan.md` for the phased rollout.
//!
//! # Credits
//!
//! [alumina-interface](https://github.com/timschmidt/alumina-interface) by
//! Timothy Schmidt (MIT-licensed) was the direct inspiration for this
//! module. The overall integration pattern — single VAO+VBO meshes with
//! `xyz rgb` stride, `egui_glow::CallbackFn` wrapped in `Arc<Mutex<_>>`,
//! the `POLYGON_OFFSET_FILL` outline trick — all come from alumina, and
//! our initial `renderer.rs` / `mesh.rs` mirror the shape of alumina's
//! `src/renderer.rs` closely.
//!
//! That said, CopperForge's needs diverge from a CNC toolpath viewer.
//! Planned additions not present in alumina (see `develop/task3d-plan.md`
//! for the phased rollout):
//!
//! - **Lit rendering.** A second shader program (`LitProgram` + `LitMesh`,
//!   9 floats/vertex: `xyz` + normal + color) with Lambertian shading for
//!   component bodies. Alumina renders everything flat-colored.
//! - **Gerber geometry extrusion.** Converting 2D gerber polygons into
//!   triangulated extruded layers with per-layer Z from the KiCad stackup
//!   block (with M12 Stackup mechanical layer as fallback).
//! - **Component body loading.** WRL first, STEP via `truck-stepio` for
//!   models that only ship STEP, tessellation cached in the project's
//!   `redb` store.
//! - **Per-instance placement.** Footprint positions/rotations read from
//!   the `.kicad_pcb` `(model …)` blocks, applied as per-instance model
//!   matrices.
//! - **Picking** (Phase-N, deferred). Click a footprint to select it.
//!
//! Each of these stretches the renderer beyond what alumina does. The
//! intent is not to maintain fork-parity — alumina remains a single-file
//! reference for the pattern, we grow from there.

pub mod axes;
pub mod camera;
pub mod grid;
pub mod mesh;
pub mod renderer;

pub use camera::{unproject_to_z0, Camera};
pub use mesh::ColoredMesh;
pub use renderer::UnlitProgram;
