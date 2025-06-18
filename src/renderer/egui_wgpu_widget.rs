//! Custom egui widget for WGPU rendering integration

use egui::{Response, Sense, Ui, Widget, Vec2, Color32};
use crate::renderer::{WgpuRenderer, PcbRenderer, RenderSettings};
use gerber_viewer::GerberLayer;

/// Custom widget that displays WGPU-rendered content within egui
pub struct WgpuWidget {
    size: Vec2,
    renderer: Option<WgpuRenderer>,
    pcb_renderer: Option<PcbRenderer>,
    initialized: bool,
    initialization_error: Option<String>,
}

impl WgpuWidget {
    /// Create a new WGPU widget
    pub fn new(size: Vec2) -> Self {
        Self {
            size,
            renderer: None,
            pcb_renderer: None,
            initialized: false,
            initialization_error: None,
        }
    }

    /// Try to initialize WGPU renderer (non-blocking)
    pub fn try_initialize(&mut self) {
        if self.initialized || self.initialization_error.is_some() {
            return;
        }

        // For now, simulate initialization with a placeholder
        // In a real implementation, you'd need to handle async WGPU init differently
        // This could be done with a background thread or by using pollster::block_on
        match self.attempt_sync_init() {
            Ok(()) => {
                self.initialized = true;
                self.initialization_error = None;
            }
            Err(e) => {
                self.initialization_error = Some(e);
                self.initialized = false;
            }
        }
    }

    /// Attempt synchronous initialization (blocking)
    fn attempt_sync_init(&mut self) -> Result<(), String> {
        // Try to create a basic WGPU instance to test availability
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
            backends: wgpu::Backends::all(),
            flags: wgpu::InstanceFlags::default(),
            dx12_shader_compiler: wgpu::Dx12Compiler::Fxc,
            gles_minor_version: wgpu::Gles3MinorVersion::Automatic,
        });

        // Use pollster to block on async operations for simplicity
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::default(),
            compatible_surface: None,
            force_fallback_adapter: false,
        }))
        .ok_or("No suitable GPU adapter found")?;

        // Test device creation
        let (_device, _queue) = pollster::block_on(adapter.request_device(
            &wgpu::DeviceDescriptor {
                label: Some("KiForge Test Device"),
                required_features: wgpu::Features::empty(),
                required_limits: wgpu::Limits::default(),
                memory_hints: wgpu::MemoryHints::Performance,
            },
            None,
        ))
        .map_err(|e| format!("Failed to create WGPU device: {}", e))?;

        Ok(())
    }

    /// Load gerber data for rendering
    pub fn load_gerber_layer(&mut self, gerber_layer: &GerberLayer) -> Result<(), String> {
        if let (Some(renderer), Some(pcb_renderer)) = (&self.renderer, &mut self.pcb_renderer) {
            pcb_renderer.load_gerber_layer(renderer, gerber_layer)
                .map_err(|e| format!("Failed to load gerber layer: {}", e))?;
        }
        Ok(())
    }

    /// Check if the widget is initialized
    pub fn is_initialized(&self) -> bool {
        self.initialized
    }

    /// Get initialization error if any
    pub fn initialization_error(&self) -> Option<&String> {
        self.initialization_error.as_ref()
    }

    /// Update the widget size
    pub fn set_size(&mut self, size: Vec2) {
        self.size = size;
    }

    /// Render the widget content without consuming self
    pub fn show(&self, ui: &mut egui::Ui) -> egui::Response {
        let (rect, response) = ui.allocate_exact_size(self.size, egui::Sense::click_and_drag());

        if self.initialized {
            // For now, render a placeholder showing that WGPU integration is ready
            let painter = ui.painter();
            
            // Draw background
            painter.rect_filled(rect, 0.0, egui::Color32::from_rgb(20, 20, 30));
            
            // Draw border
            painter.rect_stroke(rect, 0.0, egui::Stroke::new(1.0, egui::Color32::from_rgb(100, 100, 100)), egui::StrokeKind::Middle);
            
            // Draw status text
            let text = "WGPU Renderer Initialized\nReady for PCB rendering";
            painter.text(
                rect.center(),
                egui::Align2::CENTER_CENTER,
                text,
                egui::FontId::default(),
                egui::Color32::from_rgb(100, 200, 100),
            );

            // TODO: Actual WGPU rendering would happen here
            // This is where we would:
            // 1. Get the wgpu surface from egui's render state
            // 2. Render our PCB content to a texture
            // 3. Display that texture in the egui widget area
        } else {
            // Show initialization message or error
            let painter = ui.painter();
            
            let (bg_color, border_color, text_color, text) = if let Some(error) = &self.initialization_error {
                (
                    egui::Color32::from_rgb(60, 20, 20),
                    egui::Color32::from_rgb(255, 100, 100),
                    egui::Color32::from_rgb(255, 150, 150),
                    format!("WGPU Initialization Failed:\n{}", error)
                )
            } else {
                (
                    egui::Color32::from_rgb(40, 20, 20),
                    egui::Color32::from_rgb(200, 100, 100),
                    egui::Color32::from_rgb(200, 100, 100),
                    "WGPU Renderer\nNot Initialized\nClick 'Initialize WGPU' to try again".to_string()
                )
            };
            
            painter.rect_filled(rect, 0.0, bg_color);
            painter.rect_stroke(rect, 0.0, egui::Stroke::new(1.0, border_color), egui::StrokeKind::Middle);
            
            painter.text(
                rect.center(),
                egui::Align2::CENTER_CENTER,
                text,
                egui::FontId::default(),
                text_color,
            );
        }

        response
    }
}

