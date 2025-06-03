use gerber_viewer::{GerberLayer, GerberPrimitive, BoundingBox};
use gerber_viewer::position::{Position, Vector};
use std::collections::HashMap;
use std::io::BufReader;
use gerber_viewer::gerber_parser::parse;

#[derive(Debug, Clone)]
pub struct DrcSimple {
    pub min_trace_width: f32,   // mm
    pub min_via_diameter: f32,  // mm
    pub min_hole_diameter: f32, // mm
    pub min_spacing: f32,       // mm
    pub lines_only: bool,       // Only analyze Line primitives (skip rectangles)
    pub min_trace_length: f32,  // mm - minimum length to be considered a trace
}

impl Default for DrcSimple {
    fn default() -> Self {
        Self {
            min_trace_width: 0.15,   // 6 mil
            min_via_diameter: 0.3,   // 12 mil
            min_hole_diameter: 0.2,  // 8 mil
            min_spacing: 0.15,       // 6 mil
            lines_only: false,       // By default, analyze both lines and rectangles
            min_trace_length: 2.0,   // 2mm - filter out short pad/via connections
        }
    }
}

#[derive(Debug, Clone)]
pub struct Trace {
    pub width: f32,
    pub length: f32,
    pub center_x: f32,
    pub center_y: f32,
    pub trace_type: TraceType,
}

#[derive(Debug, Clone)]
pub enum TraceType {
    Line,      // Line primitive
    Rectangle, // Rectangular primitive with high aspect ratio
}

#[derive(Debug, Clone)]
pub struct TraceViolation {
    pub trace: Trace,
    pub measured_width: f32,
    pub required_width: f32,
    pub violation_type: String,
}

#[derive(Debug, Clone)]
pub struct TraceQualityIssue {
    pub issue_type: TraceQualityType,
    pub location: (f32, f32),
    pub severity: f32,
    pub description: String,
}

#[derive(Debug, Clone)]
pub enum TraceQualityType {
    UnnecessaryJog,      // Sharp turns that could be simplified
    IneffientRouting,    // Longer path than necessary  
    SharpCorner,         // 90° corners that could be rounded
    Stairstepping,       // Multiple small segments instead of diagonal
}

#[derive(Debug, Clone)]
pub struct CornerOverlayShape {
    pub points: Vec<Position>,  // Polygon points forming the filled corner
    pub trace_width: f32,
}

/// DRC Rules structure with unit conversion support
#[derive(Debug, Clone)]
pub struct DrcRules {
    pub min_trace_width: f32,      // mm
    pub min_via_diameter: f32,     // mm  
    pub min_drill_diameter: f32,   // mm
    pub min_spacing: f32,          // mm
    pub min_annular_ring: f32,     // mm
    pub use_mils: bool,            // true = display in mils, false = mm
}

impl Default for DrcRules {
    fn default() -> Self {
        Self {
            min_trace_width: 0.15,    // 0.15mm = ~6 mil
            min_via_diameter: 0.3,    // 0.3mm = ~12 mil
            min_drill_diameter: 0.2,  // 0.2mm = ~8 mil
            min_spacing: 0.15,        // 0.15mm = ~6 mil
            min_annular_ring: 0.1,    // 0.1mm = ~4 mil
            use_mils: false,          // Default to mm
        }
    }
}

impl DrcRules {
    /// Convert mm to mils (1 mm = 39.3701 mils)
    pub fn mm_to_mils(mm: f32) -> f32 {
        mm * 39.3701
    }
    
    /// Convert mils to mm (1 mil = 0.0254 mm)
    pub fn mils_to_mm(mils: f32) -> f32 {
        mils * 0.0254
    }
    
    /// Get display value (convert to mils if use_mils is true)
    pub fn get_display_value(&self, mm_value: f32) -> f32 {
        if self.use_mils {
            Self::mm_to_mils(mm_value)
        } else {
            mm_value
        }
    }
    
    /// Set value from display (convert from mils if use_mils is true)
    pub fn set_from_display(&self, display_value: f32) -> f32 {
        if self.use_mils {
            Self::mils_to_mm(display_value)
        } else {
            display_value
        }
    }
    
    /// Get unit suffix
    pub fn unit_suffix(&self) -> &str {
        if self.use_mils { " mils" } else { " mm" }
    }
}

/// DRC violation result
#[derive(Debug, Clone)]
pub struct DrcViolation {
    pub rule_name: String,
    pub description: String,
    pub layer: String,
    pub measured_value: f32,  // mm
    pub required_value: f32,  // mm
    pub x: f32,              // mm
    pub y: f32,              // mm
}

impl DrcViolation {
    pub fn format_message(&self) -> String {
        format!("{}: {} on {} - measured {:.3}mm, required {:.3}mm at ({:.1}, {:.1})",
            self.rule_name,
            self.description,
            self.layer,
            self.measured_value,
            self.required_value,
            self.x,
            self.y
        )
    }
}

