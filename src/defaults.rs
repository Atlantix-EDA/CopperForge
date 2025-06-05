use std::io::BufReader;
use gerber_viewer::gerber_parser::parse;
use gerber_viewer::GerberLayer;
use crate::layers::{LayerType, LayerInfo};
use crate::managers::LayerManager;

/// Load default gerber layers from embedded assets
pub fn load_default_gerbers() -> LayerManager {
    let mut layer_manager = LayerManager::new();
    
    // Map layer types to their corresponding gerber files
    let layer_files = [
        (LayerType::TopCopper, "cmod_s7-F_Cu.gbr"),
        (LayerType::BottomCopper, "cmod_s7-B_Cu.gbr"),
        (LayerType::TopSilk, "cmod_s7-F_SilkS.gbr"),
        (LayerType::BottomSilk, "cmod_s7-B_SilkS.gbr"),
        (LayerType::TopSoldermask, "cmod_s7-F_Mask.gbr"),
        (LayerType::BottomSoldermask, "cmod_s7-B_Mask.gbr"),
        (LayerType::MechanicalOutline, "cmod_s7-Edge_Cuts.gbr"),
    ];
    
    // Load each layer's gerber file
    for (layer_type, filename) in layer_files {
        let gerber_data = get_embedded_gerber_data(filename);
        
        let reader = BufReader::new(gerber_data.as_bytes());
        let layer_gerber = match parse(reader) {
            Ok(doc) => {
                let commands = doc.into_commands();
                Some(GerberLayer::new(commands))
            }
            Err(e) => {
                eprintln!("Failed to parse {}: {:?}", filename, e);
                None
            }
        };
        
        let layer_info = LayerInfo::new(
            layer_type,
            layer_gerber,
            Some(gerber_data.to_string()),  // Store raw Gerber data for DRC
            matches!(layer_type, LayerType::TopCopper | LayerType::MechanicalOutline),
        );
        layer_manager.add_layer(layer_type, layer_info);
    }
    
    layer_manager
}

/// Get embedded gerber data by filename
fn get_embedded_gerber_data(filename: &str) -> &'static str {
    match filename {
        "cmod_s7-F_Cu.gbr" => include_str!("../assets/cmod_s7-F_Cu.gbr"),
        "cmod_s7-B_Cu.gbr" => include_str!("../assets/cmod_s7-B_Cu.gbr"),
        "cmod_s7-F_SilkS.gbr" => include_str!("../assets/cmod_s7-F_SilkS.gbr"),
        "cmod_s7-B_SilkS.gbr" => include_str!("../assets/cmod_s7-B_SilkS.gbr"),
        "cmod_s7-F_Mask.gbr" => include_str!("../assets/cmod_s7-F_Mask.gbr"),
        "cmod_s7-B_Mask.gbr" => include_str!("../assets/cmod_s7-B_Mask.gbr"),
        "cmod_s7-Edge_Cuts.gbr" => include_str!("../assets/cmod_s7-Edge_Cuts.gbr"),
        _ => include_str!("../assets/demo.gbr"), // Fallback
    }
}

/// Load the demo gerber for legacy compatibility
pub fn load_demo_gerber() -> GerberLayer {
    let demo_str = include_str!("../assets/demo.gbr").as_bytes();
    let reader = BufReader::new(demo_str);
    let doc = parse(reader).unwrap();
    let commands = doc.into_commands();
    GerberLayer::new(commands)
}