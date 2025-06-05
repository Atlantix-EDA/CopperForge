use serde::{Deserialize, Serialize};

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

impl From<VectorOffset> for gerber_viewer::position::Vector {
    fn from(offset: VectorOffset) -> Self {
        gerber_viewer::position::Vector::new(offset.x, offset.y)
    }
}

impl From<gerber_viewer::position::Vector> for VectorOffset {
    fn from(vector: gerber_viewer::position::Vector) -> Self {
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
    
    /// Design offset for positioning
    pub design_offset: VectorOffset,
    
    /// View orientation: true = top layers, false = bottom layers
    pub showing_top: bool,
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
            showing_top: true,
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
}

impl Default for DisplayManager {
    fn default() -> Self {
        Self::new()
    }
}