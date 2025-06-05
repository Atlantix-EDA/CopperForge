pub mod manager;
pub mod grid;

// Re-export the main types for easy access
pub use manager::{DisplayManager, MirroringSettings, VectorOffset};
pub use grid::{GridSettings, GridStatus, draw_grid, get_grid_status};