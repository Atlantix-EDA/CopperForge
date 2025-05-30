use super::types::*;
use super::error::Result;

/// Simple parser that just extracts layers from a KiCad PCB file
pub fn parse_layers_only(content: &str) -> Result<PcbFile> {
    let mut pcb = PcbFile::new();
    pcb.version = "unknown".to_string();
    pcb.generator = "simple_parser".to_string();
    
    // Find the layers section
    if let Some(layers_start) = content.find("(layers") {
        let layers_section = &content[layers_start..];
        
        // Extract each layer line - they look like: (0 "F.Cu" signal)
        let lines: Vec<&str> = layers_section.lines().collect();
        
        for line in lines {
            let line = line.trim();
            if line.starts_with('(') && line.contains('"') && !line.starts_with("(layers") {
                // Try to parse layer line
                if let Some(layer) = parse_layer_line(line) {
                    pcb.layers.insert(layer.id, layer);
                }
            } else if line.starts_with(')') && pcb.layers.len() > 0 {
                // End of layers section
                break;
            }
        }
    }
    
    eprintln!("Simple parser found {} layers", pcb.layers.len());
    Ok(pcb)
}

fn parse_layer_line(line: &str) -> Option<Layer> {
    // Remove parentheses
    let line = line.trim_start_matches('(').trim_end_matches(')');
    let parts: Vec<&str> = line.split_whitespace().collect();
    
    if parts.len() >= 3 {
        // Parse ID
        if let Ok(id) = parts[0].parse::<i32>() {
            // Parse name (remove quotes)
            let name = parts[1].trim_matches('"').to_string();
            
            // Parse type
            let layer_type = parts[2].trim_matches('"').to_string();
            
            // Optional user name
            let user_name = if parts.len() > 3 {
                Some(parts[3..].join(" ").trim_matches('"').to_string())
            } else {
                None
            };
            
            return Some(Layer {
                id,
                name,
                layer_type,
                user_name,
            });
        }
    }
    
    None
}