impl DrcSimple {
    pub fn find_traces(&self, layer: &GerberLayer) -> Vec<Trace> {
        let mut traces = Vec::new();
        
        println!("Analyzing {} primitives for traces", layer.primitives().len());
        let mut line_count = 0;
        let mut rect_count = 0;
        let mut rect_trace_count = 0;
        
        for primitive in layer.primitives().iter() {
            match primitive {
                GerberPrimitive::Line { start, end, width, .. } => {
                    line_count += 1;
                    let length = ((end.x - start.x).powi(2) + (end.y - start.y).powi(2)).sqrt() as f32;
                    
                    // Filter by length to avoid pad/via connections
                    if length >= self.min_trace_length {
                        let center_x = ((start.x + end.x) / 2.0) as f32;
                        let center_y = ((start.y + end.y) / 2.0) as f32;
                        
                        traces.push(Trace {
                            width: *width as f32,
                            length,
                            center_x,
                            center_y,
                            trace_type: TraceType::Line,
                        });
                    }
                }
                GerberPrimitive::Rectangle { origin, width, height, .. } => {
                    rect_count += 1;
                    
                    // Skip rectangles if lines_only mode is enabled
                    if self.lines_only {
                        continue;
                    }
                    
                    let w = *width as f32;
                    let h = *height as f32;
                    let aspect_ratio = w.max(h) / w.min(h);
                    let area = w * h;
                    
                    // VERY conservative trace detection to avoid copper pours:
                    // 1. Very high aspect ratio (long and thin)
                    // 2. Very small area (not large pours)
                    // 3. Very narrow dimension
                    if aspect_ratio > 8.0 &&           // Even higher aspect ratio
                       area < 10.0 &&                  // Very small area (10 mm²)
                       w.min(h) < 1.0 &&               // Very narrow dimension < 1mm
                       w.min(h) > 0.05 &&              // But not impossibly small
                       w.max(h) > 2.0 {                // Some reasonable length
                        
                        rect_trace_count += 1;
                        let trace_width = w.min(h);
                        let trace_length = w.max(h);
                        let center_x = (origin.x + (*width / 2.0)) as f32;
                        let center_y = (origin.y + (*height / 2.0)) as f32;
                        
                        traces.push(Trace {
                            width: trace_width,
                            length: trace_length,
                            center_x,
                            center_y,
                            trace_type: TraceType::Rectangle,
                        });
                    }
                }
                // Skip circles and complex polygons for now
                _ => {}
            }
        }
        
        println!("Primitive analysis: {} lines, {} rectangles ({} became traces), {} total traces (min length: {}mm)", 
                 line_count, rect_count, rect_trace_count, traces.len(), self.min_trace_length);
        
        traces
    }
    
    pub fn find_trace_width_violations(&self, traces: &[Trace]) -> Vec<TraceViolation> {
        traces
            .iter()
            .filter(|trace| trace.width < self.min_trace_width)
            .map(|trace| TraceViolation {
                trace: trace.clone(),
                measured_width: trace.width,
                required_width: self.min_trace_width,
                violation_type: "Minimum Trace Width".to_string(),
            })
            .collect()
    }
    
    pub fn run_trace_width_drc(&self, layer: &GerberLayer) -> Vec<TraceViolation> {
        let traces = self.find_traces(layer);
        self.find_trace_width_violations(&traces)
    }
    
    pub fn run_trace_width_drc_with_bounds(&self, layer: &GerberLayer, pcb_bounds: Option<&gerber_viewer::BoundingBox>) -> Vec<TraceViolation> {
        let traces = self.find_traces(layer);
        let violations = self.find_trace_width_violations(&traces);
        
        // Filter violations to only those within PCB bounds
        if let Some(bounds) = pcb_bounds {
            violations.into_iter()
                .filter(|violation| {
                    let x = violation.trace.center_x as f64;
                    let y = violation.trace.center_y as f64;
                    x >= bounds.min.x && x <= bounds.max.x && 
                    y >= bounds.min.y && y <= bounds.max.y
                })
                .collect()
        } else {
            violations
        }
    }
    
    /// Analyze trace quality and detect routing artifacts like unnecessary jogs
    pub fn analyze_trace_quality(&self, layer: &GerberLayer) -> Vec<TraceQualityIssue> {
        let mut quality_issues = Vec::new();
        let primitives = layer.primitives();
        
        println!("DEBUG: Analyzing {} primitives for quality issues", primitives.len());
        
        let mut line_count = 0;
        
        // Look for patterns that indicate poor routing quality
        for (i, primitive) in primitives.iter().enumerate() {
            if let GerberPrimitive::Line { start, end, width, .. } = primitive {
                line_count += 1;
                if line_count <= 5 { // Debug first few lines
                    println!("DEBUG: Line {}: ({:.2}, {:.2}) to ({:.2}, {:.2}), width {:.3}mm", 
                        i, start.x, start.y, end.x, end.y, width);
                }
                
                // Check for unnecessary jogs by examining nearby lines
                if let Some(jog_issue) = self.detect_unnecessary_jog(primitive, primitives, i) {
                    quality_issues.push(jog_issue);
                }
                
                // Check for sharp corners that could be rounded
                if let Some(corner_issue) = self.detect_sharp_corner(primitive, primitives, i) {
                    quality_issues.push(corner_issue);
                }
            }
        }
        
        println!("DEBUG: Found {} lines total, {} quality issues", line_count, quality_issues.len());
        
        quality_issues
    }
    
    /// Detect unnecessary jogs in trace routing (e.g., horizontal→vertical→horizontal when diagonal would work)
    fn detect_unnecessary_jog(&self, current_line: &GerberPrimitive, all_primitives: &[GerberPrimitive], index: usize) -> Option<TraceQualityIssue> {
        if let GerberPrimitive::Line { start: curr_start, end: curr_end, width: curr_width, .. } = current_line {
            // Look for connected lines that form a jog pattern
            let tolerance = 0.001; // 1 micrometer tolerance for connection detection
            
            for (i, other_primitive) in all_primitives.iter().enumerate() {
                if i == index { continue; }
                
                if let GerberPrimitive::Line { start: other_start, end: other_end, width: other_width, .. } = other_primitive {
                    // Check if lines are connected and have similar width
                    let width_diff = (curr_width - other_width).abs();
                    if width_diff > 0.01 { continue; } // Different width traces
                    
                    // Check if this forms a jog pattern (L-shaped routing that could be diagonal)
                    if self.lines_form_jog(curr_start, curr_end, other_start, other_end, tolerance) {
                        let jog_center_x = (curr_start.x + curr_end.x + other_start.x + other_end.x) / 4.0;
                        let jog_center_y = (curr_start.y + curr_end.y + other_start.y + other_end.y) / 4.0;
                        
                        return Some(TraceQualityIssue {
                            issue_type: TraceQualityType::UnnecessaryJog,
                            location: (jog_center_x as f32, jog_center_y as f32),
                            severity: 0.6, // Medium severity
                            description: format!("Unnecessary jog detected - could be simplified to diagonal routing"),
                        });
                    }
                }
            }
        }
        None
    }
    