impl Widget for WgpuWidget {
    fn ui(self, ui: &mut Ui) -> Response {
        let (rect, response) = ui.allocate_exact_size(self.size, Sense::click_and_drag());

        if self.initialized {
            // For now, render a placeholder showing that WGPU integration is ready
            let painter = ui.painter();
            
            // Draw background
            painter.rect_filled(rect, 0.0, Color32::from_rgb(20, 20, 30));
            
            // Draw border
            painter.rect_stroke(rect, 0.0, egui::Stroke::new(1.0, Color32::from_rgb(100, 100, 100)), egui::StrokeKind::Middle);
            
            // Draw status text
            let text = "WGPU Renderer Initialized\nReady for PCB rendering";
            painter.text(
                rect.center(),
                egui::Align2::CENTER_CENTER,
                text,
                egui::FontId::default(),
                Color32::from_rgb(100, 200, 100),
            );

            // TODO: Actual WGPU rendering would happen here
            // This is where we would:
            // 1. Get the wgpu surface from egui's render state
            // 2. Render our PCB content to a texture
            // 3. Display that texture in the egui widget area
        } else {
            // Show initialization message or error
            let painter = ui.painter();
            
            let (bg_color, border_color, text_color, text) = if let Some(error) = &self.initialization_error {
                (
                    Color32::from_rgb(60, 20, 20),
                    Color32::from_rgb(255, 100, 100),
                    Color32::from_rgb(255, 150, 150),
                    format!("WGPU Initialization Failed:\n{}", error)
                )
            } else {
                (
                    Color32::from_rgb(40, 20, 20),
                    Color32::from_rgb(200, 100, 100),
                    Color32::from_rgb(200, 100, 100),
                    "WGPU Renderer\nNot Initialized\nClick 'Initialize WGPU' to try again".to_string()
                )
            };
            
            painter.rect_filled(rect, 0.0, bg_color);
            painter.rect_stroke(rect, 0.0, egui::Stroke::new(1.0, border_color), egui::StrokeKind::Middle);
            
            painter.text(
                rect.center(),
                egui::Align2::CENTER_CENTER,
                text,
                egui::FontId::default(),
                text_color,
            );
        }

        response
    }
}

/// Helper function to create and initialize a WGPU widget
pub fn create_wgpu_widget(size: Vec2) -> WgpuWidget {
    let mut widget = WgpuWidget::new(size);
    widget.try_initialize();
    widget
}