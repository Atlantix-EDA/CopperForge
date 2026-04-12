//! Display unit types and conversion constants.

/// Display unit for coordinates.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum DisplayUnit {
    Millimeters,
    Mils,
    Micrometers,
    Nanometers,
}

// Conversion constants (base unit = nanometer)
pub const NM_PER_MM: f64 = 1_000_000.0;
pub const NM_PER_MIL: f64 = 25_400.0;
pub const NM_PER_UM: f64 = 1_000.0;

/// Convert millimeters to mils.
pub fn mm_to_mils(mm: f64) -> f64 { mm * 1000.0 / 25.4 }

/// Convert mils to millimeters.
pub fn mils_to_mm(mils: f64) -> f64 { mils * 25.4 / 1000.0 }

// Nanometer conversion functions (base unit for KiCad-style precision)
pub type Nanometer = u32;
pub type NanometerExtended = u64;

pub fn mm_to_nm(mm: f64) -> u32 { (mm * NM_PER_MM) as u32 }
pub fn nm_to_mm(nm: u32) -> f64 { nm as f64 / NM_PER_MM }
pub fn mils_to_nm(mils: f64) -> u32 { (mils * NM_PER_MIL) as u32 }
pub fn nm_to_mils(nm: u32) -> f64 { nm as f64 / NM_PER_MIL }
