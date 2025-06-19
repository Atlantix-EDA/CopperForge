use serde::{Deserialize, Serialize};
use crate::layer_operations::{LayerType, LayerManager};

/// Serializable mirroring settings
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MirroringSettings {
    pub x: bool,
    pub y: bool,
}

impl From<MirroringSettings> for gerber_viewer::Mirroring {
    fn from(settings: MirroringSettings) -> Self {
        gerber_viewer::Mirroring {
            x: settings.x,
            y: settings.y,
        }
    }
}

impl From<gerber_viewer::Mirroring> for MirroringSettings {
    fn from(mirroring: gerber_viewer::Mirroring) -> Self {
        MirroringSettings {
            x: mirroring.x,
            y: mirroring.y,
        }
    }
}

/// Serializable vector for offsets
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VectorOffset {
    pub x: f64,
    pub y: f64,
}

impl From<VectorOffset> for nalgebra::Vector2<f64> {
    fn from(offset: VectorOffset) -> Self {
        nalgebra::Vector2::new(offset.x, offset.y)
    }
}

impl From<nalgebra::Vector2<f64>> for VectorOffset {
    fn from(vector: nalgebra::Vector2<f64>) -> Self {
        VectorOffset {
            x: vector.x,
            y: vector.y,
        }
    }
}

/// Manager for all display-related properties and settings
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DisplayManager {
    /// Enable unique colors for different shapes
    pub enable_unique_colors: bool,
    
    /// Enable polygon numbering overlay
    pub enable_polygon_numbering: bool,
    
    /// Mirroring settings for X and Y axes
    pub mirroring: MirroringSettings,
    
    /// Center offset for the display
    pub center_offset: VectorOffset,
    
    /// Design offset for positioning (calculated from mechanical centroid + user delta)
    pub design_offset: VectorOffset,
    
    /// User-adjustable delta offset for fine-tuning the design center
    pub user_delta_offset: VectorOffset,
    
    /// View orientation: true = top layers, false = bottom layers
    pub showing_top: bool,
    
    /// Enable quadrant view mode for spreading layers
    pub quadrant_view_enabled: bool,
    
    /// Offset magnitude for quadrant view (in mm)
    pub quadrant_offset_magnitude: f64,
}

impl DisplayManager {
    /// Create a new DisplayManager with default settings
    pub fn new() -> Self {
        Self {
            enable_unique_colors: false, // Matches ENABLE_UNIQUE_SHAPE_COLORS constant
            enable_polygon_numbering: false, // Matches ENABLE_POLYGON_NUMBERING constant
            mirroring: MirroringSettings { x: false, y: false },
            center_offset: VectorOffset { x: 0.0, y: 0.0 },
            design_offset: VectorOffset { x: 0.0, y: 0.0 },
            user_delta_offset: VectorOffset { x: 0.0, y: 0.0 },
            showing_top: true,
            quadrant_view_enabled: false,
            quadrant_offset_magnitude: 141.42, // Default ~100mil in x and y (sqrt(100^2 + 100^2) * 0.0254)
        }
    }
    
    /// Toggle between top and bottom view
    pub fn flip_view(&mut self) {
        self.showing_top = !self.showing_top;
    }
    
    /// Reset offsets to center the view
    pub fn reset_offsets(&mut self) {
        self.center_offset = VectorOffset { x: 0.0, y: 0.0 };
        self.design_offset = VectorOffset { x: 0.0, y: 0.0 };
        self.user_delta_offset = VectorOffset { x: 0.0, y: 0.0 };
    }
    
    /// Update design offset based on mechanical outline centroid + user delta
    pub fn update_design_offset(&mut self, layer_manager: &crate::layer_operations::LayerManager) {
        if let Some((centroid_x, centroid_y)) = layer_manager.get_mechanical_outline_centroid() {
            self.design_offset = VectorOffset {
                x: centroid_x + self.user_delta_offset.x,
                y: centroid_y + self.user_delta_offset.y,
            };
            println!("🔧 Updated design offset: mechanical_centroid({:.2}, {:.2}) + user_delta({:.2}, {:.2}) = ({:.2}, {:.2})", 
                     centroid_x, centroid_y, 
                     self.user_delta_offset.x, self.user_delta_offset.y,
                     self.design_offset.x, self.design_offset.y);
        } else {
            // Fallback: use only user delta if no mechanical outline
            self.design_offset = self.user_delta_offset.clone();
            println!("⚠️ No mechanical outline found, using user delta as design offset: ({:.2}, {:.2})", 
                     self.design_offset.x, self.design_offset.y);
        }
    }
    