    /// Check if two connected lines form an unnecessary jog pattern
    fn lines_form_jog(&self, start1: &Position, end1: &Position, 
                      start2: &Position, end2: &Position, tolerance: f64) -> bool {
        // Check if lines are connected (share an endpoint)
        let connected = 
            (start1.x - start2.x).abs() < tolerance && (start1.y - start2.y).abs() < tolerance ||
            (start1.x - end2.x).abs() < tolerance && (start1.y - end2.y).abs() < tolerance ||
            (end1.x - start2.x).abs() < tolerance && (end1.y - start2.y).abs() < tolerance ||
            (end1.x - end2.x).abs() < tolerance && (end1.y - end2.y).abs() < tolerance;
            
        if !connected { return false; }
        
        // Check if one is horizontal and other is vertical (forms L-shape)
        let line1_is_horizontal = (start1.y - end1.y).abs() < tolerance;
        let line1_is_vertical = (start1.x - end1.x).abs() < tolerance;
        let line2_is_horizontal = (start2.y - end2.y).abs() < tolerance;
        let line2_is_vertical = (start2.x - end2.x).abs() < tolerance;
        
        (line1_is_horizontal && line2_is_vertical) || (line1_is_vertical && line2_is_horizontal)
    }
    
    /// Detect sharp 90-degree corners that could benefit from rounding
    fn detect_sharp_corner(&self, current_line: &GerberPrimitive, all_primitives: &[GerberPrimitive], index: usize) -> Option<TraceQualityIssue> {
        if let GerberPrimitive::Line { start: curr_start, end: curr_end, width: curr_width, .. } = current_line {
            let tolerance = 0.01; // Increased tolerance to 10 micrometers
            
            // Look for lines that connect to this one at 90-degree angles
            for (i, other_primitive) in all_primitives.iter().enumerate() {
                if i == index { continue; }
                
                if let GerberPrimitive::Line { start: other_start, end: other_end, width: other_width, .. } = other_primitive {
                    // Check if lines have similar width (same trace)
                    let width_diff = (curr_width - other_width).abs();
                    if width_diff > 0.02 { continue; } // Allow more width variation
                    
                    // Find connection point and check if it forms a 90-degree corner
                    if let Some((corner_pos, angle)) = self.find_corner_angle(curr_start, curr_end, other_start, other_end, tolerance) {
                        // Check if it's close to 90 degrees (within 15 degrees tolerance)
                        let angle_deg = angle.to_degrees().abs();
                        if (angle_deg - 90.0).abs() < 15.0 {
                            println!("DEBUG: Found corner at ({:.2}, {:.2}) with angle {:.1}°", 
                                corner_pos.x, corner_pos.y, angle_deg);
                            
                            // Calculate minimum safe radius for rounding (must be smaller than half the trace width)
                            let max_radius = curr_width.min(*other_width) / 3.0; // Conservative: 1/3 of trace width
                            
                            return Some(TraceQualityIssue {
                                issue_type: TraceQualityType::SharpCorner,
                                location: (corner_pos.x as f32, corner_pos.y as f32),
                                severity: 0.7, // High severity - sharp corners can cause signal integrity issues
                                description: format!("Sharp {:.1}° corner detected - could be rounded with radius up to {:.3}mm", 
                                               angle_deg, max_radius),
                            });
                        }
                    }
                }
            }
        }
        None
    }
    
    /// Find the corner angle between two connected lines
    fn find_corner_angle(&self, start1: &Position, end1: &Position, start2: &Position, end2: &Position, tolerance: f64) -> Option<(Position, f64)> {
        // Find connection point
        let connection_point = if (start1.x - start2.x).abs() < tolerance && (start1.y - start2.y).abs() < tolerance {
            Some(*start1)
        } else if (start1.x - end2.x).abs() < tolerance && (start1.y - end2.y).abs() < tolerance {
            Some(*start1)
        } else if (end1.x - start2.x).abs() < tolerance && (end1.y - start2.y).abs() < tolerance {
            Some(*end1)
        } else if (end1.x - end2.x).abs() < tolerance && (end1.y - end2.y).abs() < tolerance {
            Some(*end1)
        } else {
            None
        };
        
        if let Some(corner) = connection_point {
            // Calculate direction vectors from the corner
            let dir1 = if (corner.x - start1.x).abs() < tolerance && (corner.y - start1.y).abs() < tolerance {
                // Corner is at start1, direction goes toward end1
                Position::new(end1.x - start1.x, end1.y - start1.y)
            } else {
                // Corner is at end1, direction goes toward start1
                Position::new(start1.x - end1.x, start1.y - end1.y)
            };
            
            let dir2 = if (corner.x - start2.x).abs() < tolerance && (corner.y - start2.y).abs() < tolerance {
                // Corner is at start2, direction goes toward end2
                Position::new(end2.x - start2.x, end2.y - start2.y)
            } else {
                // Corner is at end2, direction goes toward start2
                Position::new(start2.x - end2.x, start2.y - end2.y)
            };
            
            // Normalize direction vectors
            let len1 = (dir1.x * dir1.x + dir1.y * dir1.y).sqrt();
            let len2 = (dir2.x * dir2.x + dir2.y * dir2.y).sqrt();
            
            if len1 > tolerance && len2 > tolerance {
                let norm_dir1 = Position::new(dir1.x / len1, dir1.y / len1);
                let norm_dir2 = Position::new(dir2.x / len2, dir2.y / len2);
                
                // Calculate angle between vectors using dot product
                let dot_product = norm_dir1.x * norm_dir2.x + norm_dir1.y * norm_dir2.y;
                let angle = dot_product.clamp(-1.0, 1.0).acos();
                
                return Some((corner, angle));
            }
        }
        
        None
    }
    
