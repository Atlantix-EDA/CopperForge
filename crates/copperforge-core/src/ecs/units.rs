use bevy_ecs::prelude::*;

/// Base unit is nanometer (1e-9 meters) stored as u32
/// This provides ~4.29 meters of range with nanometer precision
/// Similar to KiCad's internal unit system
pub type Nanometer = u32;

/// Extended precision for calculations that might overflow u32
pub type NanometerExtended = u64;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum DisplayUnit {
    Millimeters,
    Mils,
    Micrometers,
    Nanometers,
}

impl DisplayUnit {
    pub fn is_mils(&self) -> bool {
        matches!(self, DisplayUnit::Mils)
    }
    
    pub fn is_mm(&self) -> bool {
        matches!(self, DisplayUnit::Millimeters)
    }
}

#[derive(Resource, Debug, Clone)]
pub struct UnitsResource {
    pub display_unit: DisplayUnit,
}

impl Default for UnitsResource {
    fn default() -> Self {
        Self {
            display_unit: DisplayUnit::Millimeters,
        }
    }
}

impl UnitsResource {
    pub fn new(display_unit: DisplayUnit) -> Self {
        Self { display_unit }
    }
    
    pub fn toggle(&mut self) {
        self.display_unit = match self.display_unit {
            DisplayUnit::Millimeters => DisplayUnit::Mils,
            DisplayUnit::Mils => DisplayUnit::Millimeters,
            _ => DisplayUnit::Millimeters,
        };
    }
    
    pub fn set_mils(&mut self) {
        self.display_unit = DisplayUnit::Mils;
    }
    
    pub fn set_mm(&mut self) {
        self.display_unit = DisplayUnit::Millimeters;
    }
    
    pub fn is_mils(&self) -> bool {
        self.display_unit.is_mils()
    }
    
    pub fn is_mm(&self) -> bool {
        self.display_unit.is_mm()
    }
    
    /// Convert from nanometers to display units
    pub fn to_display(&self, nm_value: Nanometer) -> f64 {
        match self.display_unit {
            DisplayUnit::Nanometers => nm_value as f64,
            DisplayUnit::Micrometers => nm_value as f64 / NM_PER_UM,
            DisplayUnit::Millimeters => nm_value as f64 / NM_PER_MM,
            DisplayUnit::Mils => nm_value as f64 / NM_PER_MIL,
        }
    }
    
    /// Convert from display units to nanometers
    pub fn from_display(&self, display_value: f64) -> Nanometer {
        let nm_value = match self.display_unit {
            DisplayUnit::Nanometers => display_value,
            DisplayUnit::Micrometers => display_value * NM_PER_UM,
            DisplayUnit::Millimeters => display_value * NM_PER_MM,
            DisplayUnit::Mils => display_value * NM_PER_MIL,
        };
        nm_value.round() as Nanometer
    }
    
    pub fn format_value(&self, nm_value: Nanometer) -> String {
        let display_value = self.to_display(nm_value);
        match self.display_unit {
            DisplayUnit::Nanometers => format!("{:.0} nm", display_value),
            DisplayUnit::Micrometers => format!("{:.3} µm", display_value),
            DisplayUnit::Millimeters => format!("{:.3} mm", display_value),
            DisplayUnit::Mils => format!("{:.1} mils", display_value),
        }
    }
    
    pub fn format_value_with_precision(&self, nm_value: Nanometer, precision: usize) -> String {
        let display_value = self.to_display(nm_value);
        match self.display_unit {
            DisplayUnit::Nanometers => format!("{:.prec$} nm", display_value, prec = precision),
            DisplayUnit::Micrometers => format!("{:.prec$} µm", display_value, prec = precision),
            DisplayUnit::Millimeters => format!("{:.prec$} mm", display_value, prec = precision),
            DisplayUnit::Mils => format!("{:.prec$} mils", display_value, prec = precision),
        }
    }
    
