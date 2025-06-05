// Display-related constants have been moved to DisplayManager defaults

// radius of the markers, in gerber coordinates
pub const MARKER_RADIUS: f32 = 2.5;

// Custom log types for different event categories
pub const LOG_TYPE_ROTATION: &str = "rotation";
pub const LOG_TYPE_CENTER_OFFSET: &str = "center_offset";
pub const LOG_TYPE_DESIGN_OFFSET: &str = "design_offset";
pub const LOG_TYPE_MIRROR: &str = "mirror";
pub const LOG_TYPE_DRC: &str = "drc";
pub const LOG_TYPE_GRID: &str = "grid";