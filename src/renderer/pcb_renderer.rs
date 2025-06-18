//! PCB-specific rendering logic

use wgpu::{
    Buffer, BufferUsages, RenderPipeline, VertexAttribute, VertexBufferLayout, VertexFormat,
    VertexStepMode, BindGroupLayout, BindGroupLayoutDescriptor, BindGroupLayoutEntry,
    BindingType, BufferBindingType, ShaderStages, PipelineLayoutDescriptor, RenderPipelineDescriptor,
    VertexState, FragmentState, ColorTargetState, BlendState, ColorWrites, PrimitiveState,
    PrimitiveTopology, FrontFace, Face, PolygonMode, MultisampleState,
};
use glam::{Vec3, Vec2};
use crate::renderer::{WgpuRenderer, WgpuRendererError};
use gerber_viewer::GerberLayer;

/// PCB-specific renderer that handles gerber layer visualization
pub struct PcbRenderer {
    vertex_buffer: Option<Buffer>,
    index_buffer: Option<Buffer>,
    render_pipeline: Option<RenderPipeline>,
    bind_group_layout: Option<BindGroupLayout>,
    vertex_count: u32,
    index_count: u32,
}

/// Vertex data for PCB rendering
#[repr(C)]
#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct PcbVertex {
    position: [f32; 3],
    color: [f32; 4],
    uv: [f32; 2],
}

impl PcbVertex {
    /// Create vertex buffer layout
    fn layout() -> VertexBufferLayout<'static> {
        VertexBufferLayout {
            array_stride: std::mem::size_of::<PcbVertex>() as wgpu::BufferAddress,
            step_mode: VertexStepMode::Vertex,
            attributes: &[
                // Position
                VertexAttribute {
                    offset: 0,
                    shader_location: 0,
                    format: VertexFormat::Float32x3,
                },
                // Color
                VertexAttribute {
                    offset: std::mem::size_of::<[f32; 3]>() as wgpu::BufferAddress,
                    shader_location: 1,
                    format: VertexFormat::Float32x4,
                },
                // UV coordinates
                VertexAttribute {
                    offset: std::mem::size_of::<[f32; 7]>() as wgpu::BufferAddress,
                    shader_location: 2,
                    format: VertexFormat::Float32x2,
                },
            ],
        }
    }
}

impl PcbRenderer {
    /// Create a new PCB renderer
    pub fn new() -> Self {
        Self {
            vertex_buffer: None,
            index_buffer: None,
            render_pipeline: None,
            bind_group_layout: None,
            vertex_count: 0,
            index_count: 0,
        }
    }

    /// Initialize the PCB renderer with WGPU resources
    pub fn initialize(&mut self, renderer: &WgpuRenderer) -> Result<(), WgpuRendererError> {
        // Create bind group layout for uniforms
        let bind_group_layout = renderer.device().create_bind_group_layout(&BindGroupLayoutDescriptor {
            label: Some("Camera Bind Group Layout"),
            entries: &[
                BindGroupLayoutEntry {
                    binding: 0,
                    visibility: ShaderStages::VERTEX,
                    ty: BindingType::Buffer {
                        ty: BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
            ],
        });

        // Create shader module
        let shader = renderer.device().create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("PCB Shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shaders/pcb.wgsl").into()),
        });

        // Create render pipeline layout
        let pipeline_layout = renderer.device().create_pipeline_layout(&PipelineLayoutDescriptor {
            label: Some("PCB Pipeline Layout"),
            bind_group_layouts: &[&bind_group_layout],
            push_constant_ranges: &[],
        });

        // Create render pipeline
        let render_pipeline = renderer.device().create_render_pipeline(&RenderPipelineDescriptor {
            label: Some("PCB Render Pipeline"),
            layout: Some(&pipeline_layout),
            vertex: VertexState {
                module: &shader,
                entry_point: "vs_main",
                buffers: &[PcbVertex::layout()],
                compilation_options: Default::default(),
            },
            fragment: Some(FragmentState {
                module: &shader,
                entry_point: "fs_main",
                targets: &[Some(ColorTargetState {
                    format: renderer.surface_config.as_ref()
                        .map(|config| config.format)
                        .unwrap_or(wgpu::TextureFormat::Bgra8UnormSrgb),
                    blend: Some(BlendState::ALPHA_BLENDING),
                    write_mask: ColorWrites::ALL,
                })],
                compilation_options: Default::default(),
            }),
            primitive: PrimitiveState {
                topology: PrimitiveTopology::TriangleList,
                strip_index_format: None,
                front_face: FrontFace::Ccw,
                cull_mode: Some(Face::Back),
                polygon_mode: PolygonMode::Fill,
                unclipped_depth: false,
                conservative: false,
            },
            depth_stencil: None,
            multisample: MultisampleState {
                count: renderer.settings.msaa_samples,
                mask: !0,
                alpha_to_coverage_enabled: false,
            },
            multiview: None,
            cache: None,
        });

