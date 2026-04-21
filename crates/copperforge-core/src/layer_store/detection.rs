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
            r"(?i)-Top Layer\.gbr$",  // KiCad 10 --no-protel-ext
        ].into_iter().map(|s| Regex::new(s).unwrap()).collect());

        // Bottom Copper (L2)
        p.insert(LayerType::Copper(2), vec![
            r"(?i)[-_\.]B[-_\.]?Cu\.gbr$", r"(?i)[-_\.]bottom[-_\.]?copper\.gbr$",
            r"(?i)[-_\.]bottom\.gbr$", r"(?i)[-_\.]back[-_\.]?copper\.gbr$",
            r"(?i)[-_\.]solder\.gbr$", r"(?i)\.gbl$",
            r"(?i)[-_\.]layer2\.gbr$", r"(?i)[-_\.]l2\.gbr$",
            r"(?i)-Bottom Layer\.gbr$",  // KiCad 10 --no-protel-ext
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

        // Silkscreen (KiCad: "Overlay" = silkscreen)
        p.insert(LayerType::Silkscreen(Side::Top), vec![
            r"(?i)[-_\.]F[-_\.]?Silk[sS]?\.gbr$", r"(?i)[-_\.]F[-_\.]?Silkscreen\.gbr$",
            r"(?i)[-_\.]top[-_\.]?silk(?:screen)?\.gbr$", r"(?i)\.gto$", r"(?i)[-_\.]sst\.gbr$",
            r"(?i)-Top Overlay\.gbr$",  // KiCad 10 --no-protel-ext
        ].into_iter().map(|s| Regex::new(s).unwrap()).collect());

        p.insert(LayerType::Silkscreen(Side::Bottom), vec![
            r"(?i)[-_\.]B[-_\.]?Silk[sS]?\.gbr$", r"(?i)[-_\.]B[-_\.]?Silkscreen\.gbr$",
            r"(?i)[-_\.]bottom[-_\.]?silk(?:screen)?\.gbr$", r"(?i)\.gbo$", r"(?i)[-_\.]ssb\.gbr$",
            r"(?i)-Bottom Overlay\.gbr$",  // KiCad 10 --no-protel-ext
        ].into_iter().map(|s| Regex::new(s).unwrap()).collect());

        // Soldermask (KiCad: "Solder" = soldermask)
        p.insert(LayerType::Soldermask(Side::Top), vec![
            r"(?i)[-_\.]F[-_\.]?Mask\.gbr$", r"(?i)[-_\.]top[-_\.]?(?:solder)?mask\.gbr$",
            r"(?i)\.gts$", r"(?i)[-_\.]smt\.gbr$",
            r"(?i)-Top Solder\.gbr$",  // KiCad 10 --no-protel-ext
        ].into_iter().map(|s| Regex::new(s).unwrap()).collect());

        p.insert(LayerType::Soldermask(Side::Bottom), vec![
            r"(?i)[-_\.]B[-_\.]?Mask\.gbr$", r"(?i)[-_\.]bottom[-_\.]?(?:solder)?mask\.gbr$",
            r"(?i)\.gbs$", r"(?i)[-_\.]smb\.gbr$",
            r"(?i)-Bottom Solder\.gbr$",  // KiCad 10 --no-protel-ext
        ].into_iter().map(|s| Regex::new(s).unwrap()).collect());

        // Paste
        p.insert(LayerType::Paste(Side::Top), vec![
            r"(?i)[-_\.]F[-_\.]?Paste\.gbr$", r"(?i)[-_\.]top[-_\.]?paste\.gbr$", r"(?i)\.gtp$",
            r"(?i)-Top Paste\.gbr$",  // KiCad 10 --no-protel-ext
        ].into_iter().map(|s| Regex::new(s).unwrap()).collect());

        p.insert(LayerType::Paste(Side::Bottom), vec![
            r"(?i)[-_\.]B[-_\.]?Paste\.gbr$", r"(?i)[-_\.]bottom[-_\.]?paste\.gbr$", r"(?i)\.gbp$",
            r"(?i)-Bottom Paste\.gbr$",  // KiCad 10 --no-protel-ext
        ].into_iter().map(|s| Regex::new(s).unwrap()).collect());

        // Mechanical Outline
        p.insert(LayerType::MechanicalOutline, vec![
            r"(?i)[-_\.]Edge[-_\.]?Cuts\.gbr$", r"(?i)[-_\.]outline\.gbr$",
            r"(?i)[-_\.]board[-_\.]?outline\.gbr$", r"(?i)[-_\.]mechanical\.gbr$",
            r"(?i)\.gko$", r"(?i)\.gm1$",
        ].into_iter().map(|s| Regex::new(s).unwrap()).collect());

        // Via plugging (KiCad 10 via filling / tenting layers).
        p.insert(LayerType::ViaPlugging(Side::Top), vec![
            r"(?i)[-_\.]plugging[-_\.]?front\.gbr$",
            r"(?i)[-_\.]plugging[-_\.]?top\.gbr$",
            r"(?i)[-_\.]F[-_\.]?Plugging\.gbr$",
        ].into_iter().map(|s| Regex::new(s).unwrap()).collect());

        p.insert(LayerType::ViaPlugging(Side::Bottom), vec![
            r"(?i)[-_\.]plugging[-_\.]?back\.gbr$",
            r"(?i)[-_\.]plugging[-_\.]?bottom\.gbr$",
            r"(?i)[-_\.]B[-_\.]?Plugging\.gbr$",
        ].into_iter().map(|s| Regex::new(s).unwrap()).collect());

        // Drill holes exported as gerber (kicad-cli pcb export drill --format gerber).
        // KiCad splits drill output by default into plated (PTH) and non-plated (NPTH)
        // through-holes; both land in the same `LayerType::Drill` bucket. KiCad 10's
        // --no-protel-ext naming is similar: "<project>-PTH-drl.gbr" etc.
        p.insert(LayerType::Drill, vec![
            r"(?i)[-_\.](?:PTH|NPTH)[-_\.]?drl\.gbr$",   // *-PTH-drl.gbr, *-NPTH-drl.gbr
            r"(?i)[-_\.]drill\.gbr$",                    // merged drill file
            r"(?i)-Drill[- ]?Holes\.gbr$",               // KiCad 10 --no-protel-ext
            r"(?i)\.gdrl$",                              // some CAM tools
        ].into_iter().map(|s| Regex::new(s).unwrap()).collect());

        Self { patterns: p }
    }

    pub fn detect_layer_type(&self, filename: &str) -> Option<LayerType> {
        self.detect_layer_type_with_names(filename, &HashMap::new())
    }

    /// Like [`detect_layer_type`], but also matches against the KiCad 10
    /// canonical names for User.N slots ("Top 3D Body", "Capping",
    /// "Filling", etc). kicad-cli writes `<project>-<LayerName>.gbr`, so
    /// for slots whose name isn't an `Mn` token the filename is the only
    /// way back to the slot — this pass reconnects those.
    pub fn detect_layer_type_with_names(
        &self,
        filename: &str,
        user_names: &HashMap<u8, String>,
    ) -> Option<LayerType> {
        for (layer_type, patterns) in &self.patterns {
            for pattern in patterns {
                if pattern.is_match(filename) { return Some(*layer_type); }
            }
        }

        // Name-map pass — kicad-cli writes `<project>-<LayerName>.gbr` so if
        // the filename ends in `-<UserLayerName>.gbr` we can bind it back to
        // its slot. Iterate longest-first so "Top 3D Body" wins over "Top".
        if !user_names.is_empty() {
            let mut by_len: Vec<(&u8, &String)> = user_names.iter().collect();
            by_len.sort_by(|a, b| b.1.len().cmp(&a.1.len()));
            for (idx, name) in by_len {
                let suffix = format!("-{}.gbr", name);
                if filename.eq_ignore_ascii_case(&suffix)
                    || filename.to_ascii_lowercase().ends_with(&suffix.to_ascii_lowercase())
                {
                    return Some(LayerType::UserLayer(*idx));
                }
            }
        }

        // Fallback: KiCad user layers named Mn or User.n or User_n.
        // Capture the index so a single fallback handles all 45 slots.
        static USER_LAYER_PATTERNS: once_cell::sync::Lazy<[Regex; 3]> =
            once_cell::sync::Lazy::new(|| [
                // "<project>-M12 Stackup.gbr", "-M1_Board_Outline.gbr", "-M10.gbr"
                Regex::new(r"(?i)[-_\.]M(\d+)(?:[ _-][^.]*)?\.gbr$").unwrap(),
                // "<project>-User_1.gbr" / "-User_12 Notes.gbr"
                Regex::new(r"(?i)[-_\.]User[_](\d+)(?:[ _-][^.]*)?\.gbr$").unwrap(),
                // KiCad canonical "-User.N.gbr" style
                Regex::new(r"(?i)[-_\.]User\.(\d+)\.gbr$").unwrap(),
            ]);
        for re in USER_LAYER_PATTERNS.iter() {
            if let Some(caps) = re.captures(filename) {
                if let Some(n_str) = caps.get(1) {
                    if let Ok(n) = n_str.as_str().parse::<u8>() {
                        if (1..=45).contains(&n) {
                            return Some(LayerType::UserLayer(n));
                        }
                    }
                }
            }
        }
        None
    }
}
