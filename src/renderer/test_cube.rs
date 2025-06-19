use bytemuck::{Pod, Zeroable};
use std::f32::consts::SQRT_2;

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
pub struct Vertex {
    _pos: [f32; 4],
    _normal: [f32; 4],
}

fn vertex(pos: [i8; 3], normal: [i8; 3]) -> Vertex {
    Vertex {
        _pos: [pos[0] as f32, pos[1] as f32, pos[2] as f32, 1.0],
        _normal: [normal[0] as f32, normal[1] as f32, normal[2] as f32, 1.0],
    }
}

fn create_vertices() -> Vec<Vertex> {
    let mut vertices = Vec::with_capacity(24);
    // Top
    vertices.push(vertex([-1, 1, 1], [0, 1, 0]));
    vertices.push(vertex([1, 1, 1], [0, 1, 0]));
    vertices.push(vertex([1, 1, -1], [0, 1, 0]));
    vertices.push(vertex([-1, 1, -1], [0, 1, 0]));
    // Bottom
    vertices.push(vertex([-1, -1, 1], [0, -1, 0]));
    vertices.push(vertex([1, -1, 1], [0, -1, 0]));
    vertices.push(vertex([1, -1, -1], [0, -1, 0]));
    vertices.push(vertex([-1, -1, -1], [0, -1, 0]));
    // Right
    vertices.push(vertex([1, -1, 1], [1, 0, 0]));
    vertices.push(vertex([1, 1, 1], [1, 0, 0]));
    vertices.push(vertex([1, 1, -1], [1, 0, 0]));
    vertices.push(vertex([1, -1, -1], [1, 0, 0]));
    // Left
    vertices.push(vertex([-1, -1, 1], [-1, 0, 0]));
    vertices.push(vertex([-1, 1, 1], [-1, 0, 0]));
    vertices.push(vertex([-1, 1, -1], [-1, 0, 0]));
    vertices.push(vertex([-1, -1, -1], [-1, 0, 0]));
    // Front
    vertices.push(vertex([-1, -1, 1], [0, 0, 1]));
    vertices.push(vertex([1, -1, 1], [0, 0, 1]));
    vertices.push(vertex([1, 1, 1], [0, 0, 1]));
    vertices.push(vertex([-1, 1, 1], [0, 0, 1]));
    // Back
    vertices.push(vertex([-1, -1, -1], [0, 0, -1]));
    vertices.push(vertex([1, -1, -1], [0, 0, -1]));
    vertices.push(vertex([1, 1, -1], [0, 0, -1]));
    vertices.push(vertex([-1, 1, -1], [0, 0, -1]));

    vertices
}

fn create_indices() -> Vec<u16> {
    vec![
        0, 1, 2, 2, 3, 0, // top
        4, 5, 6, 6, 7, 4, // bottom
        8, 9, 10, 10, 11, 8, // right
        12, 13, 14, 14, 15, 12, // left
        16, 17, 18, 18, 19, 16, // front
        20, 21, 22, 22, 23, 20, // back
    ]
}

pub fn create_cube() -> (Vec<Vertex>, Vec<u16>) {
    (create_vertices(), create_indices())
}

use crate::renderer::{Mesh3D, WgpuMeshData, WgpuVertex};

/// Convert the test cube to Mesh3D
pub fn test_cube_mesh3d() -> Mesh3D {
    let (vertices, indices) = create_cube();
    let mut mesh = Mesh3D::new();
    for v in vertices.iter() {
        mesh.vertices.push(nalgebra::Point3::new(v._pos[0], v._pos[1], v._pos[2]));
        mesh.normals.push(nalgebra::Vector3::new(v._normal[0], v._normal[1], v._normal[2]));
        mesh.uvs.push(nalgebra::Point2::new(0.0, 0.0)); // No UVs for test cube
    }
    mesh.indices = indices.iter().map(|&i| i as u32).collect();
    mesh.material_id = Some(0);
    mesh
}

/// Convert Mesh3D to WgpuMeshData
pub fn mesh3d_to_wgpu(mesh: &Mesh3D) -> WgpuMeshData {
    let vertices = mesh.vertices.iter().zip(mesh.normals.iter()).zip(mesh.uvs.iter()).map(|((pos, normal), uv)| {
        WgpuVertex {
            position: [pos.x, pos.y, pos.z],
            normal: [normal.x, normal.y, normal.z],
            uv: [uv.x, uv.y],
            material_id: mesh.material_id.unwrap_or(0),
        }
    }).collect();
    WgpuMeshData {
        layer_type: crate::layer_operations::LayerType::MechanicalOutline, // Use a generic type for test
        vertices,
        indices: mesh.indices.clone(),
        material_id: mesh.material_id.unwrap_or(0),
        visible: true,
    }
}
