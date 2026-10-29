//! STEP component models for the 3D board view (native-only prototype).
//!
//! Pipeline: `solidcraft-kernel`'s `step_import` (hand-written ISO 10303-21
//! reader, mm-normalized) → B-rep solids → CPU tessellation via
//! `display_mesh` → unlit `xyz rgb` triangle soup for `render3d`'s
//! `ColoredMesh`. The glow renderer has no normal attribute or lighting, so
//! a fixed two-light lambert term is baked into the vertex colours here —
//! view-independent, but enough for a component body to read as a solid
//! instead of a flat silhouette.

use solidcraft_kernel::{step_import, StepImport, StepNode};

/// Default body colour when the STEP file carries no presentation style —
/// the grey of molded IC packages.
const DEFAULT_BODY_COLOR: [f32; 3] = [0.55, 0.57, 0.60];

/// Chord tolerance (mm) for display tessellation. Components are small;
/// 0.05 mm keeps pins/fillets round without ballooning vertex counts.
const DISPLAY_TOL_MM: f64 = 0.05;

/// Baked key light: overhead-ish, matching the view the board is mostly
/// seen from. Normalized at use.
const KEY_LIGHT: [f32; 3] = [-0.35, 0.25, 0.9];

/// A tessellated, shaded STEP model ready for `ColoredMesh::upload`.
pub struct StepComponentMesh {
    /// Triangle soup, 6 floats per vertex (`xyz rgb`), board space (mm).
    pub verts: Vec<f32>,
    pub bbox_min: [f32; 3],
    pub bbox_max: [f32; 3],
    /// Display name (file stem or STEP product name).
    pub name: String,
    /// Reader warnings (unsupported entities, dropped faces) — surfaced in
    /// the ribbon tooltip, not fatal.
    pub warnings: Vec<String>,
}

impl StepComponentMesh {
    pub fn triangle_count(&self) -> usize {
        self.verts.len() / 18
    }
}

/// Import a STEP file's text and tessellate every body (assembly placements
/// applied) into one shaded triangle soup, in the file's own coordinates.
pub fn load_step_component(text: &str, name: &str) -> Result<StepComponentMesh, String> {
    let import: StepImport = step_import(text).map_err(|e| e.to_string())?;
    if import.bodies.is_empty() {
        return Err("STEP file contained no readable bodies".into());
    }

    let mut out = StepComponentMesh {
        verts: Vec::new(),
        bbox_min: [f32::INFINITY; 3],
        bbox_max: [f32::NEG_INFINITY; 3],
        name: name.to_string(),
        warnings: import.warnings.clone(),
    };

    const IDENTITY: [[f64; 4]; 4] = [
        [1.0, 0.0, 0.0, 0.0],
        [0.0, 1.0, 0.0, 0.0],
        [0.0, 0.0, 1.0, 0.0],
        [0.0, 0.0, 0.0, 1.0],
    ];
    for node in &import.tree {
        emit_node(node, &IDENTITY, &import, &mut out)?;
    }
    if out.verts.is_empty() {
        return Err("no body produced any display triangles".into());
    }
    Ok(out)
}

/// Walk one assembly node: accumulate its placement, tessellate its bodies,
/// recurse into children.
fn emit_node(
    node: &StepNode,
    parent: &[[f64; 4]; 4],
    import: &StepImport,
    out: &mut StepComponentMesh,
) -> Result<(), String> {
    let m = mat_mul(parent, &node.transform);
    for &bi in &node.bodies {
        let Some(body) = import.bodies.get(bi) else { continue };
        let mesh = match body.body.display_mesh(DISPLAY_TOL_MM) {
            Ok(mesh) => mesh,
            Err(e) => {
                out.warnings
                    .push(format!("body '{}' failed to tessellate: {e}", body.name));
                continue;
            }
        };
        let base = body.color.unwrap_or(DEFAULT_BODY_COLOR);
        emit_mesh(&mesh, &m, base, out);
    }
    for child in &node.children {
        emit_node(child, &m, import, out)?;
    }
    Ok(())
}

/// Append one tessellated body to the soup: transform by the accumulated
/// placement, bake the two-light lambert into the colour per vertex.
fn emit_mesh(
    mesh: &solidcraft_geom::Mesh,
    m: &[[f64; 4]; 4],
    base: [f32; 3],
    out: &mut StepComponentMesh,
) {
    let key = normalize(KEY_LIGHT);
    for tri in &mesh.triangles {
        for &idx in tri {
            let p = mesh.positions[idx as usize];
            let n = mesh
                .normals
                .get(idx as usize)
                .copied()
                .unwrap_or(solidcraft_geom::Vec3 { x: 0.0, y: 0.0, z: 1.0 });
            let wp = transform_point(m, [p.x, p.y, p.z]);
            // Placements are rigid (rotation + translation), so the linear
            // part transforms normals directly.
            let wn = normalize_f64(transform_dir(m, [n.x, n.y, n.z]));

            // Two-light studio bake (same weights as solidcraft's WGSL
            // `shade()`, minus the view-dependent specular): key diffuse +
            // a weak fill from straight above, on an ambient floor.
            let diff = (wn[0] * key[0] as f64 + wn[1] * key[1] as f64 + wn[2] * key[2] as f64)
                .max(0.0) as f32;
            let fill = wn[2].max(0.0) as f32;
            let k = (0.42 + 0.38 * diff + 0.25 * fill).min(1.0);

            let x = wp[0] as f32;
            let y = wp[1] as f32;
            let z = wp[2] as f32;
            out.verts.extend_from_slice(&[
                x,
                y,
                z,
                (base[0] * k).min(1.0),
                (base[1] * k).min(1.0),
                (base[2] * k).min(1.0),
            ]);
            for a in 0..3 {
                let v = [x, y, z][a];
                out.bbox_min[a] = out.bbox_min[a].min(v);
                out.bbox_max[a] = out.bbox_max[a].max(v);
            }
        }
    }
}

/// Column-major 4×4 multiply: `a * b` (apply `b` first, then `a`).
fn mat_mul(a: &[[f64; 4]; 4], b: &[[f64; 4]; 4]) -> [[f64; 4]; 4] {
    let mut r = [[0.0; 4]; 4];
    for (col, r_col) in r.iter_mut().enumerate() {
        for (row, cell) in r_col.iter_mut().enumerate() {
            *cell = (0..4).map(|k| a[k][row] * b[col][k]).sum();
        }
    }
    r
}

fn transform_point(m: &[[f64; 4]; 4], p: [f64; 3]) -> [f64; 3] {
    let mut r = [0.0; 3];
    for (row, out) in r.iter_mut().enumerate() {
        *out = m[0][row] * p[0] + m[1][row] * p[1] + m[2][row] * p[2] + m[3][row];
    }
    r
}

fn transform_dir(m: &[[f64; 4]; 4], d: [f64; 3]) -> [f64; 3] {
    let mut r = [0.0; 3];
    for (row, out) in r.iter_mut().enumerate() {
        *out = m[0][row] * d[0] + m[1][row] * d[1] + m[2][row] * d[2];
    }
    r
}

fn normalize(v: [f32; 3]) -> [f32; 3] {
    let len = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt().max(1e-9);
    [v[0] / len, v[1] / len, v[2] / len]
}

fn normalize_f64(v: [f64; 3]) -> [f64; 3] {
    let len = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt().max(1e-9);
    [v[0] / len, v[1] / len, v[2] / len]
}
