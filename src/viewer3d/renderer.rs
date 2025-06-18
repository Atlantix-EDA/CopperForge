//! 3D Renderer for PCB meshes
//! 
//! Handles the actual wgpu rendering pipeline for 3D PCB visualization.

use crate::ecs::{Mesh3D, wgpu_renderer::PcbWgpuRenderer};
use crate::viewer3d::camera::Camera3D;
use std::sync::{Arc, Mutex};

/// High-level 3D renderer interface
pub struct Renderer3D {
    wgpu_renderer: Arc<Mutex<Option<PcbWgpuRenderer>>>,
    is_initialized: bool,
}

impl Renderer3D {
    pub fn new() -> Self {
        Self {
            wgpu_renderer: Arc::new(Mutex::new(None)),
            is_initialized: false,
        }
    }

    /// Initialize the renderer with wgpu resources
    pub fn initialize(&mut self, device: &wgpu::Device, format: wgpu::TextureFormat) {
        if !self.is_initialized {
            let renderer = PcbWgpuRenderer::new(device, format);
            *self.wgpu_renderer.lock().unwrap() = Some(renderer);
            self.is_initialized = true;
        }
    }

    /// Check if the renderer is initialized
    pub fn is_initialized(&self) -> bool {
        self.is_initialized
    }

    /// Upload meshes to GPU
    pub fn upload_meshes(&self, device: &wgpu::Device, meshes: &[Mesh3D]) {
        if let Ok(mut renderer_guard) = self.wgpu_renderer.lock() {
            if let Some(renderer) = renderer_guard.as_mut() {
                renderer.upload_meshes(device, meshes);
            }
        }
    }

    /// Update camera settings
    pub fn update_camera(&self, queue: &wgpu::Queue, camera: &Camera3D, width: f32, height: f32) {
        if let Ok(mut renderer_guard) = self.wgpu_renderer.lock() {
            if let Some(renderer) = renderer_guard.as_mut() {
                // Convert Camera3D to PcbCamera (they should be compatible)
                renderer.camera.eye = camera.eye;
                renderer.camera.target = camera.target;
                renderer.camera.up = camera.up;
                renderer.camera.fovy = camera.fovy;
                renderer.camera.znear = camera.znear;
                renderer.camera.zfar = camera.zfar;
                renderer.camera.aspect = camera.aspect;
                
                renderer.update_camera(queue, width, height);
            }
        }
    }

    /// Get a clone of the wgpu renderer for callback use
    pub fn get_wgpu_renderer(&self) -> Arc<Mutex<Option<PcbWgpuRenderer>>> {
        Arc::clone(&self.wgpu_renderer)
    }
}

/// Paint callback for egui-wgpu integration
pub struct WgpuPaintCallback {
    pub renderer: Arc<Mutex<Option<PcbWgpuRenderer>>>,
    pub meshes: Vec<Mesh3D>,
    pub camera: Camera3D,
    pub size: egui::Vec2,
}

impl egui_wgpu::CallbackTrait for WgpuPaintCallback {
    fn prepare(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        _screen_descriptor: &egui_wgpu::ScreenDescriptor,
        _egui_encoder: &mut wgpu::CommandEncoder,
        _resources: &mut egui_wgpu::CallbackResources,
    ) -> Vec<wgpu::CommandBuffer> {
        // Update renderer with meshes and camera
        if let Ok(mut renderer_guard) = self.renderer.lock() {
            if let Some(renderer) = renderer_guard.as_mut() {
                // Upload meshes if needed
                if !self.meshes.is_empty() {
                    renderer.upload_meshes(device, &self.meshes);
                }
                
                // Update camera
                renderer.camera.eye = self.camera.eye;
                renderer.camera.target = self.camera.target;
                renderer.camera.up = self.camera.up;
                renderer.camera.fovy = self.camera.fovy;
                renderer.camera.znear = self.camera.znear;
                renderer.camera.zfar = self.camera.zfar;
                renderer.camera.aspect = self.camera.aspect;
                
                renderer.update_camera(queue, self.size.x, self.size.y);
            }
        }
        
        Vec::new() // No additional command buffers needed
    }

    fn paint(
        &self,
        _info: egui::PaintCallbackInfo,
        render_pass: &mut wgpu::RenderPass<'_>,
        _resources: &egui_wgpu::CallbackResources,
    ) {
        // Set up render pipeline and draw meshes
        if let Ok(renderer_guard) = self.renderer.lock() {
            if let Some(renderer) = renderer_guard.as_ref() {
                // Set pipeline
                render_pass.set_pipeline(&renderer.render_pipeline);
                
                // Set bind groups
                render_pass.set_bind_group(0, &renderer.camera_bind_group, &[]);
                render_pass.set_bind_group(1, &renderer.materials.bind_group, &[]);
                
                // Draw all meshes
                for mesh in &renderer.meshes {
                    render_pass.set_vertex_buffer(0, mesh.vertex_buffer.slice(..));
                    render_pass.set_index_buffer(mesh.index_buffer.slice(..), wgpu::IndexFormat::Uint32);
                    render_pass.draw_indexed(0..mesh.index_count, 0, 0..1);
                }
            }
        }
    }
}