    /// Generate rounded corner overlay data for direct rendering
    /// Returns corner data that can be rendered as filled shapes
    pub fn generate_corner_overlay_data(&self, layer: &GerberLayer, scaling: f32) -> (Vec<CornerOverlayShape>, usize) {
        // Use KiCad formula: RADIUS = scaling / (sin(π/4) + 1)
        let corner_radius = scaling / (std::f32::consts::PI.sin() / 4.0 + 1.0);
        let quality_issues = self.analyze_trace_quality(layer);
        let corner_issues: Vec<_> = quality_issues.into_iter()
            .filter(|issue| matches!(issue.issue_type, TraceQualityType::SharpCorner))
            .collect();
            
        if corner_issues.is_empty() {
            // Return empty overlay if no corners to fix
            return (Vec::new(), 0);
        }
        
        println!("Generating overlay shapes for {} corners with radius {:.3}mm", corner_issues.len(), corner_radius);
        
        // Generate filled corner shapes for direct rendering
        let mut overlay_shapes = Vec::new();
        let original_primitives = layer.primitives();
        let mut corners_processed = 0;
        
        // Process each corner issue
        for corner_issue in &corner_issues {
            let corner_pos = Position::new(corner_issue.location.0 as f64, corner_issue.location.1 as f64);
            
            // Find the line segments that form this corner
            if let Some((idx1, _idx2, dir1, dir2)) = self.find_corner_segments(corner_pos, original_primitives, 0.001) {
                // Get the trace width from one of the lines
                let trace_width = if let GerberPrimitive::Line { width, .. } = &original_primitives[idx1] {
                    *width as f32
                } else {
                    continue;
                };
                
                // Calculate the corner angle for proper arc generation
                let dot_product = dir1.x * dir2.x + dir1.y * dir2.y;
                let corner_angle = dot_product.clamp(-1.0, 1.0).acos();
                
                // Generate a proper filled corner shape
                let corner_shape = self.generate_filled_corner_shape(corner_pos, dir1, dir2, corner_angle, corner_radius, trace_width);
                
                overlay_shapes.push(corner_shape);
                corners_processed += 1;
            }
        }
        
        // Return overlay shapes for direct rendering
        (overlay_shapes, corners_processed)
    }
    
    /// Clone a GerberLayer (since it doesn't implement Clone)
    fn clone_layer(&self, layer: &GerberLayer) -> GerberLayer {
        let primitives = layer.primitives().to_vec();
        self.create_layer_from_primitives(primitives)
    }
    
    /// Create a new GerberLayer from a vector of primitives
    fn create_layer_from_primitives(&self, _primitives: Vec<GerberPrimitive>) -> GerberLayer {
        // Since we can't construct a GerberLayer directly from primitives,
        // we need to create empty commands and then build the layer
        // This is a workaround for the GerberLayer API limitations
        
        // WORKAROUND: Create a dummy layer for now
        // In a real implementation, we'd need access to GerberLayer's internal
        // primitive vector to replace it with our modified primitives
        
        use gerber_viewer::gerber_types::Command;
        let empty_commands: Vec<Command> = Vec::new();
        GerberLayer::new(empty_commands)
        
        // TODO: The real fix would be to either:
        // 1. Modify gerber_viewer crate to expose primitive modification
        // 2. Generate Gerber commands from our modified primitives
        // 3. Use reflection/unsafe to modify the internal primitive vector
    }
    
    /// Modify line segments to connect properly to the rounded arc
    fn modify_lines_for_arc(&self, line1: &GerberPrimitive, line2: &GerberPrimitive, corner_pos: Position, arc_radius: f32, tolerance: f64) -> Vec<GerberPrimitive> {
        let mut modified_lines = Vec::new();
        
        // Shorten the lines so they connect to the arc instead of the sharp corner
        if let (GerberPrimitive::Line { start: s1, end: e1, width: w1, exposure: exp1 }, 
                GerberPrimitive::Line { start: s2, end: e2, width: w2, exposure: exp2 }) = (line1, line2) {
            
            // For line1: determine which end connects to the corner and shorten it
            let line1_new = if (s1.x - corner_pos.x).abs() < tolerance && (s1.y - corner_pos.y).abs() < tolerance {
                // Start connects to corner, move start point away from corner by arc_radius
                let dir = Position::new(e1.x - s1.x, e1.y - s1.y);
                let norm_dir = self.normalize_vector(dir);
                let new_start = Position::new(
                    corner_pos.x + norm_dir.x * arc_radius as f64,
                    corner_pos.y + norm_dir.y * arc_radius as f64
                );
                GerberPrimitive::Line {
                    start: new_start,
                    end: *e1,
                    width: *w1,
                    exposure: exp1.clone(),
                }
            } else {
                // End connects to corner, move end point away from corner by arc_radius
                let dir = Position::new(s1.x - e1.x, s1.y - e1.y);
                let norm_dir = self.normalize_vector(dir);
                let new_end = Position::new(
                    corner_pos.x + norm_dir.x * arc_radius as f64,
                    corner_pos.y + norm_dir.y * arc_radius as f64
                );
                GerberPrimitive::Line {
                    start: *s1,
                    end: new_end,
                    width: *w1,
                    exposure: exp1.clone(),
                }
            };
            
            // For line2: similar logic
            let line2_new = if (s2.x - corner_pos.x).abs() < tolerance && (s2.y - corner_pos.y).abs() < tolerance {
                let dir = Position::new(e2.x - s2.x, e2.y - s2.y);
                let norm_dir = self.normalize_vector(dir);
                let new_start = Position::new(
                    corner_pos.x + norm_dir.x * arc_radius as f64,
                    corner_pos.y + norm_dir.y * arc_radius as f64
                );
                GerberPrimitive::Line {
                    start: new_start,
                    end: *e2,
                    width: *w2,
                    exposure: exp2.clone(),
                }
            } else {
                let dir = Position::new(s2.x - e2.x, s2.y - e2.y);
                let norm_dir = self.normalize_vector(dir);
                let new_end = Position::new(
                    corner_pos.x + norm_dir.x * arc_radius as f64,
                    corner_pos.y + norm_dir.y * arc_radius as f64
                );
                GerberPrimitive::Line {
                    start: *s2,
                    end: new_end,
                    width: *w2,
                    exposure: exp2.clone(),
                }
            };
            
            modified_lines.push(line1_new);
            modified_lines.push(line2_new);
        }
        
        modified_lines
    }
    
