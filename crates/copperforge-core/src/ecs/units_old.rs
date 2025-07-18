use bevy_ecs::prelude::*;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum DisplayUnit {
    Millimeters,
    Mils,
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
    
    pub fn to_display(&self, mm_value: f32) -> f32 {
        match self.display_unit {
            DisplayUnit::Millimeters => mm_value,
            DisplayUnit::Mils => mm_to_mils(mm_value),
        }
    }
    
    pub fn from_display(&self, display_value: f32) -> f32 {
        match self.display_unit {
            DisplayUnit::Millimeters => display_value,
            DisplayUnit::Mils => mils_to_mm(display_value),
        }
    }
    
    pub fn format_value(&self, mm_value: f32) -> String {
        let display_value = self.to_display(mm_value);
        match self.display_unit {
            DisplayUnit::Millimeters => format!("{:.2} mm", display_value),
            DisplayUnit::Mils => format!("{:.1} mils", display_value),
        }
    }
    
    pub fn format_value_with_precision(&self, mm_value: f32, precision: usize) -> String {
        let display_value = self.to_display(mm_value);
        match self.display_unit {
            DisplayUnit::Millimeters => format!("{:.prec$} mm", display_value, prec = precision),
            DisplayUnit::Mils => format!("{:.prec$} mils", display_value, prec = precision),
        }
    }
    
    pub fn unit_suffix(&self) -> &'static str {
        match self.display_unit {
            DisplayUnit::Millimeters => "mm",
            DisplayUnit::Mils => "mils",
        }
    }
}

pub const MM_TO_MILS: f32 = 39.3701;
pub const MILS_TO_MM: f32 = 0.0254;

pub fn mm_to_mils(mm: f32) -> f32 {
    mm * MM_TO_MILS
}

pub fn mils_to_mm(mils: f32) -> f32 {
    mils * MILS_TO_MM
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_unit_conversions() {
        let mm_value = 25.4;
        let mils_value = mm_to_mils(mm_value);
        assert!((mils_value - 1000.0).abs() < 0.1);
        
        let converted_back = mils_to_mm(mils_value);
        assert!((converted_back - mm_value).abs() < 0.001);
    }
    
    #[test]
    fn test_units_resource() {
        let mut units = UnitsResource::default();
        assert!(units.is_mm());
        assert!(!units.is_mils());
        
        units.toggle();
        assert!(units.is_mils());
        assert!(!units.is_mm());
        
        let mm_value = 10.0;
        let display_value = units.to_display(mm_value);
        assert!((display_value - 393.701).abs() < 0.01);
        
        let formatted = units.format_value(mm_value);
        assert!(formatted.contains("mils"));
    }
}