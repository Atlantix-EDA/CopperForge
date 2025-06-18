use wgpu::util::DeviceExt;
use bytemuck::{Pod, Zeroable};
use nalgebra::{Matrix4, Point3, Vector3};
use crate::ecs::Mesh3D;

/// Vertex data for wgpu rendering
#[repr(C)]
#[derive(Copy, Clone, Debug, Pod, Zeroable)]
pub struct Vertex {
    pub position: [f32; 3],
    pub normal: [f32; 3],
    pub uv: [f32; 2],
}

impl Vertex {
    const ATTRIBS: [wgpu::VertexAttribute; 3] = wgpu::vertex_attr_array![
        0 => Float32x3,  // position
        1 => Float32x3,  // normal
        2 => Float32x2,  // uv
    ];

    pub fn desc() -> wgpu::VertexBufferLayout<'static> {
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<Vertex>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &Self::ATTRIBS,
        }
    }
}

/// Material properties for PCB layers
#[repr(C)]
#[derive(Copy, Clone, Debug, Pod, Zeroable)]
pub struct MaterialUniforms {
    pub color: [f32; 4],
    pub metallic: f32,
    pub roughness: f32,
    pub _padding: [f32; 2],
}

/// Camera uniforms for 3D view
#[repr(C)]
#[derive(Copy, Clone, Debug, Pod, Zeroable)]
pub struct CameraUniforms {
    pub view_proj: [[f32; 4]; 4],
    pub view_pos: [f32; 3],
    pub _padding: f32,
}

/// GPU mesh data
pub struct GpuMesh {
    pub vertex_buffer: wgpu::Buffer,
    pub index_buffer: wgpu::Buffer,
    pub index_count: u32,
    pub material_id: u32,
}

/// PCB material definitions
pub struct PcbMaterials {
    pub materials: Vec<MaterialUniforms>,
    pub material_buffer: wgpu::Buffer,
    pub bind_group: wgpu::BindGroup,
}

impl PcbMaterials {
    pub fn new(device: &wgpu::Device, bind_group_layout: &wgpu::BindGroupLayout) -> Self {
        // Define standard PCB materials
        let materials = vec![
            // Material 0: Default
            MaterialUniforms {
                color: [0.8, 0.8, 0.8, 1.0],
                metallic: 0.1,
                roughness: 0.8,
                _padding: [0.0; 2],
            },
            // Material 1: Copper
            MaterialUniforms {
                color: [0.72, 0.45, 0.20, 1.0],
                metallic: 0.9,
                roughness: 0.1,
                _padding: [0.0; 2],
            },
            // Material 2: Soldermask (Green)
            MaterialUniforms {
                color: [0.0, 0.4, 0.0, 0.9],
                metallic: 0.0,
                roughness: 0.7,
                _padding: [0.0; 2],
            },
            // Material 3: Silkscreen (White)
            MaterialUniforms {
                color: [0.9, 0.9, 0.9, 1.0],
                metallic: 0.0,
                roughness: 0.8,
                _padding: [0.0; 2],
            },
            // Material 4: Solder paste
            MaterialUniforms {
                color: [0.8, 0.8, 0.8, 1.0],
                metallic: 0.3,
                roughness: 0.4,
                _padding: [0.0; 2],
            },
            // Material 5: FR4 substrate
            MaterialUniforms {
                color: [0.2, 0.3, 0.1, 1.0],
                metallic: 0.0,
                roughness: 0.9,
                _padding: [0.0; 2],
            },
        ];

        let material_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Material Buffer"),
            contents: bytemuck::cast_slice(&materials),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });

        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            layout: bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: material_buffer.as_entire_binding(),
            }],
            label: Some("Material Bind Group"),
        });

        Self {
            materials,
            material_buffer,
            bind_group,
        }
    }
}

