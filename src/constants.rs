use gerber_viewer::position::Vector;

pub const ENABLE_UNIQUE_SHAPE_COLORS: bool = false;
pub const ENABLE_POLYGON_NUMBERING: bool = false;
pub const MIRRORING: [bool; 2] = [false, false];

// for mirroring and rotation
pub const CENTER_OFFSET: Vector = Vector::new(0.0, 0.0);

// in EDA tools like DipTrace, a gerber offset can be specified when exporting gerbers, e.g. 10,5.
// use negative offsets here to relocate the gerber back to 0,0, e.g. -10, -5
pub const DESIGN_OFFSET: Vector = Vector::new(0.0, 0.0);

// radius of the markers, in gerber coordinates
pub const MARKER_RADIUS: f32 = 2.5;

// Custom log types for different event categories
pub const LOG_TYPE_ROTATION: &str = "rotation";
pub const LOG_TYPE_CENTER_OFFSET: &str = "center_offset";
pub const LOG_TYPE_DESIGN_OFFSET: &str = "design_offset";
pub const LOG_TYPE_MIRROR: &str = "mirror";
pub const LOG_TYPE_DRC: &str = "drc";
pub const LOG_TYPE_GRID: &str = "grid";