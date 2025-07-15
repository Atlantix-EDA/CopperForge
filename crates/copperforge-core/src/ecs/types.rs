use egui::Color32;

/// Represents different PCB layers - redesigned to support multi-layer PCBs
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum LayerType {
    /// Copper layers by number (1=top, 2,3,4...N=inner, N+1=bottom)
    Copper(u8),
    /// Silkscreen (text/component outlines) - only top/bottom
    Silkscreen(Side),
    /// Soldermask (solder resist) - only top/bottom  
    Soldermask(Side),
    /// Paste (solder paste stencil) - only top/bottom
    Paste(Side),
    /// Board outline/mechanical edges
    MechanicalOutline,
}

/// PCB side designation for non-copper layers
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum Side {
    Top,
    Bottom,
}

impl LayerType {
    // Backwards compatibility constants for old 2-layer enum style
    pub const TOP_COPPER: LayerType = LayerType::Copper(1);
    pub const BOTTOM_COPPER: LayerType = LayerType::Copper(2);
    pub const TOP_SILK: LayerType = LayerType::Silkscreen(Side::Top);
    pub const BOTTOM_SILK: LayerType = LayerType::Silkscreen(Side::Bottom);
    pub const TOP_SOLDERMASK: LayerType = LayerType::Soldermask(Side::Top);
    pub const BOTTOM_SOLDERMASK: LayerType = LayerType::Soldermask(Side::Bottom);
    pub const TOP_PASTE: LayerType = LayerType::Paste(Side::Top);
    pub const BOTTOM_PASTE: LayerType = LayerType::Paste(Side::Bottom);
    
    // Note: MechanicalOutline remains the same
    /// Get standard 2-layer PCB layer types (for backwards compatibility)
    pub fn standard_2_layer() -> Vec<Self> {
        vec![
            Self::Copper(1),                    // Top copper
            Self::Copper(2),                    // Bottom copper  
            Self::Silkscreen(Side::Top),
            Self::Silkscreen(Side::Bottom),
            Self::Soldermask(Side::Top),
            Self::Soldermask(Side::Bottom),
            Self::Paste(Side::Top),
            Self::Paste(Side::Bottom),
            Self::MechanicalOutline,
        ]
    }
    
    /// Get standard 4-layer PCB layer types
    pub fn standard_4_layer() -> Vec<Self> {
        vec![
            Self::Copper(1),                    // Top copper
            Self::Copper(2),                    // Inner 1
            Self::Copper(3),                    // Inner 2
            Self::Copper(4),                    // Bottom copper
            Self::Silkscreen(Side::Top),
            Self::Silkscreen(Side::Bottom),
            Self::Soldermask(Side::Top),
            Self::Soldermask(Side::Bottom),
            Self::Paste(Side::Top),
            Self::Paste(Side::Bottom),
            Self::MechanicalOutline,
        ]
    }
    
    /// Generate layer types for N-layer PCB
    pub fn for_layer_count(layer_count: u8) -> Vec<Self> {
        let mut layers = Vec::new();
        
        // Add copper layers (1 to layer_count)
        for i in 1..=layer_count {
            layers.push(Self::Copper(i));
        }
        
        // Add non-copper layers (only top/bottom exist)
        layers.extend_from_slice(&[
            Self::Silkscreen(Side::Top),
            Self::Silkscreen(Side::Bottom),
            Self::Soldermask(Side::Top),
            Self::Soldermask(Side::Bottom),
            Self::Paste(Side::Top),
            Self::Paste(Side::Bottom),
            Self::MechanicalOutline,
        ]);
        
        layers
    }
    
    /// Default to 2-layer for backwards compatibility
    pub fn all() -> Vec<Self> {
        Self::standard_2_layer()
    }
    
    pub fn display_name(&self) -> String {
        match self {
            Self::Copper(1) => "Top Copper (L1)".to_string(),
            Self::Copper(n) if *n == 2 => "Bottom Copper (L2)".to_string(), // For 2-layer
            Self::Copper(n) => format!("Inner Copper (L{})", n),
            Self::Silkscreen(Side::Top) => "Top Silkscreen".to_string(),
            Self::Silkscreen(Side::Bottom) => "Bottom Silkscreen".to_string(),
            Self::Soldermask(Side::Top) => "Top Soldermask".to_string(),
            Self::Soldermask(Side::Bottom) => "Bottom Soldermask".to_string(),
            Self::Paste(Side::Top) => "Top Paste".to_string(),
            Self::Paste(Side::Bottom) => "Bottom Paste".to_string(),
            Self::MechanicalOutline => "Mechanical Outline".to_string(),
        }
    }
    
