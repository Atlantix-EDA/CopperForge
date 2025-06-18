pub mod mesh3d;
pub mod gerber_extrudable;
pub mod wgpu_renderer;
pub mod pcb_viewer;

// Re-export the main types for easy access
pub use mesh3d::{Mesh3D, Polygon2D, ExtrusionEngine, ExtrusionError};
pub use gerber_extrudable::layer_to_3d_meshes;
pub use pcb_viewer::PcbViewer;