        self.render_pipeline = Some(render_pipeline);
        self.bind_group_layout = Some(bind_group_layout);

        Ok(())
    }

    /// Load PCB data from gerber layer
    pub fn load_gerber_layer(&mut self, renderer: &WgpuRenderer, gerber_layer: &GerberLayer) -> Result<(), WgpuRendererError> {
        // Convert gerber data to vertices
        let (vertices, indices) = self.convert_gerber_to_mesh(gerber_layer);

        if vertices.is_empty() {
            return Ok(());
        }

        // Create vertex buffer
        use wgpu::util::DeviceExt;
        let vertex_buffer = renderer.device().create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("PCB Vertex Buffer"),
            contents: bytemuck::cast_slice(&vertices),
            usage: BufferUsages::VERTEX,
        });

        // Create index buffer
        let index_buffer = renderer.device().create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("PCB Index Buffer"),
            contents: bytemuck::cast_slice(&indices),
            usage: BufferUsages::INDEX,
        });

        self.vertex_buffer = Some(vertex_buffer);
        self.index_buffer = Some(index_buffer);
        self.vertex_count = vertices.len() as u32;
        self.index_count = indices.len() as u32;

        Ok(())
    }

    /// Convert gerber layer data to renderable mesh
    fn convert_gerber_to_mesh(&self, gerber_layer: &GerberLayer) -> (Vec<PcbVertex>, Vec<u16>) {
        let mut vertices = Vec::new();
        let mut indices = Vec::new();

        // Get bounding box for normalization
        let bbox = gerber_layer.bounding_box();
        let _center_x = (bbox.min.x + bbox.max.x) / 2.0;
        let _center_y = (bbox.min.y + bbox.max.y) / 2.0;
        let scale = 0.01; // Scale factor to fit in normalized coordinates

        // For now, create a simple quad representing the gerber layer bounds
        // TODO: Convert actual gerber primitives to triangulated mesh
        let half_width = (bbox.max.x - bbox.min.x) as f32 * scale * 0.5;
        let half_height = (bbox.max.y - bbox.min.y) as f32 * scale * 0.5;

        // Create quad vertices
        vertices.extend_from_slice(&[
            // Bottom-left
            PcbVertex {
                position: [-half_width, -half_height, 0.0],
                color: [0.0, 1.0, 0.0, 1.0], // Green for PCB
                uv: [0.0, 1.0],
            },
            // Bottom-right  
            PcbVertex {
                position: [half_width, -half_height, 0.0],
                color: [0.0, 1.0, 0.0, 1.0],
                uv: [1.0, 1.0],
            },
            // Top-right
            PcbVertex {
                position: [half_width, half_height, 0.0],
                color: [0.0, 1.0, 0.0, 1.0],
                uv: [1.0, 0.0],
            },
            // Top-left
            PcbVertex {
                position: [-half_width, half_height, 0.0],
                color: [0.0, 1.0, 0.0, 1.0],
                uv: [0.0, 0.0],
            },
        ]);

        // Create quad indices (two triangles)
        indices.extend_from_slice(&[
            0, 1, 2,  // First triangle
            2, 3, 0,  // Second triangle
        ]);

        (vertices, indices)
    }

    /// Render the PCB
    pub fn render<'a>(&'a self, render_pass: &mut wgpu::RenderPass<'a>) {
        if let (Some(pipeline), Some(vertex_buffer), Some(index_buffer)) = 
            (&self.render_pipeline, &self.vertex_buffer, &self.index_buffer) {
            
            render_pass.set_pipeline(pipeline);
            render_pass.set_vertex_buffer(0, vertex_buffer.slice(..));
            render_pass.set_index_buffer(index_buffer.slice(..), wgpu::IndexFormat::Uint16);
            render_pass.draw_indexed(0..self.index_count, 0, 0..1);
        }
    }
}

impl Default for PcbRenderer {
    fn default() -> Self {
        Self::new()
    }
}