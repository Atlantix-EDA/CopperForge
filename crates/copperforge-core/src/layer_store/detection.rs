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

        // 6- and 8-layer stacks. Without these, a 6-layer board's In3/In4
        // files match nothing and strand in "unassigned" with a misleading
        // Top-Copper default — the detector must cover every inner KiCad
        // emits, not just the 4-layer case.
        p.insert(LayerType::Copper(5), vec![
            r"(?i)[-_\.]In3[-_\.]?Cu\.gbr$", r"(?i)[-_\.]inner3\.gbr$",
            r"(?i)[-_\.]layer5\.gbr$", r"(?i)[-_\.]l5\.gbr$", r"(?i)\.g3$",
        ].into_iter().map(|s| Regex::new(s).unwrap()).collect());

        p.insert(LayerType::Copper(6), vec![
            r"(?i)[-_\.]In4[-_\.]?Cu\.gbr$", r"(?i)[-_\.]inner4\.gbr$",
            r"(?i)[-_\.]layer6\.gbr$", r"(?i)[-_\.]l6\.gbr$", r"(?i)\.g4$",
        ].into_iter().map(|s| Regex::new(s).unwrap()).collect());

        p.insert(LayerType::Copper(7), vec![
            r"(?i)[-_\.]In5[-_\.]?Cu\.gbr$", r"(?i)[-_\.]inner5\.gbr$",
            r"(?i)[-_\.]layer7\.gbr$", r"(?i)[-_\.]l7\.gbr$", r"(?i)\.g5$",
        ].into_iter().map(|s| Regex::new(s).unwrap()).collect());

        p.insert(LayerType::Copper(8), vec![
            r"(?i)[-_\.]In6[-_\.]?Cu\.gbr$", r"(?i)[-_\.]inner6\.gbr$",
            r"(?i)[-_\.]layer8\.gbr$", r"(?i)[-_\.]l8\.gbr$", r"(?i)\.g6$",
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

        // User.Comments (Cmts.User) — KiCad draws board dimensions here.
        // kicad-cli names it `<project>-User_Comments.gbr` (or `-Cmts_User`,
        // `-Comments`) depending on protel-ext settings.
        p.insert(LayerType::UserComments, vec![
            r"(?i)[-_\.]User[-_\.]?Comments\.gbr$",
            r"(?i)[-_\.]Cmts[-_\.]?User\.gbr$",
            r"(?i)[-_\.]Comments\.gbr$",
            r"(?i)\.gbr_cmts$",
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
            r"(?i)(?:round|slot)holes\.txt$",            // Altium Excellon (*-RoundHoles.TXT)
            r"(?i)\.drl$",                               // KiCad Excellon
            r"(?i)\.xln$",                               // generic Excellon
        ].into_iter().map(|s| Regex::new(s).unwrap()).collect());

        Self { patterns: p }
    }

    pub fn detect_layer_type(&self, filename: &str) -> Option<LayerType> {
        self.detect_layer_type_with_names(filename, &HashMap::new(), &HashMap::new())
    }

    /// Like [`detect_layer_type`], but also matches against the board's own
    /// layer names: the KiCad 10 canonical names for User.N slots ("Top 3D
    /// Body", "Capping", …) AND user renames of canonical layers
    /// ("Edge.Cuts" → "Multi-Layer" on EasyEDA-Pro imports). kicad-cli
    /// writes `<project>-<LayerName>.gbr` using the USER name, so for any
    /// renamed layer the filename is the only way back to the slot — this
    /// pass reconnects those. `canonical_renames` maps canonical layer name
    /// → user name, as parsed from the `(layers ...)` block.
    pub fn detect_layer_type_with_names(
        &self,
        filename: &str,
        user_names: &HashMap<u8, String>,
        canonical_renames: &HashMap<String, String>,
    ) -> Option<LayerType> {
        for (layer_type, patterns) in &self.patterns {
            for pattern in patterns {
                if pattern.is_match(filename) { return Some(*layer_type); }
            }
        }

        // Name-map pass — kicad-cli writes `<project>-<LayerName>.gbr` so if
        // the filename ends in `-<UserLayerName>.gbr` we can bind it back to
        // its slot. Both maps feed one candidate list, iterated longest-first
        // so "Top Solder Mask Layer" wins over a hypothetical "Mask Layer".
        let mut candidates: Vec<(&str, LayerType)> = user_names
            .iter()
            .map(|(idx, name)| (name.as_str(), LayerType::UserLayer(*idx)))
            .collect();
        for (canonical, name) in canonical_renames {
            if let Some(lt) = layer_type_for_canonical_name(canonical) {
                candidates.push((name.as_str(), lt));
            }
        }
        if !candidates.is_empty() {
            candidates.sort_by(|a, b| b.0.len().cmp(&a.0.len()));
            let fname = filename.to_ascii_lowercase();
            for (name, layer_type) in candidates {
                // kicad-cli sanitizes dots in layer names to underscores
                // ("User.Comments" → `-User_Comments.gbr`), so match both
                // the raw and the sanitized form.
                let raw = format!("-{}.gbr", name.to_ascii_lowercase());
                let sanitized = format!("-{}.gbr", name.to_ascii_lowercase().replace('.', "_"));
                if fname.ends_with(&raw) || fname.ends_with(&sanitized) {
                    return Some(layer_type);
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

/// Map a canonical KiCad layer name to the detector's LayerType space
/// (2-layer-first copper numbering: B.Cu = Copper(2), In1.Cu = Copper(3) —
/// `remap_copper_for_stack` translates to stack positions at insert time).
/// Layers CopperForge has no slot for (Fab, Courtyard, Adhesive, Eco, …)
/// return None and stay unassigned.
fn layer_type_for_canonical_name(canonical: &str) -> Option<LayerType> {
    match canonical {
        "F.Cu" => Some(LayerType::Copper(1)),
        "B.Cu" => Some(LayerType::Copper(2)),
        "F.SilkS" => Some(LayerType::Silkscreen(Side::Top)),
        "B.SilkS" => Some(LayerType::Silkscreen(Side::Bottom)),
        "F.Mask" => Some(LayerType::Soldermask(Side::Top)),
        "B.Mask" => Some(LayerType::Soldermask(Side::Bottom)),
        "F.Paste" => Some(LayerType::Paste(Side::Top)),
        "B.Paste" => Some(LayerType::Paste(Side::Bottom)),
        "Edge.Cuts" => Some(LayerType::MechanicalOutline),
        "Cmts.User" => Some(LayerType::UserComments),
        _ => {
            // Inner copper: "In7.Cu" → detector-space Copper(9).
            let n: u8 = canonical.strip_prefix("In")?.strip_suffix(".Cu")?.parse().ok()?;
            Some(LayerType::Copper(n.checked_add(2)?))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Boards imported from EasyEDA Pro rename canonical layers, and
    /// kicad-cli names the exported gerbers after the USER name — the
    /// rename map is the only way back. Mirrors the LoadCellProject
    /// Interface board that originally hit this.
    #[test]
    fn canonical_renames_bind_exported_filenames() {
        let det = LayerDetector::new();
        let user: HashMap<u8, String> =
            [(1u8, "Ratline Layer".to_string()), (4u8, "3D Shell Outline Layer".to_string())]
                .into_iter()
                .collect();
        let renames: HashMap<String, String> = [
            ("Edge.Cuts", "Multi-Layer"),
            ("F.Mask", "Top Solder Mask Layer"),
            ("B.Mask", "Bottom Solder Mask Layer"),
            ("F.SilkS", "Top Silkscreen Layer"),
            ("F.Paste", "Top Paste Mask Layer"),
            ("Cmts.User", "User.Comments"),
        ]
        .into_iter()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect();

        let d = |f: &str| det.detect_layer_type_with_names(f, &user, &renames);

        assert_eq!(d("Interface-Multi-Layer.gbr"), Some(LayerType::MechanicalOutline));
        assert_eq!(
            d("Interface-Top Solder Mask Layer.gbr"),
            Some(LayerType::Soldermask(Side::Top))
        );
        assert_eq!(
            d("Interface-Bottom Solder Mask Layer.gbr"),
            Some(LayerType::Soldermask(Side::Bottom))
        );
        assert_eq!(
            d("Interface-Top Silkscreen Layer.gbr"),
            Some(LayerType::Silkscreen(Side::Top))
        );
        assert_eq!(d("Interface-Top Paste Mask Layer.gbr"), Some(LayerType::Paste(Side::Top)));
        // kicad-cli sanitizes dots to underscores: "User.Comments" →
        // `-User_Comments.gbr`. (Also matches the UserComments regex, but
        // must keep matching if that pattern ever narrows.)
        assert_eq!(d("Interface-User_Comments.gbr"), Some(LayerType::UserComments));
        // User.N slot names still bind through the same pass.
        assert_eq!(d("Interface-Ratline Layer.gbr"), Some(LayerType::UserLayer(1)));
        assert_eq!(d("Interface-3D Shell Outline Layer.gbr"), Some(LayerType::UserLayer(4)));
        // Un-renamed canonical filenames keep working without any maps.
        assert_eq!(
            det.detect_layer_type("Board-Edge_Cuts.gbr"),
            Some(LayerType::MechanicalOutline)
        );
    }
}