/// Camera controller for 3D PCB view
#[derive(Clone)]
pub struct PcbCamera {
    pub eye: Point3<f32>,
    pub target: Point3<f32>,
    pub up: Vector3<f32>,
    pub fovy: f32,
    pub znear: f32,
    pub zfar: f32,
    pub aspect: f32,
}

impl PcbCamera {
    pub fn new() -> Self {
        Self {
            eye: Point3::new(0.0, 0.0, 5.0),
            target: Point3::new(0.0, 0.0, 0.0),
            up: Vector3::new(0.0, 1.0, 0.0),
            fovy: 45.0,
            znear: 0.1,
            zfar: 100.0,
            aspect: 1.0,
        }
    }

    pub fn build_view_projection_matrix(&self) -> Matrix4<f32> {
        let view = Matrix4::look_at_rh(&self.eye, &self.target, &self.up);
        let proj = Matrix4::new_perspective(self.aspect, self.fovy.to_radians(), self.znear, self.zfar);
        proj * view
    }

    pub fn update_aspect(&mut self, width: f32, height: f32) {
        self.aspect = width / height;
    }

    /// Handle mouse input for camera controls
    pub fn handle_input(&mut self, delta_x: f32, delta_y: f32, zoom: f32) {
        // Rotate around target
        let radius = (self.eye - self.target).magnitude();
        
        // Convert to spherical coordinates
        let mut theta = (self.eye.z - self.target.z).atan2(self.eye.x - self.target.x);
        let mut phi = ((self.eye.y - self.target.y) / radius).asin();
        
        // Apply rotation
        theta -= delta_x * 0.01;
        phi += delta_y * 0.01;
        
        // Clamp phi to prevent flipping
        phi = phi.clamp(-std::f32::consts::PI / 2.0 + 0.1, std::f32::consts::PI / 2.0 - 0.1);
        
        // Apply zoom
        let new_radius = (radius * (1.0 + zoom * 0.1)).clamp(0.1, 50.0);
        
        // Convert back to cartesian
        self.eye = Point3::new(
            self.target.x + new_radius * phi.cos() * theta.cos(),
            self.target.y + new_radius * phi.sin(),
            self.target.z + new_radius * phi.cos() * theta.sin(),
        );
    }
}

/// Main PCB renderer using wgpu
pub struct PcbWgpuRenderer {
    pub render_pipeline: wgpu::RenderPipeline,
    camera_buffer: wgpu::Buffer,
    pub camera_bind_group: wgpu::BindGroup,
    pub materials: PcbMaterials,
    pub meshes: Vec<GpuMesh>,
    pub camera: PcbCamera,
}

