use regex::Regex;
use std::collections::HashMap;
use crate::layer_operations::LayerType;

/// Common layer name patterns found across different PCB design tools
#[derive(Debug)]
pub struct LayerDetector {
    patterns: HashMap<LayerType, Vec<Regex>>,
}

impl Default for LayerDetector {
    fn default() -> Self {
        Self::new()
    }
}

impl LayerDetector {
    pub fn new() -> Self {
        let mut patterns = HashMap::new();
        
        // Top Copper patterns
        patterns.insert(LayerType::TopCopper, vec![
            Regex::new(r"(?i)[-_\.]F[-_\.]?Cu\.gbr$").unwrap(),
            Regex::new(r"(?i)[-_\.]top[-_\.]?copper\.gbr$").unwrap(),
            Regex::new(r"(?i)[-_\.]top\.gbr$").unwrap(),
            Regex::new(r"(?i)[-_\.]front[-_\.]?copper\.gbr$").unwrap(),
            Regex::new(r"(?i)[-_\.]component\.gbr$").unwrap(),
            Regex::new(r"(?i)\.gtl$").unwrap(), // Gerber top layer
            Regex::new(r"(?i)[-_\.]layer1\.gbr$").unwrap(),
            Regex::new(r"(?i)[-_\.]l1\.gbr$").unwrap(),
        ]);
        
        // Bottom Copper patterns
        patterns.insert(LayerType::BottomCopper, vec![
            Regex::new(r"(?i)[-_\.]B[-_\.]?Cu\.gbr$").unwrap(),
            Regex::new(r"(?i)[-_\.]bottom[-_\.]?copper\.gbr$").unwrap(),
            Regex::new(r"(?i)[-_\.]bottom\.gbr$").unwrap(),
            Regex::new(r"(?i)[-_\.]back[-_\.]?copper\.gbr$").unwrap(),
            Regex::new(r"(?i)[-_\.]solder\.gbr$").unwrap(),
            Regex::new(r"(?i)\.gbl$").unwrap(), // Gerber bottom layer
            Regex::new(r"(?i)[-_\.]layer2\.gbr$").unwrap(),
            Regex::new(r"(?i)[-_\.]l2\.gbr$").unwrap(),
        ]);
        
        // Top Silkscreen patterns
        patterns.insert(LayerType::TopSilk, vec![
            Regex::new(r"(?i)[-_\.]F[-_\.]?Silk[sS]?\.gbr$").unwrap(),
            Regex::new(r"(?i)[-_\.]F[-_\.]?Silkscreen\.gbr$").unwrap(),
            Regex::new(r"(?i)[-_\.]top[-_\.]?silk(?:screen)?\.gbr$").unwrap(),
            Regex::new(r"(?i)[-_\.]front[-_\.]?silk(?:screen)?\.gbr$").unwrap(),
            Regex::new(r"(?i)[-_\.]component[-_\.]?silk(?:screen)?\.gbr$").unwrap(),
            Regex::new(r"(?i)\.gto$").unwrap(), // Gerber top overlay
            Regex::new(r"(?i)[-_\.]sst\.gbr$").unwrap(), // Silkscreen top
        ]);
        
        // Bottom Silkscreen patterns
        patterns.insert(LayerType::BottomSilk, vec![
            Regex::new(r"(?i)[-_\.]B[-_\.]?Silk[sS]?\.gbr$").unwrap(),
            Regex::new(r"(?i)[-_\.]B[-_\.]?Silkscreen\.gbr$").unwrap(),
            Regex::new(r"(?i)[-_\.]bottom[-_\.]?silk(?:screen)?\.gbr$").unwrap(),
            Regex::new(r"(?i)[-_\.]back[-_\.]?silk(?:screen)?\.gbr$").unwrap(),
            Regex::new(r"(?i)[-_\.]solder[-_\.]?silk(?:screen)?\.gbr$").unwrap(),
            Regex::new(r"(?i)\.gbo$").unwrap(), // Gerber bottom overlay
            Regex::new(r"(?i)[-_\.]ssb\.gbr$").unwrap(), // Silkscreen bottom
        ]);
        
        // Top Soldermask patterns
        patterns.insert(LayerType::TopSoldermask, vec![
            Regex::new(r"(?i)[-_\.]F[-_\.]?Mask\.gbr$").unwrap(),
            Regex::new(r"(?i)[-_\.]top[-_\.]?(?:solder)?mask\.gbr$").unwrap(),
            Regex::new(r"(?i)[-_\.]front[-_\.]?(?:solder)?mask\.gbr$").unwrap(),
            Regex::new(r"(?i)[-_\.]component[-_\.]?(?:solder)?mask\.gbr$").unwrap(),
            Regex::new(r"(?i)\.gts$").unwrap(), // Gerber top soldermask
            Regex::new(r"(?i)[-_\.]smt\.gbr$").unwrap(), // Soldermask top
        ]);
        
        // Bottom Soldermask patterns
        patterns.insert(LayerType::BottomSoldermask, vec![
            Regex::new(r"(?i)[-_\.]B[-_\.]?Mask\.gbr$").unwrap(),
            Regex::new(r"(?i)[-_\.]bottom[-_\.]?(?:solder)?mask\.gbr$").unwrap(),
            Regex::new(r"(?i)[-_\.]back[-_\.]?(?:solder)?mask\.gbr$").unwrap(),
            Regex::new(r"(?i)[-_\.]solder[-_\.]?(?:solder)?mask\.gbr$").unwrap(),
            Regex::new(r"(?i)\.gbs$").unwrap(), // Gerber bottom soldermask
            Regex::new(r"(?i)[-_\.]smb\.gbr$").unwrap(), // Soldermask bottom
        ]);
        
        // Top Paste patterns
        patterns.insert(LayerType::TopPaste, vec![
            Regex::new(r"(?i)[-_\.]F[-_\.]?Paste\.gbr$").unwrap(),
            Regex::new(r"(?i)[-_\.]top[-_\.]?paste\.gbr$").unwrap(),
            Regex::new(r"(?i)[-_\.]front[-_\.]?paste\.gbr$").unwrap(),
            Regex::new(r"(?i)[-_\.]component[-_\.]?paste\.gbr$").unwrap(),
            Regex::new(r"(?i)\.gtp$").unwrap(), // Gerber top paste
            Regex::new(r"(?i)[-_\.]spt\.gbr$").unwrap(), // Solderpaste top
        ]);
        
        // Bottom Paste patterns
        patterns.insert(LayerType::BottomPaste, vec![
            Regex::new(r"(?i)[-_\.]B[-_\.]?Paste\.gbr$").unwrap(),
            Regex::new(r"(?i)[-_\.]bottom[-_\.]?paste\.gbr$").unwrap(),
            Regex::new(r"(?i)[-_\.]back[-_\.]?paste\.gbr$").unwrap(),
            Regex::new(r"(?i)[-_\.]solder[-_\.]?paste\.gbr$").unwrap(),
            Regex::new(r"(?i)\.gbp$").unwrap(), // Gerber bottom paste
            Regex::new(r"(?i)[-_\.]spb\.gbr$").unwrap(), // Solderpaste bottom
        ]);
        
        // Mechanical Outline patterns
        patterns.insert(LayerType::MechanicalOutline, vec![
            Regex::new(r"(?i)[-_\.]Edge[-_\.]?Cuts\.gbr$").unwrap(),
            Regex::new(r"(?i)[-_\.]outline\.gbr$").unwrap(),
            Regex::new(r"(?i)[-_\.]board[-_\.]?outline\.gbr$").unwrap(),
            Regex::new(r"(?i)[-_\.]mechanical\.gbr$").unwrap(),
            Regex::new(r"(?i)[-_\.]profile\.gbr$").unwrap(),
            Regex::new(r"(?i)\.gko$").unwrap(), // Gerber keepout/outline
            Regex::new(r"(?i)\.gm1$").unwrap(), // Gerber mechanical 1
            Regex::new(r"(?i)[-_\.]routing\.gbr$").unwrap(),
        ]);
        
        Self { patterns }
    }
    
    /// Try to detect layer type from filename using regex patterns
    pub fn detect_layer_type(&self, filename: &str) -> Option<LayerType> {
        for (layer_type, patterns) in &self.patterns {
            for pattern in patterns {
                if pattern.is_match(filename) {
                    return Some(*layer_type);
                }
            }
        }
        None
    }
    
    /// Get all patterns for a specific layer type (for display/debugging)
    pub fn get_patterns_for_layer(&self, layer_type: LayerType) -> Vec<String> {
        self.patterns.get(&layer_type)
            .map(|patterns| patterns.iter()
                .map(|p| p.as_str().to_string())
                .collect())
            .unwrap_or_default()
    }
}

/// Represents unassigned gerber files that couldn't be automatically detected
#[derive(Debug)]
pub struct UnassignedGerber {
    pub filename: String,
    pub content: String,
    pub parsed_layer: gerber_viewer::GerberLayer,
}