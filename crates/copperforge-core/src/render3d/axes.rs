/// 6-vertex RGB axis gizmo: X = red, Y = green, Z = blue.
/// Returns a flat `xyz rgb` buffer ready for `ColoredMesh::upload`.
pub fn axes_vertices(length: f32) -> Vec<f32> {
    let l = length;
    vec![
        // X axis: red
        0.0, 0.0, 0.0,  1.0, 0.0, 0.0,
        l,   0.0, 0.0,  1.0, 0.0, 0.0,
        // Y axis: green
        0.0, 0.0, 0.0,  0.0, 1.0, 0.0,
        0.0, l,   0.0,  0.0, 1.0, 0.0,
        // Z axis: blue
        0.0, 0.0, 0.0,  0.0, 0.0, 1.0,
        0.0, 0.0, l,    0.0, 0.0, 1.0,
    ]
}