    /// Calculate optimal shortening amount based on KiCad algorithm
    fn calculate_shortening_amount(&self, corner_angle: f64, track_length: f32, radius: f32) -> f32 {
        // KiCad approach: theta = π/2 - half_track_angle
        let half_angle = corner_angle / 2.0;
        let theta = std::f64::consts::PI / 2.0 - half_angle;
        let f = 1.0 / (2.0 * theta.cos() + 2.0);
        
        // Use minimum of radius-based and length-based constraints
        let radius_constraint = radius * f as f32;
        let length_constraint = track_length * 0.5; // Don't shorten by more than 50% of track length
        
        radius_constraint.min(length_constraint)
    }
    
    /// Generate a simple filled corner shape for overlay rendering
    fn generate_filled_corner_shape(&self, corner_pos: Position, dir1: Position, dir2: Position, corner_angle: f64, radius: f32, trace_width: f32) -> CornerOverlayShape {
        // Use KiCad-style midpoint calculation for the curve
        let half_angle = corner_angle / 2.0;
        let theta = std::f64::consts::PI / 2.0 - half_angle;
        let f = 1.0 / (2.0 * theta.cos() + 2.0);
        
        // Calculate start and end points of the arc
        let norm_dir1 = self.normalize_vector(dir1);
        let norm_dir2 = self.normalize_vector(dir2);
        
        let shorten_amount = radius as f64 * f;
        let arc_start = Position::new(
            corner_pos.x + norm_dir1.x * shorten_amount,
            corner_pos.y + norm_dir1.y * shorten_amount
        );
        let arc_end = Position::new(
            corner_pos.x + norm_dir2.x * shorten_amount,
            corner_pos.y + norm_dir2.y * shorten_amount
        );
        
        // KiCad midpoint formula
        let midpoint = Position::new(
            corner_pos.x * (1.0 - f * 2.0) + arc_start.x * f + arc_end.x * f,
            corner_pos.y * (1.0 - f * 2.0) + arc_start.y * f + arc_end.y * f
        );
        
        // Create a simple filled triangle/wedge shape for the corner
        let mut points = Vec::new();
        
        // Add the three main points: corner + the two arc endpoints
        points.push(corner_pos);
        points.push(arc_start);
        
        // Add some curve points for smoothness (just along the centerline)
        let num_curve_points = 8;
        for i in 1..num_curve_points {
            let t = i as f64 / num_curve_points as f64;
            let curve_point = self.bezier_interpolate(arc_start, midpoint, arc_end, t);
            points.push(curve_point);
        }
        
        points.push(arc_end);
        
        CornerOverlayShape {
            points,
            trace_width,
        }
    }
    
    /// Generate arc segments to replace a sharp corner using KiCad-style midpoint calculation
    fn generate_corner_arc(&self, corner_pos: Position, dir1: Position, dir2: Position, corner_angle: f64, radius: f32, trace_width: f32) -> Vec<GerberPrimitive> {
        let mut arc_segments = Vec::new();
        
        // Use KiCad-style midpoint calculation for smoother arc generation
        let half_angle = corner_angle / 2.0;
        let theta = std::f64::consts::PI / 2.0 - half_angle;
        let f = 1.0 / (2.0 * theta.cos() + 2.0);
        
        // Calculate start and end points of the arc (shortened track endpoints)
        let norm_dir1 = self.normalize_vector(dir1);
        let norm_dir2 = self.normalize_vector(dir2);
        
        let shorten_amount = radius as f64 * f;
        let start_point = Position::new(
            corner_pos.x + norm_dir1.x * shorten_amount,
            corner_pos.y + norm_dir1.y * shorten_amount
        );
        let end_point = Position::new(
            corner_pos.x + norm_dir2.x * shorten_amount,
            corner_pos.y + norm_dir2.y * shorten_amount
        );
        
        // KiCad midpoint formula: mp = newX*(1-f*2) + sp.x*f + ep.x*f
        let midpoint = Position::new(
            corner_pos.x * (1.0 - f * 2.0) + start_point.x * f + end_point.x * f,
            corner_pos.y * (1.0 - f * 2.0) + start_point.y * f + end_point.y * f
        );
        
        // Generate overlapping line segments for better visual coverage
        let num_segments = 16; // More segments for smoother appearance
        
        // For KiCad-style arcs, we create overlapping segments that interpolate smoothly
        for i in 0..num_segments {
            let t1 = i as f64 / num_segments as f64;
            let t2 = (i + 1) as f64 / num_segments as f64;
            
            // Use quadratic Bezier curve interpolation
            let seg_start = self.bezier_interpolate(start_point, midpoint, end_point, t1);
            let seg_end = self.bezier_interpolate(start_point, midpoint, end_point, t2);
            
            // Make line segments slightly wider for better overlap
            let wider_width = trace_width * 1.1; // 10% wider for overlap
            
            arc_segments.push(GerberPrimitive::Line {
                start: seg_start,
                end: seg_end,
                width: wider_width as f64,
                exposure: gerber_viewer::Exposure::Add,
            });
        }
        
        arc_segments
    }
    
