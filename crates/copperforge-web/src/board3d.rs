//! Browser-side 3D geometry builder.
//!
//! Bridges the in-memory uploaded gerbers (`<basename, bytes>`) to the
//! geometry IR that `copperforge_core::panels::Board3dView` renders. The
//! native app extracts the same IR straight from files on disk; the browser
//! has no filesystem, so we use the `*_from_bytes` variants added to
//! `copperforge_core::gerber_geom`.
//!
//! Built once per upload (tessellation isn't free) and cached on `WebApp`;
//! `Board3dView` does its own per-frame change-detection, so handing it the
//! same cached references every frame is cheap.

use std::collections::BTreeMap;

use copperforge_core::gerber_geom::{
    extract_copper_from_bytes, extract_drill_excellon_from_bytes, extract_drill_gerber_from_bytes,
    extract_mask_from_bytes, extract_outline_from_bytes, CopperData, DrillData, MaskData,
    OutlineData,
};

use crate::canvas::model::LayerKind;

/// Tessellated meshes for the 3D board view, keyed by layer role. Every
/// field is optional — a board may upload only an outline, or copper with
/// no soldermask, etc. The outline is the keystone: copper and mask are
/// placed in the *outline's* world frame, so without it nothing else can
/// be positioned and they're all left `None`.
#[derive(Default)]
pub struct Board3dGeom {
    pub outline: Option<OutlineData>,
    pub top_copper: Option<CopperData>,
    pub bottom_copper: Option<CopperData>,
    pub top_mask: Option<MaskData>,
    pub bottom_mask: Option<MaskData>,
    pub drill: Option<DrillData>,
}

impl Board3dGeom {
    /// Classify each `.gbr` by filename and extract its mesh. Returns an
    /// all-`None` `Board3dGeom` when no parseable edge-cuts layer is found
    /// (the 3D tab then shows just the axes/grid until a board with an
    /// outline is loaded).
    pub fn build_from_entries(entries: &BTreeMap<String, Vec<u8>>) -> Self {
        // ── Outline first — it defines the world frame for everything ──
        let outline = entries
            .iter()
            .filter(|(name, _)| name.to_lowercase().ends_with(".gbr"))
            .find(|(name, _)| LayerKind::from_filename(name) == LayerKind::EdgeCuts)
            .and_then(|(_, bytes)| extract_outline_from_bytes(bytes))
            .map(|(data, _counts)| data);

        let Some(outline) = outline else {
            return Self::default();
        };

        // Copper + mask meshes share the outline's bbox (and the outline's
        // contours, for the mask's punch-out tessellation).
        let bbox = &outline.bbox;
        let contours = &outline.contours;

        let mut geom = Self {
            top_copper: None,
            bottom_copper: None,
            top_mask: None,
            bottom_mask: None,
            drill: None,
            outline: None,
        };

        for (name, bytes) in entries {
            if !name.to_lowercase().ends_with(".gbr") {
                continue;
            }
            match LayerKind::from_filename(name) {
                LayerKind::TopCopper => {
                    geom.top_copper =
                        extract_copper_from_bytes(bytes, bbox).map(|(d, _)| d);
                }
                LayerKind::BottomCopper => {
                    geom.bottom_copper =
                        extract_copper_from_bytes(bytes, bbox).map(|(d, _)| d);
                }
                LayerKind::TopMask => {
                    geom.top_mask =
                        extract_mask_from_bytes(bytes, contours, bbox).map(|(d, _)| d);
                }
                LayerKind::BottomMask => {
                    geom.bottom_mask =
                        extract_mask_from_bytes(bytes, contours, bbox).map(|(d, _)| d);
                }
                _ => {}
            }
        }

        // Drill holes. Sources may be split into PTH + NPTH files, in either
        // Excellon (.drl) or drill-as-gerber form — collect from every match
        // and merge. (LayerKind has no Drill variant, so match on filename.)
        let mut holes = DrillData::default();
        for (name, bytes) in entries {
            let l = name.to_lowercase();
            let found = if l.ends_with(".drl") {
                extract_drill_excellon_from_bytes(bytes, bbox)
            } else if l.ends_with(".gbr")
                && (l.contains("drill")
                    || l.contains("drl")
                    || l.contains("-pth")
                    || l.contains("-npth"))
            {
                extract_drill_gerber_from_bytes(bytes, bbox)
            } else {
                None
            };
            if let Some(mut d) = found {
                holes.holes.append(&mut d.holes);
            }
        }
        if !holes.is_empty() {
            geom.drill = Some(holes);
        }

        geom.outline = Some(outline);
        geom
    }
}
