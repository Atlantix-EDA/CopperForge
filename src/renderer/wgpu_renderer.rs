//! Core WGPU renderer implementation

use wgpu::{
    Adapter, Buffer, BufferUsages, CommandEncoderDescriptor, Device, Instance, Queue,
    RenderPassColorAttachment, RenderPassDescriptor, RenderPipeline, Surface, SurfaceConfiguration,
    TextureFormat, TextureUsages, TextureView,
};
use glam::{Mat4, Vec3};
use crate::renderer::RenderSettings;

/// Main WGPU renderer that handles the graphics device and rendering pipeline
pub struct WgpuRenderer {
    pub instance: Instance,
    pub adapter: Adapter,
    pub device: Device,
    pub queue: Queue,
    pub surface_config: Option<SurfaceConfiguration>,
    pub render_pipeline: Option<RenderPipeline>,
    pub settings: RenderSettings,
    
    // Camera matrices
    pub view_matrix: Mat4,
    pub projection_matrix: Mat4,
    pub uniform_buffer: Option<Buffer>,
}

impl WgpuRenderer {
    /// Create a new WGPU renderer instance
    pub async fn new(settings: RenderSettings) -> Result<Self, WgpuRendererError> {
        // Create WGPU instance
        let instance = Instance::new(wgpu::InstanceDescriptor {
            backends: wgpu::Backends::all(),
            flags: wgpu::InstanceFlags::default(),
            dx12_shader_compiler: wgpu::Dx12Compiler::Fxc,
            gles_minor_version: wgpu::Gles3MinorVersion::Automatic,
        });

        // Request adapter
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                compatible_surface: None,
                force_fallback_adapter: false,
            })
            .await
            .ok_or(WgpuRendererError::AdapterRequestFailed)?;

        // Request device and queue
        let (device, queue) = adapter
            .request_device(
                &wgpu::DeviceDescriptor {
                    label: Some("KiForge Device"),
                    required_features: wgpu::Features::empty(),
                    required_limits: wgpu::Limits::default(),
                    memory_hints: wgpu::MemoryHints::Performance,
                },
                None,
            )
            .await
            .map_err(WgpuRendererError::DeviceRequestFailed)?;

        // Initialize default camera
        let view_matrix = Mat4::look_at_rh(
            Vec3::new(0.0, 0.0, 5.0),  // Camera position
            Vec3::new(0.0, 0.0, 0.0),  // Look at origin
            Vec3::new(0.0, 1.0, 0.0),  // Up vector
        );
        
        let projection_matrix = Mat4::perspective_rh(
            45.0_f32.to_radians(),  // FOV
            16.0 / 9.0,             // Aspect ratio (will be updated)
            0.1,                    // Near plane
            100.0,                  // Far plane
        );

        Ok(Self {
            instance,
            adapter,
            device,
            queue,
            surface_config: None,
            render_pipeline: None,
            settings,
            view_matrix,
            projection_matrix,
            uniform_buffer: None,
        })
    }

    /// Configure the surface for rendering
    pub fn configure_surface(&mut self, surface: &Surface, width: u32, height: u32) -> Result<(), WgpuRendererError> {
        let surface_caps = surface.get_capabilities(&self.adapter);
        let surface_format = surface_caps
            .formats
            .iter()
            .copied()
            .find(|f| f.is_srgb())
            .unwrap_or(surface_caps.formats[0]);

        let config = SurfaceConfiguration {
            usage: TextureUsages::RENDER_ATTACHMENT,
            format: surface_format,
            width,
            height,
            present_mode: surface_caps.present_modes[0],
            alpha_mode: surface_caps.alpha_modes[0],
            view_formats: vec![],
            desired_maximum_frame_latency: 2,
        };

        surface.configure(&self.device, &config);
        self.surface_config = Some(config);

        // Update projection matrix with correct aspect ratio
        self.projection_matrix = Mat4::perspective_rh(
            45.0_f32.to_radians(),
            width as f32 / height as f32,
            0.1,
            100.0,
        );

        // Create or update uniform buffer
        self.create_uniform_buffer()?;

        Ok(())
    }

    /// Create the uniform buffer for camera matrices
    fn create_uniform_buffer(&mut self) -> Result<(), WgpuRendererError> {
        // Camera uniform data (view matrix + projection matrix)
        let uniform_data = CameraUniforms {
            view_matrix: self.view_matrix.to_cols_array_2d(),
            projection_matrix: self.projection_matrix.to_cols_array_2d(),
        };

        use wgpu::util::DeviceExt;
        let uniform_buffer = self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Camera Uniform Buffer"),
            contents: bytemuck::cast_slice(&[uniform_data]),
            usage: BufferUsages::UNIFORM | BufferUsages::COPY_DST,
        });

        self.uniform_buffer = Some(uniform_buffer);
        Ok(())
    }

    /// Update camera matrices
    pub fn update_camera(&mut self, view_matrix: Mat4, projection_matrix: Mat4) -> Result<(), WgpuRendererError> {
        self.view_matrix = view_matrix;
        self.projection_matrix = projection_matrix;

        if let Some(ref uniform_buffer) = self.uniform_buffer {
            let uniform_data = CameraUniforms {
                view_matrix: self.view_matrix.to_cols_array_2d(),
                projection_matrix: self.projection_matrix.to_cols_array_2d(),
            };

            self.queue.write_buffer(
                uniform_buffer,
                0,
                bytemuck::cast_slice(&[uniform_data]),
            );
        }

        Ok(())
    }

    /// Begin a render pass  
    pub fn begin_render_pass<'a>(&'a self, encoder: &'a mut wgpu::CommandEncoder, view: &'a TextureView) -> wgpu::RenderPass<'a> {
        encoder.begin_render_pass(&RenderPassDescriptor {
            label: Some("KiForge Render Pass"),
            color_attachments: &[Some(RenderPassColorAttachment {
                view,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color {
                        r: self.settings.background_color[0] as f64,
                        g: self.settings.background_color[1] as f64,
                        b: self.settings.background_color[2] as f64,
                        a: self.settings.background_color[3] as f64,
                    }),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            occlusion_query_set: None,
            timestamp_writes: None,
        })
    }

    /// Get a reference to the device
    pub fn device(&self) -> &Device {
        &self.device
    }

    /// Get a reference to the queue
    pub fn queue(&self) -> &Queue {
        &self.queue
    }
}

/// Camera uniform data structure
#[repr(C)]
#[derive(Debug, Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
struct CameraUniforms {
    view_matrix: [[f32; 4]; 4],
    projection_matrix: [[f32; 4]; 4],
}

/// WGPU renderer error types
#[derive(Debug)]
pub enum WgpuRendererError {
    AdapterRequestFailed,
    DeviceRequestFailed(wgpu::RequestDeviceError),
    SurfaceError(wgpu::SurfaceError),
    BufferCreationFailed,
}

impl std::fmt::Display for WgpuRendererError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            WgpuRendererError::AdapterRequestFailed => write!(f, "Failed to request WGPU adapter"),
            WgpuRendererError::DeviceRequestFailed(e) => write!(f, "Failed to request WGPU device: {}", e),
            WgpuRendererError::SurfaceError(e) => write!(f, "WGPU surface error: {}", e),
            WgpuRendererError::BufferCreationFailed => write!(f, "Failed to create WGPU buffer"),
        }
    }
}

impl std::error::Error for WgpuRendererError {}