    /// Normalize a vector to unit length
    fn normalize_vector(&self, vec: Position) -> Position {
        let length = (vec.x * vec.x + vec.y * vec.y).sqrt();
        if length > 0.001 {
            Position::new(vec.x / length, vec.y / length)
        } else {
            Position::new(1.0, 0.0)
        }
    }
    
    /// Quadratic Bezier curve interpolation for smooth arc generation
    fn bezier_interpolate(&self, start: Position, control: Position, end: Position, t: f64) -> Position {
        let one_minus_t = 1.0 - t;
        let t_squared = t * t;
        let one_minus_t_squared = one_minus_t * one_minus_t;
        
        Position::new(
            one_minus_t_squared * start.x + 2.0 * one_minus_t * t * control.x + t_squared * end.x,
            one_minus_t_squared * start.y + 2.0 * one_minus_t * t * control.y + t_squared * end.y
        )
    }
    
    /// Find lines that form a corner and return the segments that need to be replaced
    fn find_corner_segments(&self, corner_pos: Position, primitives: &[GerberPrimitive], tolerance: f64) -> Option<(usize, usize, Position, Position)> {
        let mut connected_lines = Vec::new();
        
        // Find all lines connected to this corner
        for (i, primitive) in primitives.iter().enumerate() {
            if let GerberPrimitive::Line { start, end, .. } = primitive {
                let start_connected = (start.x - corner_pos.x).abs() < tolerance && (start.y - corner_pos.y).abs() < tolerance;
                let end_connected = (end.x - corner_pos.x).abs() < tolerance && (end.y - corner_pos.y).abs() < tolerance;
                
                if start_connected || end_connected {
                    connected_lines.push((i, *start, *end));
                }
            }
        }
        
        // We need exactly 2 lines to form a corner
        if connected_lines.len() == 2 {
            let (idx1, start1, end1) = connected_lines[0];
            let (idx2, start2, end2) = connected_lines[1];
            
            // Calculate direction vectors away from the corner
            let dir1 = if (start1.x - corner_pos.x).abs() < tolerance && (start1.y - corner_pos.y).abs() < tolerance {
                Position::new(end1.x - start1.x, end1.y - start1.y)
            } else {
                Position::new(start1.x - end1.x, start1.y - end1.y)
            };
            
            let dir2 = if (start2.x - corner_pos.x).abs() < tolerance && (start2.y - corner_pos.y).abs() < tolerance {
                Position::new(end2.x - start2.x, end2.y - start2.y)
            } else {
                Position::new(start2.x - end2.x, start2.y - end2.y)
            };
            
            return Some((idx1, idx2, dir1, dir2));
        }
        
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gerber_viewer::Exposure;
    
    // Helper to create a mock gerber layer for testing
    fn create_test_layer_with_primitives(_primitives: Vec<GerberPrimitive>) -> GerberLayer {
        // This would need to be implemented based on GerberLayer's constructor
        // For now, this is a placeholder
        todo!("Need to create test GerberLayer with custom primitives")
    }
    
    #[test]
    fn test_drc_simple_default() {
        let drc = DrcSimple::default();
        assert_eq!(drc.min_trace_width, 0.15);
        assert_eq!(drc.min_via_diameter, 0.3);
        assert_eq!(drc.lines_only, false);
    }
    
    #[test]
    fn test_find_trace_width_violations() {
        let drc = DrcSimple::default();
        
        let traces = vec![
            Trace {
                width: 0.1,  // Below minimum
                length: 5.0,
                center_x: 1.0,
                center_y: 2.0,
                trace_type: TraceType::Line,
            },
            Trace {
                width: 0.2,  // Above minimum
                length: 3.0,
                center_x: 3.0,
                center_y: 4.0,
                trace_type: TraceType::Rectangle,
            },
        ];
        
        let violations = drc.find_trace_width_violations(&traces);
        
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].measured_width, 0.1);
        assert_eq!(violations[0].required_width, 0.15);
    }
    
    #[test]
    fn test_lines_only_mode() {
        let drc = DrcSimple {
            lines_only: true,
            ..DrcSimple::default()
        };
        assert_eq!(drc.lines_only, true);
    }
}

// Main DRC checking functions moved from main.rs

/// Helper function to check if an aperture is likely a trace (not a pad/pour)
pub fn is_trace_aperture(diameter: f32) -> bool {
    // Apertures smaller than 0.5mm (20 mils) are likely traces
    // Larger apertures are likely pads or copper pours
    diameter < 0.5
}

/// Helper function to determine if a location is likely a trace (not in a pad cluster)
pub fn is_likely_trace_location(_x: f32, _y: f32, _diameter: f32) -> bool {
    // TODO: Implement clustering logic to identify pad locations
    // For now, accept all locations
    true
}

/// Check if a point is within the PCB boundary
pub fn is_within_pcb_boundary(x: f32, y: f32, boundary: &BoundingBox) -> bool {
    x >= boundary.min.x as f32 && 
    x <= boundary.max.x as f32 && 
    y >= boundary.min.y as f32 && 
    y <= boundary.max.y as f32
}

/// Extract coordinates from a gerber command string
pub fn extract_coordinates_from_command(command_str: &str) -> (f32, f32) {
    let mut x = 0.0;
    let mut y = 0.0;
    
    // Extract X coordinate
    if let Some(x_start) = command_str.find("x: ") {
        let x_offset = x_start + 3;
        if let Some(x_end) = command_str[x_offset..].find(',') {
            if let Ok(x_nano) = command_str[x_offset..x_offset + x_end].parse::<f32>() {
                x = x_nano;
            }
        }
    }
    
    // Extract Y coordinate  
    if let Some(y_start) = command_str.find("y: ") {
        let y_offset = y_start + 3;
        if let Some(y_end) = command_str[y_offset..].find(' ') {
            if let Ok(y_nano) = command_str[y_offset..y_offset + y_end].parse::<f32>() {
                y = y_nano;
            }
        }
    }
    
    (x, y)
}