    /// Toggle X-axis mirroring
    pub fn toggle_x_mirror(&mut self) {
        self.mirroring.x = !self.mirroring.x;
    }
    
    /// Toggle Y-axis mirroring
    pub fn toggle_y_mirror(&mut self) {
        self.mirroring.y = !self.mirroring.y;
    }
    
    /// Get a descriptive string for the current view
    pub fn get_view_description(&self) -> &'static str {
        if self.showing_top {
            "Top View"
        } else {
            "Bottom View"
        }
    }
    
    /// Check if any mirroring is active
    pub fn is_mirrored(&self) -> bool {
        self.mirroring.x || self.mirroring.y
    }
    
    /// Get mirroring description
    pub fn get_mirroring_description(&self) -> String {
        match (self.mirroring.x, self.mirroring.y) {
            (false, false) => "No mirroring".to_string(),
            (true, false) => "X-axis mirrored".to_string(),
            (false, true) => "Y-axis mirrored".to_string(),
            (true, true) => "X and Y mirrored".to_string(),
        }
    }
    
    /// Toggle quadrant view mode
    pub fn toggle_quadrant_view(&mut self) {
        self.quadrant_view_enabled = !self.quadrant_view_enabled;
    }
    
    /// Get the quadrant offset for a specific layer type
    /// Returns (x_offset, y_offset) in mm
    pub fn get_quadrant_offset(&self, layer_type: &crate::layer_operations::LayerType) -> VectorOffset {
        if !self.quadrant_view_enabled {
            return VectorOffset { x: 0.0, y: 0.0 };
        }
        
        // Ensure we have a valid offset magnitude (minimum 1mm to avoid issues)
        let safe_magnitude = self.quadrant_offset_magnitude.max(1.0);
        
        // Calculate base offset - divide by sqrt(2) to get X and Y components
        let base_offset = safe_magnitude / std::f64::consts::SQRT_2;
        
        // Determine quadrant based on layer type
        // Quadrant 1 (top-right): Copper layers
        // Quadrant 2 (top-left): Silkscreen layers  
        // Quadrant 3 (bottom-left): Soldermask layers
        // Quadrant 4 (bottom-right): Paste layers
        
        use crate::layer_operations::LayerType;
        
        let (x_mult, y_mult) = match layer_type {
            // Copper layers - Quadrant 1 (top-right)
            LayerType::TopCopper | LayerType::BottomCopper => (1.0, 1.0),
            
            // Silkscreen layers - Quadrant 2 (top-left)
            LayerType::TopSilk | LayerType::BottomSilk => (-1.0, 1.0),
            
            // Soldermask layers - Quadrant 3 (bottom-left)
            LayerType::TopSoldermask | LayerType::BottomSoldermask => (-1.0, -1.0),
            
            // Paste layers - Quadrant 4 (bottom-right)
            LayerType::TopPaste | LayerType::BottomPaste => (1.0, -1.0),
            
            // Mechanical outline should not be displayed in quadrant view
            // (it will be rendered separately with each layer)
            LayerType::MechanicalOutline => (0.0, 0.0),
        };
        
        VectorOffset {
            x: base_offset * x_mult,
            y: base_offset * y_mult,
        }
    }
    
    /// Set the quadrant offset magnitude in mm
    pub fn set_quadrant_offset_magnitude(&mut self, magnitude_mm: f64) {
        // Ensure magnitude is finite and positive, with reasonable bounds
        if magnitude_mm.is_finite() && magnitude_mm >= 0.0 {
            self.quadrant_offset_magnitude = magnitude_mm.clamp(0.1, 1000.0); // 0.1mm to 1m max
        }
    }
    
    /// Set the quadrant offset magnitude in mils
    pub fn set_quadrant_offset_magnitude_mils(&mut self, magnitude_mils: f64) {
        if magnitude_mils.is_finite() && magnitude_mils >= 0.0 {
            let magnitude_mm = magnitude_mils * 0.0254;
            self.set_quadrant_offset_magnitude(magnitude_mm);
        }
    }
    
    /// Update all layer positions based on quadrant view settings
    /// This properly positions layers in traditional geometry space
    pub fn update_layer_positions(&self, layer_manager: &mut LayerManager) {
        // Always mark coordinates as updated when we run this
        let _should_update = layer_manager.coordinates_need_update() || 
                           self.quadrant_view_enabled; // Always update in quadrant mode
        // First, get the mechanical outline to determine the base size
        let mechanical_size = if let Some(mechanical_layer) = layer_manager.get_layer(&LayerType::MechanicalOutline) {
            if let Some(ref coords) = mechanical_layer.coordinates {
                (coords.x_width, coords.y_height)
            } else {
                (100.0, 100.0) // Default size if no coordinates
            }
        } else {
            (100.0, 100.0) // Default size if no mechanical outline
        };
        
        // Calculate spacing between quadrants
        let spacing = if self.quadrant_view_enabled {
            self.quadrant_offset_magnitude.max(1.0)
        } else {
            0.0 // No spacing if quadrant view disabled
        };
        
        // Update each layer's position
        for (layer_type, layer_info) in layer_manager.layers.iter_mut() {
            if let Some(ref _coords) = layer_info.coordinates {
                let (screen_upper_left, screen_lower_right) = if self.quadrant_view_enabled {
                    self.calculate_quadrant_position(layer_type, mechanical_size.0, mechanical_size.1, spacing)
                } else {
                    // All layers centered at origin when quadrant view is disabled
                    let half_width = mechanical_size.0 / 2.0;
                    let half_height = mechanical_size.1 / 2.0;
                    (
                        (-half_width, half_height),   // Upper left in traditional coords
                        (half_width, -half_height)     // Lower right in traditional coords
                    )
                };
                
                layer_info.update_screen_position(screen_upper_left, screen_lower_right);
            }
        }
    }
    
    /// Calculate the positioned bounds for a layer in quadrant view
    fn calculate_quadrant_position(
        &self,
        layer_type: &LayerType,
        width: f32,
        height: f32,
        spacing: f64,
    ) -> ((f32, f32), (f32, f32)) {
        let half_width = width / 2.0;
        let half_height = height / 2.0;
        let offset = spacing as f32; // Full spacing from center to quadrant center
        
        // Calculate quadrant centers and then position the layer bounds
        // Remember: In traditional geometry, Y increases upward
        match layer_type {
            // Quadrant 1 (top-right): Copper layers
            LayerType::TopCopper | LayerType::BottomCopper => {
                let center_x = offset;
                let center_y = offset;
                (
                    (center_x - half_width, center_y + half_height),   // Upper left
                    (center_x + half_width, center_y - half_height)    // Lower right
                )
            },
            
            // Quadrant 2 (top-left): Silkscreen layers  
            LayerType::TopSilk | LayerType::BottomSilk => {
                let center_x = -offset;
                let center_y = offset;
                (
                    (center_x - half_width, center_y + half_height),   // Upper left
                    (center_x + half_width, center_y - half_height)    // Lower right
                )
            },
            
            // Quadrant 3 (bottom-left): Soldermask layers
            LayerType::TopSoldermask | LayerType::BottomSoldermask => {
                let center_x = -offset;
                let center_y = -offset;
                (
                    (center_x - half_width, center_y + half_height),   // Upper left
                    (center_x + half_width, center_y - half_height)    // Lower right
                )
            },
            
            // Quadrant 4 (bottom-right): Paste layers
            LayerType::TopPaste | LayerType::BottomPaste => {
                let center_x = offset;
                let center_y = -offset;
                (
                    (center_x - half_width, center_y + half_height),   // Upper left
                    (center_x + half_width, center_y - half_height)    // Lower right
                )
            },
            
            // Mechanical outline centered at origin
            LayerType::MechanicalOutline => (
                (-half_width, half_height),                      // Upper left
                (half_width, -half_height)                       // Lower right
            ),
        }
    }
}

impl Default for DisplayManager {
    fn default() -> Self {
        Self::new()
    }
}

// Helper trait for vector conversions
pub trait ToPosition {
    fn to_position(self) -> crate::drc_operations::types::Position;
}

impl ToPosition for nalgebra::Vector2<f64> {
    fn to_position(self) -> crate::drc_operations::types::Position {
        crate::drc_operations::types::Position { x: self.x, y: self.y }
    }
}