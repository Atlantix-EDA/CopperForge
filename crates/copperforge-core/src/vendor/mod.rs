//! Vendor integration — package gerber/drill files for specific PCB fabs.
//!
//! Replicates what the KiCad plugins for PCBWay and Sierra Proto Express
//! do: zip up the right combination of gerber + drill files with the
//! naming conventions each vendor expects.

use std::path::PathBuf;

/// Supported PCB fabrication vendors.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum VendorKind {
    PcbWay,
    SierraProtoExpress,
    Jlcpcb,
    Oshpark,
    Custom,
}

impl VendorKind {
    pub fn display_name(&self) -> &str {
        match self {
            Self::PcbWay => "PCBWay",
            Self::SierraProtoExpress => "Sierra Proto Express",
            Self::Jlcpcb => "JLCPCB",
            Self::Oshpark => "OSH Park",
            Self::Custom => "Custom",
        }
    }

    pub fn all() -> &'static [Self] {
        &[Self::PcbWay, Self::SierraProtoExpress, Self::Jlcpcb, Self::Oshpark]
    }
}

/// Configuration for packaging gerbers for a specific vendor.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct VendorPackageConfig {
    pub vendor: VendorKind,
    /// File naming pattern overrides (vendor-specific)
    pub naming_convention: Option<String>,
    /// Whether to include drill files
    pub include_drills: bool,
    /// Whether to include pick-and-place data
    pub include_pnp: bool,
}

/// Packages gerber/drill files for vendors.
#[derive(Default)]
pub struct VendorPackager;

impl VendorPackager {
    pub fn new() -> Self { Self }

    // TODO: package(gerber_dir, vendor, output_path) -> Result<PathBuf>
    // TODO: validate_for_vendor(gerber_dir, vendor) -> Vec<ValidationIssue>
    // TODO: get_required_files(vendor) -> Vec<RequiredFile>
}
