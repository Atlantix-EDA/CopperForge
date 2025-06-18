// Vertex shader

struct CameraUniforms {
    view_proj: mat4x4<f32>,
    view_pos: vec3<f32>,
};

struct MaterialUniforms {
    color: vec4<f32>,
    metallic: f32,
    roughness: f32,
};

@group(0) @binding(0)
var<uniform> camera: CameraUniforms;

@group(1) @binding(0)
var<uniform> material: MaterialUniforms;

struct VertexInput {
    @location(0) position: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) uv: vec2<f32>,
};

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) world_position: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) uv: vec2<f32>,
};

@vertex
fn vs_main(model: VertexInput) -> VertexOutput {
    var out: VertexOutput;
    
    // Transform position to world space (assuming model matrix is identity for now)
    out.world_position = model.position;
    
    // Transform to clip space
    out.clip_position = camera.view_proj * vec4<f32>(model.position, 1.0);
    
    // Pass through normal and UV
    out.normal = normalize(model.normal);
    out.uv = model.uv;
    
    return out;
}

// Fragment shader

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    // Simple lighting calculation
    let light_dir = normalize(vec3<f32>(1.0, 1.0, 1.0));
    let view_dir = normalize(camera.view_pos - in.world_position);
    let half_dir = normalize(light_dir + view_dir);
    
    // Lambertian diffuse
    let n_dot_l = max(dot(in.normal, light_dir), 0.0);
    let diffuse = material.color.rgb * n_dot_l;
    
    // Simple Blinn-Phong specular
    let n_dot_h = max(dot(in.normal, half_dir), 0.0);
    let shininess = (1.0 - material.roughness) * 128.0 + 1.0;
    let specular_strength = material.metallic;
    let specular = vec3<f32>(specular_strength) * pow(n_dot_h, shininess);
    
    // Ambient lighting
    let ambient = material.color.rgb * 0.1;
    
    // Combine lighting
    let color = ambient + diffuse + specular;
    
    return vec4<f32>(color, material.color.a);
}