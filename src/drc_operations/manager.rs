use serde::{Deserialize, Serialize};
use super::types::{DrcRules, DrcViolation, TraceQualityIssue, CornerOverlayShape};
use gerber_viewer::GerberPrimitive;

/// Manager for all DRC (Design Rule Check) related functionality
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DrcManager {
    /// Currently selected DRC ruleset name
    pub current_ruleset: Option<String>,
    
    /// DRC rules configuration
    pub rules: DrcRules,
    
    /// List of detected DRC violations
    pub violations: Vec<DrcViolation>,
    
    /// List of trace quality issues
    pub trace_quality_issues: Vec<TraceQualityIssue>,
    
    /// Primitives with rounded corners (for visualization)
    #[serde(skip)] // Skip serialization as GerberPrimitive might not be serializable
    pub rounded_corner_primitives: Vec<GerberPrimitive>,
    
    /// Corner overlay shapes for visualization
    #[serde(skip)] // Skip serialization as CornerOverlayShape contains non-serializable Position
    pub corner_overlay_shapes: Vec<CornerOverlayShape>,
}

impl DrcManager {
    /// Create a new DrcManager with default settings
    pub fn new() -> Self {
        Self {
            current_ruleset: None,
            rules: DrcRules::default(),
            violations: Vec::new(),
            trace_quality_issues: Vec::new(),
            rounded_corner_primitives: Vec::new(),
            corner_overlay_shapes: Vec::new(),
        }
    }
    
    /// Clear all DRC violations and issues
    pub fn clear_violations(&mut self) {
        self.violations.clear();
        self.trace_quality_issues.clear();
        self.corner_overlay_shapes.clear();
        self.rounded_corner_primitives.clear();
    }
    
    /// Add a new DRC violation
    pub fn add_violation(&mut self, violation: DrcViolation) {
        self.violations.push(violation);
    }
    
    /// Add a new trace quality issue
    pub fn add_trace_quality_issue(&mut self, issue: TraceQualityIssue) {
        self.trace_quality_issues.push(issue);
    }
    
    /// Add a corner overlay shape for visualization
    pub fn add_corner_overlay_shape(&mut self, shape: CornerOverlayShape) {
        self.corner_overlay_shapes.push(shape);
    }
    
    /// Add a rounded corner primitive
    pub fn add_rounded_corner_primitive(&mut self, primitive: GerberPrimitive) {
        self.rounded_corner_primitives.push(primitive);
    }
    
    /// Get the total number of violations
    pub fn violation_count(&self) -> usize {
        self.violations.len()
    }
    
    /// Get the total number of trace quality issues
    pub fn trace_quality_issue_count(&self) -> usize {
        self.trace_quality_issues.len()
    }
    
    /// Check if there are any DRC issues
    pub fn has_issues(&self) -> bool {
        !self.violations.is_empty() || !self.trace_quality_issues.is_empty()
    }
    
    /// Get a summary of DRC status
    pub fn get_status_summary(&self) -> String {
        let violation_count = self.violation_count();
        let issue_count = self.trace_quality_issue_count();
        
        match (violation_count, issue_count) {
            (0, 0) => "No DRC issues found".to_string(),
            (v, 0) => format!("{} DRC violation{}", v, if v == 1 { "" } else { "s" }),
            (0, i) => format!("{} trace quality issue{}", i, if i == 1 { "" } else { "s" }),
            (v, i) => format!("{} DRC violation{}, {} trace quality issue{}", 
                             v, if v == 1 { "" } else { "s" },
                             i, if i == 1 { "" } else { "s" }),
        }
    }
    
    /// Set the current DRC ruleset
    pub fn set_current_ruleset(&mut self, ruleset_name: Option<String>) {
        self.current_ruleset = ruleset_name;
    }
    
    /// Update DRC rules
    pub fn update_rules(&mut self, rules: DrcRules) {
        self.rules = rules;
    }
}

impl Default for DrcManager {
    fn default() -> Self {
        Self::new()
    }
}