    pub fn unit_suffix(&self) -> &'static str {
        match self.display_unit {
            DisplayUnit::Nanometers => "nm",
            DisplayUnit::Micrometers => "µm",
            DisplayUnit::Millimeters => "mm",
            DisplayUnit::Mils => "mils",
        }
    }
}

// Conversion constants
pub const NM_PER_MM: f64 = 1_000_000.0;     // 1 mm = 1,000,000 nm
pub const NM_PER_UM: f64 = 1_000.0;         // 1 µm = 1,000 nm
pub const NM_PER_MIL: f64 = 25_400.0;       // 1 mil = 0.0254 mm = 25,400 nm
pub const NM_PER_INCH: f64 = 25_400_000.0;  // 1 inch = 25.4 mm = 25,400,000 nm
pub const NM_PER_METER: f64 = 1_000_000_000.0; // 1 m = 1,000,000,000 nm

// Conversion functions for legacy f32 values
pub fn mm_to_nm(mm: f32) -> Nanometer {
    (mm as f64 * NM_PER_MM).round() as Nanometer
}

pub fn nm_to_mm(nm: Nanometer) -> f32 {
    (nm as f64 / NM_PER_MM) as f32
}

pub fn mils_to_nm(mils: f32) -> Nanometer {
    (mils as f64 * NM_PER_MIL).round() as Nanometer
}

pub fn nm_to_mils(nm: Nanometer) -> f32 {
    (nm as f64 / NM_PER_MIL) as f32
}

// Legacy compatibility functions (for gradual migration)
pub fn mm_to_mils(mm: f32) -> f32 {
    nm_to_mils(mm_to_nm(mm))
}

pub fn mils_to_mm(mils: f32) -> f32 {
    nm_to_mm(mils_to_nm(mils))
}

// Precision-aware conversion for coordinates
#[derive(Debug, Clone, Copy)]
pub struct Coordinate {
    pub x: Nanometer,
    pub y: Nanometer,
}

impl Coordinate {
    pub fn new(x: Nanometer, y: Nanometer) -> Self {
        Self { x, y }
    }
    
    pub fn from_mm(x_mm: f32, y_mm: f32) -> Self {
        Self {
            x: mm_to_nm(x_mm),
            y: mm_to_nm(y_mm),
        }
    }
    
    pub fn from_mils(x_mils: f32, y_mils: f32) -> Self {
        Self {
            x: mils_to_nm(x_mils),
            y: mils_to_nm(y_mils),
        }
    }
    
    pub fn to_mm(&self) -> (f32, f32) {
        (nm_to_mm(self.x), nm_to_mm(self.y))
    }
    
    pub fn to_mils(&self) -> (f32, f32) {
        (nm_to_mils(self.x), nm_to_mils(self.y))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_unit_conversions() {
        // Test mm to nm and back
        let mm_value = 25.4;
        let nm_value = mm_to_nm(mm_value);
        assert_eq!(nm_value, 25_400_000);
        let converted_back = nm_to_mm(nm_value);
        assert!((converted_back - mm_value).abs() < 0.001);
        
        // Test mil to nm and back
        let mil_value = 1000.0;
        let nm_value = mils_to_nm(mil_value);
        assert_eq!(nm_value, 25_400_000);
        let converted_back = nm_to_mils(nm_value);
        assert!((converted_back - mil_value).abs() < 0.001);
    }
    
    #[test]
    fn test_coordinate_conversions() {
        let coord = Coordinate::from_mm(10.0, 20.0);
        assert_eq!(coord.x, 10_000_000);
        assert_eq!(coord.y, 20_000_000);
        
        let (x_mm, y_mm) = coord.to_mm();
        assert!((x_mm - 10.0).abs() < 0.001);
        assert!((y_mm - 20.0).abs() < 0.001);
    }
    
    #[test]
    fn test_precision_limits() {
        // Test maximum range with u32
        let max_nm: Nanometer = u32::MAX;
        let max_mm = nm_to_mm(max_nm);
        assert!(max_mm > 4290.0); // ~4.29 meters
        assert!(max_mm < 4300.0);
    }
}