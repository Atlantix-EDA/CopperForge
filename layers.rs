use eframe::epaint::Color32;
use gerber_viewer::GerberLayer;

/// Represents different PCB layers
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LayerType {
    TopCopper,
    BottomCopper,
    TopSilk,
    BottomSilk,
    TopSoldermask,
    BottomSoldermask,
    MechanicalOutline,
}

impl LayerType {
    pub fn all() -> Vec<Self> {
        vec![
            Self::TopCopper,
            Self::BottomCopper,
            Self::TopSilk,
            Self::BottomSilk,
            Self::TopSoldermask,
            Self::BottomSoldermask,
            Self::MechanicalOutline,
        ]
    }
    
    pub fn display_name(&self) -> &'static str {
        match self {
            Self::TopCopper => "Top Copper",
            Self::BottomCopper => "Bottom Copper",
            Self::TopSilk => "Top Silk",
            Self::BottomSilk => "Bottom Silk",
            Self::TopSoldermask => "Top Soldermask",
            Self::BottomSoldermask => "Bottom Soldermask",
            Self::MechanicalOutline => "Mechanical Outline",
        }
    }
    
    pub fn color(&self) -> Color32 {
        match self {
            Self::TopCopper => Color32::from_rgba_premultiplied(184, 115, 51, 220),      // Copper with transparency
            Self::BottomCopper => Color32::from_rgba_premultiplied(115, 184, 51, 220),   // Different copper color for bottom
            Self::TopSilk => Color32::from_rgba_premultiplied(255, 255, 255, 250),       // White silk
            Self::BottomSilk => Color32::from_rgba_premultiplied(255, 255, 255, 250),    // White silk
            Self::TopSoldermask => Color32::from_rgba_premultiplied(0, 132, 80, 180),    // Green with transparency
            Self::BottomSoldermask => Color32::from_rgba_premultiplied(0, 80, 132, 180), // Blue for bottom soldermask
            Self::MechanicalOutline => Color32::from_rgba_premultiplied(255, 255, 0, 250), // Yellow outline
        }
    }
    
    pub fn filename(&self) -> &'static str {
        match self {
            Self::TopCopper => "cmod_s7-F_Cu.gbr",
            Self::BottomCopper => "cmod_s7-B_Cu.gbr",
            Self::TopSilk => "cmod_s7-F_SilkS.gbr",
            Self::BottomSilk => "cmod_s7-B_SilkS.gbr",
            Self::TopSoldermask => "cmod_s7-F_Mask.gbr",
            Self::BottomSoldermask => "cmod_s7-B_Mask.gbr",
            Self::MechanicalOutline => "cmod_s7-Edge_Cuts.gbr",
        }
    }
    
    pub fn should_render(&self, showing_top: bool) -> bool {
        match self {
            Self::TopCopper | Self::TopSilk | Self::TopSoldermask => showing_top,
            Self::BottomCopper | Self::BottomSilk | Self::BottomSoldermask => !showing_top,
            Self::MechanicalOutline => true, // Always show outline
        }
    }
}

/// Layer information including the gerber data and visibility
pub struct LayerInfo {
    pub layer_type: LayerType,
    pub gerber_layer: Option<GerberLayer>,
    pub visible: bool,
}

impl LayerInfo {
    pub fn new(layer_type: LayerType, gerber_layer: Option<GerberLayer>, visible: bool) -> Self {
        Self {
            layer_type,
            gerber_layer,
            visible,
        }
    }
}