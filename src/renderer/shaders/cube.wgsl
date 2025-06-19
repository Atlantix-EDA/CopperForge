// cube.wgsl - Minimal 3D shader for wgpu

struct Camera {
    view_proj: mat4x4<f32>;
};

@group(0) @binding(0)
var<uniform> camera: Camera;

struct VertexInput {
    @location(0) position: vec3<f32>;
    @location(1) normal: vec3<f32>;
};

struct VertexOutput {
    @builtin(position) position: vec4<f32>;
    @location(0) normal: vec3<f32>;
};

@vertex
fn vs_main(input: VertexInput) -> VertexOutput {
    var out: VertexOutput;
    out.position = camera.view_proj * vec4<f32>(input.position, 1.0);
    out.normal = input.normal;
    return out;
}

@fragment
fn fs_main(input: VertexOutput) -> @location(0) vec4<f32> {
    // Simple shading: color by normal
    let color = 0.5 * (input.normal + vec3<f32>(1.0, 1.0, 1.0));
    return vec4<f32>(color, 1.0);
}
