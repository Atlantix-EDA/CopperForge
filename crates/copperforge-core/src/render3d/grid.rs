/// XY-plane gridlines at Z=0 for spatial context. Returns an `xyz rgb` vertex
/// buffer for the LINES primitive. Centerlines (x=0 and y=0) are skipped so
/// they don't z-fight with the axes gizmo.
pub fn grid_vertices(half_extent: f32, step: f32, color: [f32; 3]) -> Vec<f32> {
    let [r, g, b] = color;
    let n = (half_extent / step).floor() as i32;
    let mut v = Vec::with_capacity((4 * n as usize) * 12);

    // Lines parallel to Y axis (varying x)
    for i in -n..=n {
        if i == 0 { continue; }
        let x = i as f32 * step;
        v.extend_from_slice(&[x, -half_extent, 0.0, r, g, b]);
        v.extend_from_slice(&[x,  half_extent, 0.0, r, g, b]);
    }

    // Lines parallel to X axis (varying y)
    for i in -n..=n {
        if i == 0 { continue; }
        let y = i as f32 * step;
        v.extend_from_slice(&[-half_extent, y, 0.0, r, g, b]);
        v.extend_from_slice(&[ half_extent, y, 0.0, r, g, b]);
    }

    v
}
