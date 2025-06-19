use serde::{Deserialize, Serialize};

/// Comprehensive coordinate tracking for gerber layers
/// Combines physical gerber dimensions with screen positioning for proper coordinate management
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LayerCoord {
    /// Physical width of the gerber layer (should be same for all layers)
    pub x_width: f32,
    
    /// Physical height of the gerber layer (should be same for all layers)  
    pub y_height: f32,
    
    /// Physical centroid of the gerber layer (should be same for all layers)
    pub centroid: (f32, f32),  // (x, y) centroid in gerber coordinates
    
    /// Screen position of upper-left corner (unique per layer in quadrant view)
    pub screen_upper_left: (f32, f32),  // (x, y) in screen coordinates
    
    /// Screen position of lower-right corner (unique per layer in quadrant view)  
    pub screen_lower_right: (f32, f32), // (x, y) in screen coordinates
}

impl LayerCoord {
    /// Create a new LayerCoord from gerber dimensions and screen positioning
    pub fn new(
        x_width: f32,
        y_height: f32, 
        centroid: (f32, f32),
        screen_upper_left: (f32, f32),
        screen_lower_right: (f32, f32)
    ) -> Self {
        Self {
            x_width,
            y_height,
            centroid,
            screen_upper_left,
            screen_lower_right,
        }
    }
    
    /// Calculate the centroid in screen coordinates
    pub fn find_screen_centroid(&self) -> (f32, f32) {
        let screen_center_x = (self.screen_upper_left.0 + self.screen_lower_right.0) / 2.0;
        let screen_center_y = (self.screen_upper_left.1 + self.screen_lower_right.1) / 2.0;
        (screen_center_x, screen_center_y)
    }
    
    /// Get the screen width of the layer
    pub fn screen_width(&self) -> f32 {
        (self.screen_lower_right.0 - self.screen_upper_left.0).abs()
    }
    
    /// Get the screen height of the layer  
    pub fn screen_height(&self) -> f32 {
        (self.screen_lower_right.1 - self.screen_upper_left.1).abs()
    }
    
    /// Convert a physical gerber coordinate to positioned coordinate in traditional geometry space
    pub fn gerber_to_positioned(&self, gerber_x: f32, gerber_y: f32) -> (f32, f32) {
        // Calculate relative position within the gerber (0.0 to 1.0)
        let rel_x = (gerber_x - (self.centroid.0 - self.x_width / 2.0)) / self.x_width;
        let rel_y = (gerber_y - (self.centroid.1 - self.y_height / 2.0)) / self.y_height;
        
        // Map to positioned coordinates within traditional geometry space
        let positioned_x = self.screen_upper_left.0 + (rel_x * self.screen_width());
        let positioned_y = self.screen_upper_left.1 + (rel_y * self.screen_height());
        
        (positioned_x, positioned_y)
    }
    
    /// Convert a positioned coordinate in traditional geometry space to physical gerber coordinate
    pub fn positioned_to_gerber(&self, positioned_x: f32, positioned_y: f32) -> (f32, f32) {
        // Calculate relative position within positioned area (0.0 to 1.0)
        let rel_x = (positioned_x - self.screen_upper_left.0) / self.screen_width();
        let rel_y = (positioned_y - self.screen_upper_left.1) / self.screen_height();
        
        // Map to gerber coordinates
        let gerber_x = (self.centroid.0 - self.x_width / 2.0) + (rel_x * self.x_width);
        let gerber_y = (self.centroid.1 - self.y_height / 2.0) + (rel_y * self.y_height);
        
        (gerber_x, gerber_y)
    }
    
    /// Check if a positioned coordinate (in traditional geometry space) is within this layer's bounds
    pub fn contains_positioned_point(&self, positioned_x: f32, positioned_y: f32) -> bool {
        positioned_x >= self.screen_upper_left.0 && 
        positioned_x <= self.screen_lower_right.0 &&
        positioned_y >= self.screen_upper_left.1 && 
        positioned_y <= self.screen_lower_right.1
    }
    
    /// Get the physical aspect ratio of the gerber
    pub fn aspect_ratio(&self) -> f32 {
        self.x_width / self.y_height
    }
    
    /// Update screen positioning (for quadrant view changes)
    pub fn update_screen_position(&mut self, screen_upper_left: (f32, f32), screen_lower_right: (f32, f32)) {
        self.screen_upper_left = screen_upper_left;
        self.screen_lower_right = screen_lower_right;
    }
}

impl Default for LayerCoord {
    fn default() -> Self {
        Self {
            x_width: 100.0,
            y_height: 100.0,
            centroid: (0.0, 0.0),
            screen_upper_left: (0.0, 0.0),
            screen_lower_right: (100.0, 100.0),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_screen_centroid_calculation() {
        let coord = LayerCoord::new(
            50.0, 30.0,           // Physical dimensions
            (25.0, 15.0),         // Physical centroid  
            (100.0, 200.0),       // Screen upper-left
            (300.0, 400.0)        // Screen lower-right
        );
        
        let screen_centroid = coord.find_screen_centroid();
        assert_eq!(screen_centroid, (200.0, 300.0)); // Middle of screen area
    }
    
    #[test]
    fn test_coordinate_conversion() {
        let coord = LayerCoord::new(
            100.0, 100.0,         // 100x100 gerber
            (50.0, 50.0),         // Centered at (50,50)
            (0.0, 0.0),           // Screen from (0,0)  
            (200.0, 200.0)        // to (200,200)
        );
        
        // Test gerber center -> positioned center
        let positioned_pos = coord.gerber_to_positioned(50.0, 50.0);
        assert_eq!(positioned_pos, (100.0, 100.0));
        
        // Test round-trip conversion
        let gerber_pos = coord.positioned_to_gerber(100.0, 100.0);
        assert!((gerber_pos.0 - 50.0).abs() < 0.01);
        assert!((gerber_pos.1 - 50.0).abs() < 0.01);
    }
    
    #[test]
    fn test_positioned_bounds_checking() {
        let coord = LayerCoord::new(
            50.0, 50.0,
            (25.0, 25.0),
            (10.0, 10.0),
            (60.0, 60.0)
        );
        
        assert!(coord.contains_positioned_point(30.0, 30.0)); // Inside
        assert!(!coord.contains_positioned_point(5.0, 30.0)); // Outside left
        assert!(!coord.contains_positioned_point(70.0, 30.0)); // Outside right
    }
}