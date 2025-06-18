use bevy_ecs::prelude::*;
use nalgebra::Point2;
use std::path::Path;
use std::collections::HashMap;
use crate::ecs::{Position3D, Transform3D, GerberLayerComponent, PcbElement, MaterialProperties};
use crate::layer_operations::LayerType;

/// KiCad component information extracted from .kicad_pcb files
#[derive(Component, Debug, Clone)]
pub struct KiCadComponent {
    /// Reference designator (U1, R5, C3, etc.)
    pub reference: String,
    /// Component value (if available)
    pub value: Option<String>,
    /// Footprint library and name
    pub footprint: String,
    /// Layer assignment
    pub layer: KiCadLayer,
    /// Component properties/attributes
    pub properties: HashMap<String, String>,
    /// Rotation in degrees
    pub rotation: f64,
    /// Component bounding box
    pub bbox: Option<(Point2<f64>, Point2<f64>)>,
}

impl KiCadComponent {
    pub fn new(reference: String, footprint: String, layer: KiCadLayer) -> Self {
        Self {
            reference,
            value: None,
            footprint,
            layer,
            properties: HashMap::new(),
            rotation: 0.0,
            bbox: None,
        }
    }

    /// Check if this is a SMD component
    pub fn is_smd(&self) -> bool {
        self.layer == KiCadLayer::Front || self.layer == KiCadLayer::Back
    }

    /// Check if this is a through-hole component
    pub fn is_through_hole(&self) -> bool {
        self.layer == KiCadLayer::Both
    }

