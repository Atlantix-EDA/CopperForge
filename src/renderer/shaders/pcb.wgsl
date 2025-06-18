// PCB rendering shader

// Camera uniform buffer
struct CameraUniforms {
    view_matrix: mat4x4<f32>,
    projection_matrix: mat4x4<f32>,
}

@group(0) @binding(0)
var<uniform> camera: CameraUniforms;

// Vertex input
struct VertexInput {
    @location(0) position: vec3<f32>,
    @location(1) color: vec4<f32>,
    @location(2) uv: vec2<f32>,
}

// Vertex output / Fragment input
struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) color: vec4<f32>,
    @location(1) uv: vec2<f32>,
}

// Vertex shader
@vertex
fn vs_main(input: VertexInput) -> VertexOutput {
    var out: VertexOutput;
    
    // Transform position through view and projection matrices
    let world_position = vec4<f32>(input.position, 1.0);
    let view_position = camera.view_matrix * world_position;
    out.clip_position = camera.projection_matrix * view_position;
    
    // Pass through color and UV
    out.color = input.color;
    out.uv = input.uv;
    
    return out;
}

// Fragment shader
@fragment
fn fs_main(input: VertexOutput) -> @location(0) vec4<f32> {
    // Simple colored output for now
    // TODO: Add texture sampling and more sophisticated shading
    return input.color;
}