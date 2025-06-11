mod pcb_3d;
mod simple_3d;

use simple_3d::Simple3DViewer;

pub struct Pcb3DViewer {
    initialized: bool,
    viewer: Simple3DViewer,
}

#[derive(PartialEq)]
pub enum ViewMode {
    Gerber2D,
    Pcb3D,
}

impl Pcb3DViewer {
    pub fn new() -> Result<Self, Box<dyn std::error::Error>> {
        Ok(Self {
            initialized: false,
            viewer: Simple3DViewer::new(),
        })
    }
    
    /// Build 3D PCB from the layer manager data
    pub fn build_from_layers(&mut self, layer_manager: &crate::layer_operations::LayerManager) {
        self.viewer.build_from_layers(layer_manager);
        self.initialized = true;
    }
    
    pub fn render(&mut self, ui: &mut egui::Ui) -> Result<(), Box<dyn std::error::Error>> {
        let available_size = ui.available_size();
        let size = egui::Vec2::new(
            available_size.x.max(400.0),
            available_size.y.max(400.0)
        );
        
        let (rect, _response) = ui.allocate_exact_size(size, egui::Sense::hover());
        
        // Render the 3D view
        self.viewer.render(ui, rect);
        
        Ok(())
    }
    
    pub fn is_initialized(&self) -> bool {
        self.initialized
    }
}