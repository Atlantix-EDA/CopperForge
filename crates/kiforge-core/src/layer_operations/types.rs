use eframe::epaint::Color32;
use gerber_viewer::GerberLayer;
use crate::navigation::LayerCoord;

/// Represents different PCB layers
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum LayerType {
    TopCopper,
    BottomCopper,
    TopSilk,
    BottomSilk,
    TopSoldermask,
    BottomSoldermask,
    TopPaste,
    BottomPaste,
    MechanicalOutline,
}

impl LayerType {
    pub fn all() -> Vec<Self> {
        vec![
            Self::TopCopper,
            Self::BottomCopper,
            Self::TopSilk,
            Self::BottomSilk,
            Self::TopSoldermask,
            Self::BottomSoldermask,
            Self::TopPaste,
            Self::BottomPaste,
            Self::MechanicalOutline,
        ]
    }
    
    pub fn display_name(&self) -> &'static str {
        match self {
            Self::TopCopper => "Top Copper",
            Self::BottomCopper => "Bottom Copper",
            Self::TopSilk => "Top Silk",
            Self::BottomSilk => "Bottom Silk",
            Self::TopSoldermask => "Top Soldermask",
            Self::BottomSoldermask => "Bottom Soldermask",
            Self::TopPaste => "Top Paste",
            Self::BottomPaste => "Bottom Paste",
            Self::MechanicalOutline => "Mechanical Outline",
        }
    }
    
    pub fn color(&self) -> Color32 {
        match self {
            Self::TopCopper => Color32::from_rgba_premultiplied(184, 115, 51, 220),      // Copper with transparency
            Self::BottomCopper => Color32::from_rgba_premultiplied(115, 184, 51, 220),   // Different copper color for bottom
            Self::TopSilk => Color32::from_rgba_premultiplied(255, 255, 255, 250),       // White silk
            Self::BottomSilk => Color32::from_rgba_premultiplied(255, 255, 255, 250),    // White silk
            Self::TopSoldermask => Color32::from_rgba_premultiplied(0, 132, 80, 180),    // Green with transparency
            Self::BottomSoldermask => Color32::from_rgba_premultiplied(0, 80, 132, 180), // Blue for bottom soldermask
            Self::TopPaste => Color32::from_rgba_premultiplied(192, 192, 192, 200),      // Light gray for paste
            Self::BottomPaste => Color32::from_rgba_premultiplied(128, 128, 128, 200),   // Darker gray for bottom paste
            Self::MechanicalOutline => Color32::from_rgba_premultiplied(255, 255, 0, 250), // Yellow outline
        }
    }
    
    pub fn filename(&self) -> &'static str {
        match self {
            Self::TopCopper => "cmod_s7-F_Cu.gbr",
            Self::BottomCopper => "cmod_s7-B_Cu.gbr",
            Self::TopSilk => "cmod_s7-F_SilkS.gbr",
            Self::BottomSilk => "cmod_s7-B_SilkS.gbr",
            Self::TopSoldermask => "cmod_s7-F_Mask.gbr",
            Self::BottomSoldermask => "cmod_s7-B_Mask.gbr",
            Self::TopPaste => "cmod_s7-F_Paste.gbr",
            Self::BottomPaste => "cmod_s7-B_Paste.gbr",
            Self::MechanicalOutline => "cmod_s7-Edge_Cuts.gbr",
        }
    }
    
    pub fn should_render(&self, showing_top: bool) -> bool {
        match self {
            Self::TopCopper | Self::TopSilk | Self::TopSoldermask | Self::TopPaste => showing_top,
            Self::BottomCopper | Self::BottomSilk | Self::BottomSoldermask | Self::BottomPaste => !showing_top,
            Self::MechanicalOutline => true, // Always show outline
        }
    }
}

/// Layer information including the gerber data, visibility, and coordinate tracking
#[derive(Debug)]
pub struct LayerInfo {
    pub layer_type: LayerType,
    pub gerber_layer: Option<GerberLayer>,
    pub raw_gerber_data: Option<String>,  // Store raw Gerber content for DRC parsing
    pub visible: bool,
    pub coordinates: Option<LayerCoord>,  // Coordinate tracking for positioning and export
    pub color: Color32,  // Customizable layer color
}

impl LayerInfo {
    pub fn new(layer_type: LayerType, gerber_layer: Option<GerberLayer>, raw_gerber_data: Option<String>, visible: bool) -> Self {
        Self {
            layer_type,
            gerber_layer,
            raw_gerber_data,
            visible,
            coordinates: None,  // Will be initialized when layer is positioned
            color: layer_type.color(),  // Initialize with default color
        }
    }
    
    /// Create LayerInfo with coordinate tracking
    pub fn with_coordinates(
        layer_type: LayerType, 
        gerber_layer: Option<GerberLayer>, 
        raw_gerber_data: Option<String>, 
        visible: bool,
        coordinates: LayerCoord
    ) -> Self {
        Self {
            layer_type,
            gerber_layer,
            raw_gerber_data,
            visible,
            coordinates: Some(coordinates),
            color: layer_type.color(),
        }
    }
    
    /// Initialize coordinates from gerber layer bounding box
    pub fn initialize_coordinates_from_gerber(&mut self) {
        if let Some(ref gerber) = self.gerber_layer {
            let bbox = gerber.bounding_box();
            let x_width = bbox.width() as f32;
            let y_height = bbox.height() as f32;
            let centroid = (
                bbox.center().x as f32,
                bbox.center().y as f32
            );
            
            // Initialize with default screen coordinates (will be updated by quadrant positioning)
            let default_screen_upper_left = (0.0, 0.0);
            let default_screen_lower_right = (x_width, y_height);
            
            
            self.coordinates = Some(LayerCoord::new(
                x_width,
                y_height,
                centroid,
                default_screen_upper_left,
                default_screen_lower_right
            ));
        }
    }
    
    /// Update screen positioning for quadrant view
    pub fn update_screen_position(&mut self, screen_upper_left: (f32, f32), screen_lower_right: (f32, f32)) {
        if let Some(ref mut coords) = self.coordinates {
            coords.update_screen_position(screen_upper_left, screen_lower_right);
        }
    }
    
    /// Get the positioned centroid (in traditional geometry space) if coordinates are available
    pub fn get_positioned_centroid(&self) -> Option<(f32, f32)> {
        self.coordinates.as_ref().map(|coords| coords.find_screen_centroid())
    }
    
    /// Convert gerber coordinates to positioned coordinates (in traditional geometry space) for this layer
    pub fn gerber_to_positioned(&self, gerber_x: f32, gerber_y: f32) -> Option<(f32, f32)> {
        self.coordinates.as_ref().map(|coords| coords.gerber_to_positioned(gerber_x, gerber_y))
    }
    
    /// Convert positioned coordinates (in traditional geometry space) to gerber coordinates for this layer
    pub fn positioned_to_gerber(&self, positioned_x: f32, positioned_y: f32) -> Option<(f32, f32)> {
        self.coordinates.as_ref().map(|coords| coords.positioned_to_gerber(positioned_x, positioned_y))
    }
    
    /// Check if a positioned point (in traditional geometry space) is within this layer's bounds
    pub fn contains_positioned_point(&self, positioned_x: f32, positioned_y: f32) -> bool {
        self.coordinates.as_ref()
            .map(|coords| coords.contains_positioned_point(positioned_x, positioned_y))
            .unwrap_or(false)
    }
}