    /// Get dynamic display name based on total layer count
    pub fn display_name_with_context(&self, total_copper_layers: u8) -> String {
        match self {
            Self::Copper(1) => "Top Copper (L1)".to_string(),
            Self::Copper(n) if *n == total_copper_layers => format!("Bottom Copper (L{})", n),
            Self::Copper(n) => format!("Inner Copper (L{})", n),
            Self::Silkscreen(Side::Top) => "Top Silkscreen".to_string(),
            Self::Silkscreen(Side::Bottom) => "Bottom Silkscreen".to_string(),
            Self::Soldermask(Side::Top) => "Top Soldermask".to_string(),
            Self::Soldermask(Side::Bottom) => "Bottom Soldermask".to_string(),
            Self::Paste(Side::Top) => "Top Paste".to_string(),
            Self::Paste(Side::Bottom) => "Bottom Paste".to_string(),
            Self::MechanicalOutline => "Mechanical Outline".to_string(),
        }
    }
    
    pub fn color(&self) -> Color32 {
        match self {
            Self::Copper(1) => Color32::from_rgba_premultiplied(184, 115, 51, 220),     // Top: copper
            Self::Copper(2) => Color32::from_rgba_premultiplied(115, 184, 51, 220),     // Bottom: green copper
            Self::Copper(n) => {
                // Inner layers: different colors for each
                let hue = (*n as f32 * 60.0) % 360.0; // Spread colors around hue wheel
                let (r, g, b) = hsv_to_rgb(hue, 0.7, 0.8);
                Color32::from_rgba_premultiplied((r * 255.0) as u8, (g * 255.0) as u8, (b * 255.0) as u8, 220)
            },
            Self::Silkscreen(_) => Color32::from_rgba_premultiplied(255, 255, 255, 250),
            Self::Soldermask(Side::Top) => Color32::from_rgba_premultiplied(0, 132, 80, 180),    // Green
            Self::Soldermask(Side::Bottom) => Color32::from_rgba_premultiplied(0, 80, 132, 180), // Blue
            Self::Paste(Side::Top) => Color32::from_rgba_premultiplied(192, 192, 192, 200),
            Self::Paste(Side::Bottom) => Color32::from_rgba_premultiplied(128, 128, 128, 200),
            Self::MechanicalOutline => Color32::from_rgba_premultiplied(255, 255, 0, 250),
        }
    }
    
    pub fn should_render(&self, showing_top: bool) -> bool {
        match self {
            Self::Copper(1) => showing_top,                               // Top copper
            Self::Copper(_) => true,                                      // Inner + bottom always visible
            Self::Silkscreen(Side::Top) | Self::Soldermask(Side::Top) | Self::Paste(Side::Top) => showing_top,
            Self::Silkscreen(Side::Bottom) | Self::Soldermask(Side::Bottom) | Self::Paste(Side::Bottom) => !showing_top,
            Self::MechanicalOutline => true,                              // Always show outline
        }
    }
    
    /// Check if this is a copper layer
    pub fn is_copper(&self) -> bool {
        matches!(self, Self::Copper(_))
    }
    
    /// Get copper layer number (if copper layer)
    pub fn copper_layer_number(&self) -> Option<u8> {
        match self {
            Self::Copper(n) => Some(*n),
            _ => None,
        }
    }
    
    /// Check if this is top layer (L1 copper or top side)
    pub fn is_top(&self) -> bool {
        match self {
            Self::Copper(1) => true,
            Self::Silkscreen(Side::Top) | Self::Soldermask(Side::Top) | Self::Paste(Side::Top) => true,
            _ => false,
        }
    }
    
    /// Check if this is bottom layer (highest copper or bottom side)
    pub fn is_bottom(&self, total_copper_layers: u8) -> bool {
        match self {
            Self::Copper(n) => *n == total_copper_layers,
            Self::Silkscreen(Side::Bottom) | Self::Soldermask(Side::Bottom) | Self::Paste(Side::Bottom) => true,
            _ => false,
        }
    }
}

/// Helper function to convert HSV to RGB
fn hsv_to_rgb(h: f32, s: f32, v: f32) -> (f32, f32, f32) {
    let c = v * s;
    let x = c * (1.0 - ((h / 60.0) % 2.0 - 1.0).abs());
    let m = v - c;
    
    let (r, g, b) = if h < 60.0 {
        (c, x, 0.0)
    } else if h < 120.0 {
        (x, c, 0.0)
    } else if h < 180.0 {
        (0.0, c, x)
    } else if h < 240.0 {
        (0.0, x, c)
    } else if h < 300.0 {
        (x, 0.0, c)
    } else {
        (c, 0.0, x)
    };
    
    (r + m, g + m, b + m)
}