/// Cluster DRC violations by trace  
pub fn cluster_violations_per_trace(violations: &[DrcViolation]) -> Vec<DrcViolation> {
    if violations.is_empty() {
        return Vec::new();
    }
    
    // Group violations by proximity (traces are continuous)
    let mut clusters: Vec<Vec<&DrcViolation>> = Vec::new();
    let cluster_distance = 5.0; // mm - violations within 5mm are likely same trace
    
    for violation in violations {
        let mut added_to_cluster = false;
        
        for cluster in &mut clusters {
            // Check if this violation is close to any violation in the cluster
            for cluster_violation in cluster.iter() {
                let dx = violation.x - cluster_violation.x;
                let dy = violation.y - cluster_violation.y;
                let distance = (dx * dx + dy * dy).sqrt();
                
                if distance <= cluster_distance {
                    cluster.push(violation);
                    added_to_cluster = true;
                    break;
                }
            }
            if added_to_cluster {
                break;
            }
        }
        
        if !added_to_cluster {
            clusters.push(vec![violation]);
        }
    }
    
    // Merge overlapping clusters
    let mut merged = true;
    while merged {
        merged = false;
        let mut i = 0;
        while i < clusters.len() {
            let mut j = i + 1;
            while j < clusters.len() {
                // Check if clusters should be merged
                let mut should_merge = false;
                'outer: for v1 in &clusters[i] {
                    for v2 in &clusters[j] {
                        let dx = v1.x - v2.x;
                        let dy = v1.y - v2.y;
                        let distance = (dx * dx + dy * dy).sqrt();
                        if distance <= cluster_distance {
                            should_merge = true;
                            break 'outer;
                        }
                    }
                }
                
                if should_merge {
                    // Merge cluster j into cluster i
                    let cluster_j = clusters.remove(j);
                    clusters[i].extend(cluster_j);
                    merged = true;
                } else {
                    j += 1;
                }
            }
            i += 1;
        }
    }
    
    println!("Clustered {} violations into {} traces", violations.len(), clusters.len());
    
    // Return one representative violation per cluster (trace)
    clusters.into_iter()
        .map(|cluster| {
            // Find the violation with the smallest width (worst case)
            cluster.into_iter()
                .min_by(|a, b| a.measured_value.partial_cmp(&b.measured_value).unwrap())
                .unwrap()
                .clone()
        })
        .collect()
}

