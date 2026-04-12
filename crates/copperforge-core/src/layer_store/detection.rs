//! Auto-detection of layer types from gerber filenames.

use regex::Regex;
use std::collections::HashMap;
use super::{LayerType, Side};

/// Unassigned gerber file awaiting manual or auto assignment.
#[derive(Debug, Clone)]
pub struct UnassignedGerber {
    pub filename: String,
    pub content: String,
    pub parsed_layer: gerber_viewer::GerberLayer,
}

/// Detects layer types from gerber filenames using regex patterns.
pub struct LayerDetector {
    patterns: HashMap<LayerType, Vec<Regex>>,
}

impl Default for LayerDetector {
    fn default() -> Self { Self::new() }
}

impl LayerDetector {
    pub fn new() -> Self {
        let mut p = HashMap::new();

        // Top Copper (L1)
        p.insert(LayerType::Copper(1), vec![
            r"(?i)[-_\.]F[-_\.]?Cu\.gbr$", r"(?i)[-_\.]top[-_\.]?copper\.gbr$",
            r"(?i)[-_\.]top\.gbr$", r"(?i)[-_\.]front[-_\.]?copper\.gbr$",
            r"(?i)[-_\.]component\.gbr$", r"(?i)\.gtl$",
            r"(?i)[-_\.]layer1\.gbr$", r"(?i)[-_\.]l1\.gbr$",
        ].into_iter().map(|s| Regex::new(s).unwrap()).collect());

        // Bottom Copper (L2)
        p.insert(LayerType::Copper(2), vec![
            r"(?i)[-_\.]B[-_\.]?Cu\.gbr$", r"(?i)[-_\.]bottom[-_\.]?copper\.gbr$",
            r"(?i)[-_\.]bottom\.gbr$", r"(?i)[-_\.]back[-_\.]?copper\.gbr$",
            r"(?i)[-_\.]solder\.gbr$", r"(?i)\.gbl$",
            r"(?i)[-_\.]layer2\.gbr$", r"(?i)[-_\.]l2\.gbr$",
        ].into_iter().map(|s| Regex::new(s).unwrap()).collect());

        // Inner layers
        p.insert(LayerType::Copper(3), vec![
            r"(?i)[-_\.]In1[-_\.]?Cu\.gbr$", r"(?i)[-_\.]inner1\.gbr$",
            r"(?i)[-_\.]layer3\.gbr$", r"(?i)[-_\.]l3\.gbr$", r"(?i)\.g1$",
        ].into_iter().map(|s| Regex::new(s).unwrap()).collect());

        p.insert(LayerType::Copper(4), vec![
            r"(?i)[-_\.]In2[-_\.]?Cu\.gbr$", r"(?i)[-_\.]inner2\.gbr$",
            r"(?i)[-_\.]layer4\.gbr$", r"(?i)[-_\.]l4\.gbr$", r"(?i)\.g2$",
        ].into_iter().map(|s| Regex::new(s).unwrap()).collect());

        // Silkscreen
        p.insert(LayerType::Silkscreen(Side::Top), vec![
            r"(?i)[-_\.]F[-_\.]?Silk[sS]?\.gbr$", r"(?i)[-_\.]F[-_\.]?Silkscreen\.gbr$",
            r"(?i)[-_\.]top[-_\.]?silk(?:screen)?\.gbr$", r"(?i)\.gto$", r"(?i)[-_\.]sst\.gbr$",
        ].into_iter().map(|s| Regex::new(s).unwrap()).collect());

        p.insert(LayerType::Silkscreen(Side::Bottom), vec![
            r"(?i)[-_\.]B[-_\.]?Silk[sS]?\.gbr$", r"(?i)[-_\.]B[-_\.]?Silkscreen\.gbr$",
            r"(?i)[-_\.]bottom[-_\.]?silk(?:screen)?\.gbr$", r"(?i)\.gbo$", r"(?i)[-_\.]ssb\.gbr$",
        ].into_iter().map(|s| Regex::new(s).unwrap()).collect());

        // Soldermask
        p.insert(LayerType::Soldermask(Side::Top), vec![
            r"(?i)[-_\.]F[-_\.]?Mask\.gbr$", r"(?i)[-_\.]top[-_\.]?(?:solder)?mask\.gbr$",
            r"(?i)\.gts$", r"(?i)[-_\.]smt\.gbr$",
        ].into_iter().map(|s| Regex::new(s).unwrap()).collect());

        p.insert(LayerType::Soldermask(Side::Bottom), vec![
            r"(?i)[-_\.]B[-_\.]?Mask\.gbr$", r"(?i)[-_\.]bottom[-_\.]?(?:solder)?mask\.gbr$",
            r"(?i)\.gbs$", r"(?i)[-_\.]smb\.gbr$",
        ].into_iter().map(|s| Regex::new(s).unwrap()).collect());

        // Paste
        p.insert(LayerType::Paste(Side::Top), vec![
            r"(?i)[-_\.]F[-_\.]?Paste\.gbr$", r"(?i)[-_\.]top[-_\.]?paste\.gbr$", r"(?i)\.gtp$",
        ].into_iter().map(|s| Regex::new(s).unwrap()).collect());

        p.insert(LayerType::Paste(Side::Bottom), vec![
            r"(?i)[-_\.]B[-_\.]?Paste\.gbr$", r"(?i)[-_\.]bottom[-_\.]?paste\.gbr$", r"(?i)\.gbp$",
        ].into_iter().map(|s| Regex::new(s).unwrap()).collect());

        // Mechanical Outline
        p.insert(LayerType::MechanicalOutline, vec![
            r"(?i)[-_\.]Edge[-_\.]?Cuts\.gbr$", r"(?i)[-_\.]outline\.gbr$",
            r"(?i)[-_\.]board[-_\.]?outline\.gbr$", r"(?i)[-_\.]mechanical\.gbr$",
            r"(?i)\.gko$", r"(?i)\.gm1$",
        ].into_iter().map(|s| Regex::new(s).unwrap()).collect());

        Self { patterns: p }
    }

    pub fn detect_layer_type(&self, filename: &str) -> Option<LayerType> {
        for (layer_type, patterns) in &self.patterns {
            for pattern in patterns {
                if pattern.is_match(filename) { return Some(*layer_type); }
            }
        }
        None
    }
}