impl PcbWgpuRenderer {
    pub fn new(device: &wgpu::Device, format: wgpu::TextureFormat) -> Self {
        // Create shader
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("PCB Shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shaders/pcb.wgsl").into()),
        });

        // Create camera uniform buffer
        let camera_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Camera Buffer"),
            size: std::mem::size_of::<CameraUniforms>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        // Create bind group layouts
        let camera_bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
            label: Some("Camera Bind Group Layout"),
        });

        let material_bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
            label: Some("Material Bind Group Layout"),
        });

        // Create camera bind group
        let camera_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            layout: &camera_bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: camera_buffer.as_entire_binding(),
            }],
            label: Some("Camera Bind Group"),
        });

        // Create materials
        let materials = PcbMaterials::new(device, &material_bind_group_layout);

        // Create render pipeline layout
        let render_pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("PCB Render Pipeline Layout"),
            bind_group_layouts: &[&camera_bind_group_layout, &material_bind_group_layout],
            push_constant_ranges: &[],
        });

        // Create render pipeline
        let render_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("PCB Render Pipeline"),
            layout: Some(&render_pipeline_layout),
            cache: None,
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                buffers: &[Vertex::desc()],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: Default::default(),
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                strip_index_format: None,
                front_face: wgpu::FrontFace::Ccw,
                cull_mode: Some(wgpu::Face::Back),
                polygon_mode: wgpu::PolygonMode::Fill,
                unclipped_depth: false,
                conservative: false,
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: wgpu::TextureFormat::Depth32Float,
                depth_write_enabled: true,
                depth_compare: wgpu::CompareFunction::Less,
                stencil: wgpu::StencilState::default(),
                bias: wgpu::DepthBiasState::default(),
            }),
            multisample: wgpu::MultisampleState {
                count: 1,
                mask: !0,
                alpha_to_coverage_enabled: false,
            },
            multiview: None,
        });

        Self {
            render_pipeline,
            camera_buffer,
            camera_bind_group,
            materials,
            meshes: Vec::new(),
            camera: PcbCamera::new(),
        }
    }

    /// Convert Mesh3D to GPU mesh
    pub fn upload_mesh(&self, device: &wgpu::Device, mesh: &Mesh3D) -> GpuMesh {
        // Convert to vertices
        let vertices: Vec<Vertex> = mesh.vertices
            .iter()
            .zip(mesh.normals.iter())
            .zip(mesh.uvs.iter())
            .map(|((pos, normal), uv)| Vertex {
                position: [pos.x, pos.y, pos.z],
                normal: [normal.x, normal.y, normal.z],
                uv: [uv.x, uv.y],
            })
            .collect();

        let vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Vertex Buffer"),
            contents: bytemuck::cast_slice(&vertices),
            usage: wgpu::BufferUsages::VERTEX,
        });

        let index_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Index Buffer"),
            contents: bytemuck::cast_slice(&mesh.indices),
            usage: wgpu::BufferUsages::INDEX,
        });

        GpuMesh {
            vertex_buffer,
            index_buffer,
            index_count: mesh.indices.len() as u32,
            material_id: mesh.material_id.unwrap_or(0),
        }
    }

    /// Upload multiple meshes to GPU
    pub fn upload_meshes(&mut self, device: &wgpu::Device, meshes: &[Mesh3D]) {
        self.meshes.clear();
        for mesh in meshes {
            let gpu_mesh = self.upload_mesh(device, mesh);
            self.meshes.push(gpu_mesh);
        }
    }

    /// Update camera uniforms
    pub fn update_camera(&mut self, queue: &wgpu::Queue, width: f32, height: f32) {
        let mut camera = self.camera.clone();
        camera.update_aspect(width, height);
        
        let view_proj = camera.build_view_projection_matrix();
        let uniforms = CameraUniforms {
            view_proj: view_proj.into(),
            view_pos: [camera.eye.x, camera.eye.y, camera.eye.z],
            _padding: 0.0,
        };

        queue.write_buffer(&self.camera_buffer, 0, bytemuck::cast_slice(&[uniforms]));
    }

    /// Render the PCB meshes
    pub fn render(&self, view: &wgpu::TextureView, depth_view: &wgpu::TextureView, encoder: &mut wgpu::CommandEncoder) {
        let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("PCB Render Pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color {
                        r: 0.1,
                        g: 0.1,
                        b: 0.1,
                        a: 1.0,
                    }),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: depth_view,
                depth_ops: Some(wgpu::Operations {
                    load: wgpu::LoadOp::Clear(1.0),
                    store: wgpu::StoreOp::Store,
                }),
                stencil_ops: None,
            }),
            occlusion_query_set: None,
            timestamp_writes: None,
        });

        render_pass.set_pipeline(&self.render_pipeline);
        render_pass.set_bind_group(0, &self.camera_bind_group, &[]);
        render_pass.set_bind_group(1, &self.materials.bind_group, &[]);

        // Render all meshes
        for mesh in &self.meshes {
            render_pass.set_vertex_buffer(0, mesh.vertex_buffer.slice(..));
            render_pass.set_index_buffer(mesh.index_buffer.slice(..), wgpu::IndexFormat::Uint32);
            render_pass.draw_indexed(0..mesh.index_count, 0, 0..1);
        }
    }
}