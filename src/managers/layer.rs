use serde;
use std::collections::HashMap;
use crate::layers::{LayerType, LayerInfo};
use crate::layer_detection::{LayerDetector, UnassignedGerber};

/// Manager for all layer-related functionality
#[derive(Debug)]
pub struct LayerManager {
    /// Multi-layer support - map of layer types to their information
    pub layers: HashMap<LayerType, LayerInfo>,
    
    /// Currently active/selected layer
    pub active_layer: LayerType,
    
    /// Layer detection system for auto-assignment
    pub layer_detector: LayerDetector,
    
    /// Gerber files that couldn't be automatically assigned to layers
    pub unassigned_gerbers: Vec<UnassignedGerber>,
    
    /// Manual layer assignments (filename -> layer type)
    pub layer_assignments: HashMap<String, LayerType>,
}

impl LayerManager {
    /// Create a new LayerManager with default settings
    pub fn new() -> Self {
        Self {
            layers: HashMap::new(),
            active_layer: LayerType::TopCopper,
            layer_detector: LayerDetector::new(),
            unassigned_gerbers: Vec::new(),
            layer_assignments: HashMap::new(),
        }
    }
    
    /// Add or update a layer
    pub fn add_layer(&mut self, layer_type: LayerType, layer_info: LayerInfo) {
        self.layers.insert(layer_type, layer_info);
    }
    
    /// Remove a layer
    pub fn remove_layer(&mut self, layer_type: &LayerType) -> Option<LayerInfo> {
        self.layers.remove(layer_type)
    }
    
    /// Get a layer by type
    pub fn get_layer(&self, layer_type: &LayerType) -> Option<&LayerInfo> {
        self.layers.get(layer_type)
    }
    
    /// Get a mutable reference to a layer by type
    pub fn get_layer_mut(&mut self, layer_type: &LayerType) -> Option<&mut LayerInfo> {
        self.layers.get_mut(layer_type)
    }
    
    /// Set the active layer
    pub fn set_active_layer(&mut self, layer_type: LayerType) {
        self.active_layer = layer_type;
    }
    
    /// Get the active layer
    pub fn get_active_layer(&self) -> LayerType {
        self.active_layer
    }
    
    /// Clear all layers and assignments
    pub fn clear_all(&mut self) {
        self.layers.clear();
        self.unassigned_gerbers.clear();
        self.layer_assignments.clear();
    }
    
    /// Add an unassigned gerber file
    pub fn add_unassigned_gerber(&mut self, gerber: UnassignedGerber) {
        self.unassigned_gerbers.push(gerber);
    }
    
    /// Remove an unassigned gerber by index
    pub fn remove_unassigned_gerber(&mut self, index: usize) -> Option<UnassignedGerber> {
        if index < self.unassigned_gerbers.len() {
            Some(self.unassigned_gerbers.remove(index))
        } else {
            None
        }
    }
    
    /// Assign a layer manually
    pub fn assign_layer(&mut self, filename: String, layer_type: LayerType) {
        self.layer_assignments.insert(filename, layer_type);
    }
    
    /// Remove a layer assignment
    pub fn remove_assignment(&mut self, filename: &str) -> Option<LayerType> {
        self.layer_assignments.remove(filename)
    }
    
    /// Get the assignment for a filename
    pub fn get_assignment(&self, filename: &str) -> Option<&LayerType> {
        self.layer_assignments.get(filename)
    }
    
    /// Check if a layer type is already assigned
    pub fn is_layer_assigned(&self, layer_type: &LayerType) -> bool {
        self.layer_assignments.values().any(|lt| lt == layer_type)
    }
    
    /// Get all visible layers
    pub fn get_visible_layers(&self) -> Vec<(&LayerType, &LayerInfo)> {
        self.layers.iter()
            .filter(|(_, layer_info)| layer_info.visible)
            .collect()
    }
    
    /// Toggle layer visibility
    pub fn toggle_layer_visibility(&mut self, layer_type: &LayerType) {
        if let Some(layer_info) = self.layers.get_mut(layer_type) {
            layer_info.visible = !layer_info.visible;
        }
    }
    
    /// Set layer visibility
    pub fn set_layer_visibility(&mut self, layer_type: &LayerType, visible: bool) {
        if let Some(layer_info) = self.layers.get_mut(layer_type) {
            layer_info.visible = visible;
        }
    }
    
    /// Get the number of loaded layers
    pub fn layer_count(&self) -> usize {
        self.layers.len()
    }
    
    /// Get the number of unassigned gerbers
    pub fn unassigned_count(&self) -> usize {
        self.unassigned_gerbers.len()
    }
    
    /// Get layer statistics
    pub fn get_statistics(&self) -> LayerStatistics {
        LayerStatistics {
            total_layers: self.layer_count(),
            visible_layers: self.get_visible_layers().len(),
            unassigned_gerbers: self.unassigned_count(),
            assignments: self.layer_assignments.len(),
        }
    }
    
    /// Auto-detect layer type for a filename
    pub fn detect_layer_type(&self, filename: &str) -> Option<LayerType> {
        self.layer_detector.detect_layer_type(filename)
    }
}

impl Default for LayerManager {
    fn default() -> Self {
        Self::new()
    }
}

// Custom deserialization to handle skipped fields
impl<'de> serde::Deserialize<'de> for LayerManager {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(serde::Deserialize)]
        struct LayerManagerData {
            active_layer: LayerType,
            layer_assignments: HashMap<String, LayerType>,
        }
        
        let data = LayerManagerData::deserialize(deserializer)?;
        
        Ok(LayerManager {
            layers: HashMap::new(),
            active_layer: data.active_layer,
            layer_detector: LayerDetector::new(),
            unassigned_gerbers: Vec::new(),
            layer_assignments: data.layer_assignments,
        })
    }
}

impl serde::Serialize for LayerManager {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        
        let mut state = serializer.serialize_struct("LayerManager", 2)?;
        state.serialize_field("active_layer", &self.active_layer)?;
        state.serialize_field("layer_assignments", &self.layer_assignments)?;
        state.end()
    }
}

/// Statistics about the layer manager state
#[derive(Debug, Clone)]
pub struct LayerStatistics {
    pub total_layers: usize,
    pub visible_layers: usize,
    pub unassigned_gerbers: usize,
    pub assignments: usize,
}