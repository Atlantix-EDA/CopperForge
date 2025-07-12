pub mod manager;
pub mod grid;

// Re-export the main types for easy access
pub use manager::{DisplayManager, VectorOffset};
pub use grid::{GridSettings, draw_grid, snap_to_grid, align_to_grid};