    /// Get component category based on reference prefix
    pub fn category(&self) -> ComponentCategory {
        let prefix = self.reference.chars().take_while(|c| c.is_alphabetic()).collect::<String>();
        match prefix.to_uppercase().as_str() {
            "R" => ComponentCategory::Resistor,
            "C" => ComponentCategory::Capacitor,
            "L" => ComponentCategory::Inductor,
            "U" | "IC" => ComponentCategory::IntegratedCircuit,
            "Q" => ComponentCategory::Transistor,
            "D" => ComponentCategory::Diode,
            "LED" => ComponentCategory::LED,
            "J" | "P" => ComponentCategory::Connector,
            "SW" => ComponentCategory::Switch,
            "X" | "Y" => ComponentCategory::Crystal,
            "F" => ComponentCategory::Fuse,
            "T" => ComponentCategory::Transformer,
            _ => ComponentCategory::Other,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum KiCadLayer {
    Front,
    Back,
    Both, // Through-hole components
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum ComponentCategory {
    Resistor,
    Capacitor,
    Inductor,
    IntegratedCircuit,
    Transistor,
    Diode,
    LED,
    Connector,
    Switch,
    Crystal,
    Fuse,
    Transformer,
    Other,
}

/// KiCad trace information
#[derive(Component, Debug, Clone)]
pub struct KiCadTrace {
    pub width: f64,
    pub layer: String,
    pub net_name: Option<String>,
    pub length: f64,
}

/// KiCad via information
#[derive(Component, Debug, Clone)]
pub struct KiCadVia {
    pub drill_diameter: f64,
    pub via_diameter: f64,
    pub layer_start: String,
    pub layer_end: String,
    pub net_name: Option<String>,
}

/// KiCad drill/hole information
#[derive(Component, Debug, Clone)]
pub struct KiCadDrill {
    pub diameter: f64,
    pub plated: bool,
    pub drill_type: DrillType,
}

#[derive(Debug, Clone, PartialEq)]
pub enum DrillType {
    ComponentHole,
    Via,
    MountingHole,
    Other,
}

/// PCB board outline and physical properties
#[derive(Debug, Clone)]
pub struct PcbOutline {
    pub vertices: Vec<Point2<f64>>,
    pub thickness: f64,
    pub layer_count: u32,
    pub copper_layers: Vec<String>,
}

/// KiParse integration for extracting component data from .kicad_pcb files
pub struct KiParseExtractor {
    /// Component statistics
    pub component_count: usize,
    pub trace_count: usize,
    pub via_count: usize,
    pub drill_count: usize,
}

impl KiParseExtractor {
    pub fn new() -> Self {
        Self {
            component_count: 0,
            trace_count: 0,
            via_count: 0,
            drill_count: 0,
        }
    }

    /// Extract all component data from a .kicad_pcb file
    pub fn extract_from_file<P: AsRef<Path>>(
        &mut self,
        pcb_file: P,
    ) -> Result<ExtractedPcbData, KiParseError> {
        let content = std::fs::read_to_string(pcb_file.as_ref())
            .map_err(|e| KiParseError::FileError(e.to_string()))?;

        self.extract_from_content(&content)
    }

    /// Extract data from PCB file content
    pub fn extract_from_content(&mut self, content: &str) -> Result<ExtractedPcbData, KiParseError> {
        // For now, implement a simple parser
        // In a real implementation, we'd use the kiparse crate properly
        let mut data = ExtractedPcbData::new();

        // Reset counters
        self.component_count = 0;
        self.trace_count = 0;
        self.via_count = 0;
        self.drill_count = 0;

        // Parse components (footprints in KiCad terminology)
        self.parse_components(content, &mut data)?;

        // Parse traces (track segments)
        self.parse_traces(content, &mut data)?;

        // Parse vias
        self.parse_vias(content, &mut data)?;

        // Parse board outline
        self.parse_board_outline(content, &mut data)?;

        Ok(data)
    }

    /// Parse component/footprint information
    fn parse_components(&mut self, content: &str, data: &mut ExtractedPcbData) -> Result<(), KiParseError> {
        // Simple regex-based parsing for demonstration
        // In production, use proper S-expression parsing from kiparse
        
        for line in content.lines() {
            let trimmed = line.trim();
            if trimmed.starts_with("(footprint ") {
                if let Some(component) = self.parse_footprint_line(trimmed)? {
                    data.components.push(component);
                    self.component_count += 1;
                }
            }
        }

        Ok(())
    }

    /// Parse a single footprint line (simplified)
    fn parse_footprint_line(&self, line: &str) -> Result<Option<KiCadComponent>, KiParseError> {
        // This is a simplified parser - real implementation would use proper S-expression parsing
        if line.contains("fp_text reference") {
            // Extract reference designator
            // Example: (fp_text reference "U1" ...
            if let Some(start) = line.find('"') {
                if let Some(end) = line[start + 1..].find('"') {
                    let reference = line[start + 1..start + 1 + end].to_string();
                    let component = KiCadComponent::new(
                        reference,
                        "Unknown".to_string(), // Would extract from footprint declaration
                        KiCadLayer::Front, // Would parse actual layer
                    );
                    return Ok(Some(component));
                }
            }
        }
        Ok(None)
    }

    /// Parse trace/track information
    fn parse_traces(&mut self, content: &str, data: &mut ExtractedPcbData) -> Result<(), KiParseError> {
        for line in content.lines() {
            let trimmed = line.trim();
            if trimmed.starts_with("(segment ") {
                if let Some(trace) = self.parse_segment_line(trimmed)? {
                    data.traces.push(trace);
                    self.trace_count += 1;
                }
            }
        }
        Ok(())
    }

    /// Parse a single track segment
    fn parse_segment_line(&self, _line: &str) -> Result<Option<KiCadTrace>, KiParseError> {
        // Simplified implementation
        let trace = KiCadTrace {
            width: 0.2, // Would parse actual width
            layer: "F.Cu".to_string(), // Would parse actual layer
            net_name: None, // Would parse net information
            length: 1.0, // Would calculate from start/end points
        };
        Ok(Some(trace))
    }

    /// Parse via information
    fn parse_vias(&mut self, content: &str, data: &mut ExtractedPcbData) -> Result<(), KiParseError> {
        for line in content.lines() {
            let trimmed = line.trim();
            if trimmed.starts_with("(via ") {
                if let Some(via) = self.parse_via_line(trimmed)? {
                    data.vias.push(via);
                    self.via_count += 1;
                }
            }
        }
        Ok(())
    }

    /// Parse a single via
    fn parse_via_line(&self, _line: &str) -> Result<Option<KiCadVia>, KiParseError> {
        // Simplified implementation
        let via = KiCadVia {
            drill_diameter: 0.2,
            via_diameter: 0.4,
            layer_start: "F.Cu".to_string(),
            layer_end: "B.Cu".to_string(),
            net_name: None,
        };
        Ok(Some(via))
    }

    /// Parse board outline
    fn parse_board_outline(&mut self, _content: &str, data: &mut ExtractedPcbData) -> Result<(), KiParseError> {
        // Simplified - would parse actual outline from Edge.Cuts layer
        data.outline = Some(PcbOutline {
            vertices: vec![
                Point2::new(0.0, 0.0),
                Point2::new(100.0, 0.0),
                Point2::new(100.0, 80.0),
                Point2::new(0.0, 80.0),
            ],
            thickness: 1.6,
            layer_count: 2,
            copper_layers: vec!["F.Cu".to_string(), "B.Cu".to_string()],
        });
        Ok(())
    }

    /// Convert extracted data to ECS entities
    pub fn convert_to_ecs_entities(
        &self,
        data: &ExtractedPcbData,
        world: &mut World,
    ) -> Result<ConversionStats, KiParseError> {
        let mut stats = ConversionStats::default();

        // Convert components to ECS entities
        for component in &data.components {
            let entity = world.spawn((
                component.clone(),
                Position3D::new(0.0, 0.0, 0.0), // Would use actual position from PCB
                Transform3D::default(),
                PcbElement::Component {
                    name: component.reference.clone(),
                    footprint: component.footprint.clone(),
                },
                MaterialProperties::copper(), // Would vary by component type
            )).id();

            stats.components_converted += 1;
            log::info!("Created ECS entity for component {} (entity: {:?})", component.reference, entity);
        }

        // Convert traces to ECS entities
        for trace in &data.traces {
            world.spawn((
                trace.clone(),
                Position3D::new(0.0, 0.0, 0.0),
                Transform3D::default(),
                PcbElement::Trace {
                    width: trace.width,
                    start: Point2::new(0.0, 0.0), // Would use actual coordinates
                    end: Point2::new(1.0, 0.0),
                },
            ));
            stats.traces_converted += 1;
        }

        // Convert vias to ECS entities
        for via in &data.vias {
            world.spawn((
                via.clone(),
                Position3D::new(0.0, 0.0, 0.0),
                Transform3D::default(),
                PcbElement::Via {
                    radius: via.via_diameter / 2.0,
                    drill_radius: via.drill_diameter / 2.0,
                },
            ));
            stats.vias_converted += 1;
        }

        Ok(stats)
    }

    /// Get extraction statistics
    pub fn get_stats(&self) -> ExtractionStats {
        ExtractionStats {
            components: self.component_count,
            traces: self.trace_count,
            vias: self.via_count,
            drills: self.drill_count,
        }
    }
}

/// Extracted PCB data from KiCad file
#[derive(Debug, Clone)]
pub struct ExtractedPcbData {
    pub components: Vec<KiCadComponent>,
    pub traces: Vec<KiCadTrace>,
    pub vias: Vec<KiCadVia>,
    pub drills: Vec<KiCadDrill>,
    pub outline: Option<PcbOutline>,
}

impl ExtractedPcbData {
    pub fn new() -> Self {
        Self {
            components: Vec::new(),
            traces: Vec::new(),
            vias: Vec::new(),
            drills: Vec::new(),
            outline: None,
        }
    }

    /// Get summary statistics
    pub fn summary(&self) -> PcbDataSummary {
        let component_categories: HashMap<ComponentCategory, usize> = 
            self.components.iter()
                .map(|c| c.category())
                .fold(HashMap::new(), |mut acc, cat| {
                    *acc.entry(cat).or_insert(0) += 1;
                    acc
                });

        PcbDataSummary {
            total_components: self.components.len(),
            component_categories,
            total_traces: self.traces.len(),
            total_vias: self.vias.len(),
            total_drills: self.drills.len(),
            has_outline: self.outline.is_some(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct PcbDataSummary {
    pub total_components: usize,
    pub component_categories: HashMap<ComponentCategory, usize>,
    pub total_traces: usize,
    pub total_vias: usize,
    pub total_drills: usize,
    pub has_outline: bool,
}

#[derive(Debug, Default)]
pub struct ExtractionStats {
    pub components: usize,
    pub traces: usize,
    pub vias: usize,
    pub drills: usize,
}

#[derive(Debug, Default)]
pub struct ConversionStats {
    pub components_converted: usize,
    pub traces_converted: usize,
    pub vias_converted: usize,
    pub drills_converted: usize,
}

#[derive(Debug, Clone)]
pub enum KiParseError {
    FileError(String),
    ParseError(String),
    InvalidFormat(String),
    ConversionError(String),
}

impl std::fmt::Display for KiParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            KiParseError::FileError(msg) => write!(f, "File error: {}", msg),
            KiParseError::ParseError(msg) => write!(f, "Parse error: {}", msg),
            KiParseError::InvalidFormat(msg) => write!(f, "Invalid format: {}", msg),
            KiParseError::ConversionError(msg) => write!(f, "Conversion error: {}", msg),
        }
    }
}

impl std::error::Error for KiParseError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_component_category() {
        let resistor = KiCadComponent::new("R1".to_string(), "0805".to_string(), KiCadLayer::Front);
        assert_eq!(resistor.category(), ComponentCategory::Resistor);

        let ic = KiCadComponent::new("U1".to_string(), "SOIC-8".to_string(), KiCadLayer::Front);
        assert_eq!(ic.category(), ComponentCategory::IntegratedCircuit);
    }

    #[test]
    fn test_component_type_detection() {
        let smd = KiCadComponent::new("R1".to_string(), "0805".to_string(), KiCadLayer::Front);
        assert!(smd.is_smd());
        assert!(!smd.is_through_hole());

        let through_hole = KiCadComponent::new("J1".to_string(), "Pin_Header".to_string(), KiCadLayer::Both);
        assert!(!through_hole.is_smd());
        assert!(through_hole.is_through_hole());
    }

    #[test]
    fn test_extracted_data_summary() {
        let mut data = ExtractedPcbData::new();
        data.components.push(KiCadComponent::new("R1".to_string(), "0805".to_string(), KiCadLayer::Front));
        data.components.push(KiCadComponent::new("C1".to_string(), "0603".to_string(), KiCadLayer::Front));

        let summary = data.summary();
        assert_eq!(summary.total_components, 2);
        assert_eq!(summary.component_categories.get(&ComponentCategory::Resistor), Some(&1));
        assert_eq!(summary.component_categories.get(&ComponentCategory::Capacitor), Some(&1));
    }
}