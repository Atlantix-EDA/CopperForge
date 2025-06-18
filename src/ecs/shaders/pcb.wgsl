// PCB 3D Shader
// Simple vertex/fragment shader for rendering PCB meshes with basic lighting

struct CameraUniforms {
    view_proj: mat4x4<f32>,
    view_pos: vec3<f32>,
};

struct MaterialUniforms {
    color: vec4<f32>,
    metallic: f32,
    roughness: f32,
};

struct VertexInput {
    @location(0) position: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) uv: vec2<f32>,
};

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) world_position: vec3<f32>,
    @location(1) world_normal: vec3<f32>,
    @location(2) uv: vec2<f32>,
};

@group(0) @binding(0)
var<uniform> camera: CameraUniforms;

@group(1) @binding(0)
var<uniform> material: MaterialUniforms;

@vertex
fn vs_main(vertex: VertexInput) -> VertexOutput {
    var out: VertexOutput;
    
    out.world_position = vertex.position;
    out.world_normal = vertex.normal;
    out.uv = vertex.uv;
    out.clip_position = camera.view_proj * vec4<f32>(vertex.position, 1.0);
    
    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    // Simple lighting calculation
    let light_dir = normalize(vec3<f32>(1.0, 1.0, 1.0));
    let normal = normalize(in.world_normal);
    
    // Diffuse lighting
    let diffuse = max(dot(normal, light_dir), 0.0);
    
    // Ambient lighting
    let ambient = 0.3;
    
    // Combine lighting with material color
    let lighting = ambient + diffuse * 0.7;
    let final_color = material.color.rgb * lighting;
    
    return vec4<f32>(final_color, material.color.a);
}