/// Check trace width in gerber data
pub fn check_trace_width_in_gerber_data(
    gerber_data: &str, 
    layer_name: &str, 
    min_width: f32,
    pcb_boundary: &BoundingBox
) -> Vec<DrcViolation> {
    let mut violations = Vec::new();
    
    let reader = BufReader::new(gerber_data.as_bytes());
    match parse(reader) {
        Ok(doc) => {
            println!("Successfully parsed Gerber file for {}", layer_name);
            
            // Convert to commands like in the working code
            let commands = doc.into_commands();
            println!("Found {} commands in {}", commands.len(), layer_name);
            
            // Now parse for real DRC violations
            println!("Analyzing {} commands for DRC violations in {}", commands.len(), layer_name);
            
            // Build a map of aperture codes to their diameters
            let mut aperture_map = std::collections::HashMap::new();
            let mut current_aperture: Option<i32> = None;
            
            for command in &commands {
                // Extract aperture definitions and classify them
                let command_str = format!("{:?}", command);
                if command_str.contains("ApertureDefinition") {
                    if let Some(code_start) = command_str.find("code: ") {
                        if let Some(code_end) = command_str[code_start + 6..].find(',') {
                            if let Ok(code) = command_str[code_start + 6..code_start + 6 + code_end].parse::<i32>() {
                                // Extract diameter from circle apertures
                                if let Some(diameter_start) = command_str.find("diameter: ") {
                                    if let Some(diameter_end) = command_str[diameter_start + 10..].find(',') {
                                        if let Ok(diameter) = command_str[diameter_start + 10..diameter_start + 10 + diameter_end].parse::<f32>() {
                                            // Only include small apertures likely to be traces (not pads/pours)
                                            if is_trace_aperture(diameter) {
                                                aperture_map.insert(code, diameter);
                                                println!("Found trace aperture {}: diameter {}mm", code, diameter);
                                            } else {
                                                println!("Ignored pad/pour aperture {}: diameter {}mm (too large)", code, diameter);
                                            }
                                        }
                                    }
                                }
                                // Extract width from rectangular apertures
                                if let Some(x_start) = command_str.find("x: ") {
                                    if let Some(x_end) = command_str[x_start + 3..].find(',') {
                                        if let Ok(width) = command_str[x_start + 3..x_start + 3 + x_end].parse::<f32>() {
                                            // Only include small rectangular apertures likely to be traces
                                            if is_trace_aperture(width) {
                                                aperture_map.insert(code, width);
                                                println!("Found trace aperture {}: width {}mm", code, width);
                                            } else {
                                                println!("Ignored pad/pour aperture {}: width {}mm (too large)", code, width);
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
                
                // Track aperture selections
                if command_str.contains("SelectAperture") {
                    if let Some(aperture_start) = command_str.find("SelectAperture(") {
                        if let Some(aperture_end) = command_str[aperture_start + 15..].find(')') {
                            if let Ok(aperture) = command_str[aperture_start + 15..aperture_start + 15 + aperture_end].parse::<i32>() {
                                current_aperture = Some(aperture);
                            }
                        }
                    }
                }
                
                // Check interpolate operations (drawing lines - equivalent to D01)
                // Ignore Flash operations (D03) which are used for pads/vias
                if command_str.contains("Interpolate") && !command_str.contains("Flash") {
                    if let Some(current_aperture_code) = current_aperture {
                        if let Some(&diameter) = aperture_map.get(&current_aperture_code) {
                            if diameter < min_width {
                                // Extract coordinates from command string
                                let (x, y) = extract_coordinates_from_command(&command_str);
                                let x_mm = x / 1_000_000.0; // Convert from nanometers to mm
                                let y_mm = y / 1_000_000.0;
                                
                                // Additional filtering: reject if coordinates suggest this is near a pad/component
                                if is_within_pcb_boundary(x_mm, y_mm, pcb_boundary) && is_likely_trace_location(x_mm, y_mm, diameter) {
                                    violations.push(DrcViolation {
                                        rule_name: "Minimum Trace Width".to_string(),
                                        description: format!("Trace width {:.3}mm below minimum", diameter),
                                        layer: layer_name.to_string(),
                                        measured_value: diameter,
                                        required_value: min_width,
                                        x: x_mm,
                                        y: y_mm,
                                    });
                                }
                            }
                        }
                    }
                }
            }
            
            println!("Found {} potential trace width violations in {}", violations.len(), layer_name);
        }
        Err(e) => {
            println!("Failed to parse Gerber data for {}: {:?}", layer_name, e);
        }
    }
    
    violations
}

/// Main DRC check function - runs all configured DRC checks
pub fn run_simple_drc_check(
    layers: &HashMap<crate::layers::LayerType, crate::layers::LayerInfo>,
    drc_rules: &DrcRules,
    trace_quality_issues: &mut Vec<TraceQualityIssue>
) -> Vec<DrcViolation> {
    use crate::layers::LayerType;
    
    let mut violations = Vec::new();
    
    // Clear previous quality issues
    trace_quality_issues.clear();
    
    // Get PCB boundary from mechanical outline layer
    let pcb_boundary = if let Some(outline_info) = layers.get(&LayerType::MechanicalOutline) {
        outline_info.gerber_layer.as_ref().map(|layer| layer.bounding_box())
    } else {
        None
    };
    
    if pcb_boundary.is_none() {
        println!("Warning: No mechanical outline found - cannot determine PCB boundary for DRC");
        return violations;
    }
    
    let boundary = pcb_boundary.unwrap();
    println!("DRC boundary check: PCB area is {:.1} x {:.1} mm", boundary.width(), boundary.height());
    
    // Check each copper layer for trace width violations
    for (layer_type, layer_info) in layers {
        // Only check copper layers
        if !matches!(layer_type, LayerType::TopCopper | LayerType::BottomCopper) {
            continue;
        }
        
        // Use primitive-based DRC analysis
        if let Some(gerber_layer) = &layer_info.gerber_layer {
            println!("Running primitive-based trace detection on {}", layer_type.display_name());
            
            let drc = DrcSimple {
                min_trace_width: drc_rules.min_trace_width,
                lines_only: true,  // Only check Line primitives to avoid copper pour false positives
                min_trace_length: 1.0,  // Only lines >= 1mm are considered traces (not pad connections)
                ..DrcSimple::default()
            };
            
            // Get mechanical outline bounds for filtering
            let pcb_bounds = layers.get(&LayerType::MechanicalOutline)
                .and_then(|outline| outline.gerber_layer.as_ref())
                .map(|layer| layer.bounding_box());
                
            let primitive_violations = drc.run_trace_width_drc_with_bounds(gerber_layer, pcb_bounds);
            
            // Also analyze trace quality (corners, jogs, etc.)
            let quality_issues = drc.analyze_trace_quality(gerber_layer);
            println!("Found {} trace quality issues on {}", quality_issues.len(), layer_type.display_name());
            
            // Log corner issues specifically
            for issue in &quality_issues {
                if matches!(issue.issue_type, TraceQualityType::SharpCorner) {
                    println!("Corner issue at ({:.2}, {:.2}): {}", issue.location.0, issue.location.1, issue.description);
                }
            }
            
            // Store quality issues for this layer (extend the existing vector)
            trace_quality_issues.extend(quality_issues);
            
            if let Some(bounds) = &pcb_bounds {
                println!("PCB bounds: ({:.2}, {:.2}) to ({:.2}, {:.2})", 
                    bounds.min.x, bounds.min.y, bounds.max.x, bounds.max.y);
            }
            
            // Convert to DrcViolation format
            for (i, violation) in primitive_violations.iter().enumerate() {
                if i < 3 { // Debug first few violations
                    println!("Violation {}: trace at ({:.2}, {:.2}), width {:.3}mm", 
                        i, violation.trace.center_x, violation.trace.center_y, violation.trace.width);
                }
            }
            
            // Convert to DrcViolation format
            for violation in primitive_violations {
                violations.push(DrcViolation {
                    rule_name: "Primitive Trace Width".to_string(),
                    description: format!("Trace width {:.3}mm below minimum", violation.measured_width),
                    layer: layer_type.display_name().to_string(),
                    measured_value: violation.measured_width,
                    required_value: drc_rules.min_trace_width,
                    x: violation.trace.center_x,
                    y: violation.trace.center_y,
                });
            }
        }
        
        // Also check using raw gerber data analysis
        if let Some(raw_data) = &layer_info.raw_gerber_data {
            let raw_violations = check_trace_width_in_gerber_data(
                raw_data, 
                layer_type.display_name(), 
                drc_rules.min_trace_width,
                &boundary
            );
            
            // Cluster violations to reduce duplicates
            let clustered = cluster_violations_per_trace(&raw_violations);
            println!("Raw gerber analysis found {} violations ({} traces) on {}", 
                raw_violations.len(), clustered.len(), layer_type.display_name());
            
            violations.extend(clustered);
        }
    }
    
    violations
}