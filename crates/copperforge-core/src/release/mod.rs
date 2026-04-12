//! Release management — tag and track gerber fabrication releases.
//!
//! Each release is a snapshot of gerber/drill files for a specific
//! fabrication run, e.g. "pcbway_01June2025_release".
//!
//! Future: ReleaseManager will handle creating tagged releases,
//! archiving gerber sets, and tracking fabrication history.

use std::path::PathBuf;

/// A tagged fabrication release.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Release {
    /// Human-readable tag, e.g. "pcbway_01June2025_release"
    pub tag: String,
    /// When the release was created
    pub created_at: chrono::DateTime<chrono::Utc>,
    /// Path to the archived gerber/drill package
    pub archive_path: PathBuf,
    /// Which vendor this release targets (if any)
    pub vendor: Option<String>,
    /// Notes about this release
    pub notes: String,
}

/// Manages fabrication releases for a project.
#[derive(Default)]
pub struct ReleaseManager {
    pub releases: Vec<Release>,
}

impl ReleaseManager {
    pub fn new() -> Self { Self::default() }

    // TODO: create_release(tag, gerber_dir, vendor) -> Result<Release>
    // TODO: list_releases() -> &[Release]
    // TODO: load_from_project(project_path) -> Self
    // TODO: save_to_project(project_path) -> Result<()>
}
