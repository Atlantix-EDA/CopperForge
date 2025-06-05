pub mod types;
pub mod detection;
pub mod manager;

// Re-export the main types for easy access
pub use types::{LayerType, LayerInfo};
pub use detection::{LayerDetector, UnassignedGerber};
pub use manager::{LayerManager, LayerStatistics};