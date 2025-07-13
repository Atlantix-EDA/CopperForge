#[cfg(test)]
mod tests {
    use crate::kicad::*;
    use crate::kicad::simple_parser::parse_layers_only;
    use crate::kicad::error::KicadError;
    use std::collections::HashMap;

    // Test data for minimal valid KiCad PCB file
    const MINIMAL_PCB: &str = r#"(kicad_pcb
  (version "20240108")
  (generator "pcbnew")
  (layers
    (0 "F.Cu" signal)
    (31 "B.Cu" signal)
    (32 "B.Adhes" user "B.Adhesive")
  )
)"#;

    // Test data with tracks
    const PCB_WITH_TRACKS: &str = r#"(kicad_pcb
  (version "20240108")
  (generator "pcbnew")
  (layers
    (0 "F.Cu" signal)
    (31 "B.Cu" signal)
  )
  (segment (start 100.0 50.0) (end 150.0 50.0) (width 0.25) (layer "F.Cu") (net 1 "GND"))
  (segment (start 150.0 50.0) (end 200.0 100.0) (width 0.15) (layer "B.Cu") (net 2 "VCC"))
)"#;

    // Test data with footprints and pads
    const PCB_WITH_FOOTPRINTS: &str = r#"(kicad_pcb
  (version "20240108")
  (generator "pcbnew")
  (layers
    (0 "F.Cu" signal)
    (31 "B.Cu" signal)
  )
  (footprint "Resistor_SMD:R_0805_2012Metric"
    (layer "F.Cu")
    (uuid "12345678-1234-1234-1234-123456789abc")
    (at 100.0 50.0 0)
    (pad "1" smd rect (at -1.0 0) (size 1.2 1.4) (layers "F.Cu" "F.Paste" "F.Mask") (net 1 "GND"))
    (pad "2" smd rect (at 1.0 0) (size 1.2 1.4) (layers "F.Cu" "F.Paste" "F.Mask") (net 2 "VCC"))
  )
)"#;

    // Invalid PCB data for error testing
    const INVALID_PCB_SYNTAX: &str = r#"(kicad_pcb
  (version "20240108"
  (generator "pcbnew")
  (layers
    (0 "F.Cu" signal)
  )
)"#; // Missing closing paren for version

    const INVALID_PCB_STRUCTURE: &str = r#"(not_a_pcb
  (version "20240108")
  (generator "pcbnew")
)"#;

    // Edge case test data
    const PCB_WITH_NUMBERS: &str = r#"(kicad_pcb
  (version 20240108)
  (generator "pcbnew")
  (layers
    (0 "F.Cu" signal)
  )
)"#;

    const PCB_EMPTY_LAYERS: &str = r#"(kicad_pcb
  (version "20240108")
  (generator "pcbnew")
  (layers
  )
)"#;

    #[test]
    fn test_simple_parser_minimal_pcb() {
        let result = parse_layers_only(MINIMAL_PCB);
        assert!(result.is_ok());
        
        let pcb = result.unwrap();
        assert_eq!(pcb.version, "unknown");
        assert_eq!(pcb.generator, "simple_parser");
        assert_eq!(pcb.layers.len(), 3);
        
        // Check specific layers
        assert!(pcb.layers.contains_key(&0));
        assert!(pcb.layers.contains_key(&31));
        assert!(pcb.layers.contains_key(&32));
        
        let f_cu = pcb.layers.get(&0).unwrap();
        assert_eq!(f_cu.name, "F.Cu");
        assert_eq!(f_cu.layer_type, "signal");
        assert_eq!(f_cu.user_name, None);
        
        let b_adhes = pcb.layers.get(&32).unwrap();
        assert_eq!(b_adhes.name, "B.Adhes");
        assert_eq!(b_adhes.layer_type, "user");
        assert_eq!(b_adhes.user_name, Some("B.Adhesive".to_string()));
    }

    #[test]
    fn test_simple_parser_empty_content() {
        let result = parse_layers_only("");
        assert!(result.is_ok());
        
        let pcb = result.unwrap();
        assert_eq!(pcb.layers.len(), 0);
    }

    #[test]
    fn test_simple_parser_no_layers_section() {
        let content = r#"(kicad_pcb
  (version "20240108")
  (generator "pcbnew")
)"#;
        
        let result = parse_layers_only(content);
        assert!(result.is_ok());
        
        let pcb = result.unwrap();
        assert_eq!(pcb.layers.len(), 0);
    }

    #[test]
    fn test_point_creation() {
        let point = Point { x: 10.5, y: -20.3 };
        assert_eq!(point.x, 10.5);
        assert_eq!(point.y, -20.3);
    }

    #[test]
    fn test_layer_creation() {
        let layer = Layer {
            id: 0,
            name: "F.Cu".to_string(),
            layer_type: "signal".to_string(),
            user_name: None,
        };
        
        assert_eq!(layer.id, 0);
        assert_eq!(layer.name, "F.Cu");
        assert_eq!(layer.layer_type, "signal");
        assert_eq!(layer.user_name, None);
    }

    #[test]
    fn test_pcb_file_new() {
        let pcb = PcbFile::new();
        
        assert_eq!(pcb.version, "");
        assert_eq!(pcb.generator, "");
        assert_eq!(pcb.board_thickness, None);
        assert_eq!(pcb.paper_size, None);
        assert_eq!(pcb.layers.len(), 0);
        assert_eq!(pcb.footprints.len(), 0);
        assert_eq!(pcb.tracks.len(), 0);
        assert_eq!(pcb.vias.len(), 0);
        assert_eq!(pcb.zones.len(), 0);
        assert_eq!(pcb.texts.len(), 0);
        assert_eq!(pcb.graphics.len(), 0);
    }

    #[test]
    fn test_pcb_file_layer_filtering() {
        let mut pcb = PcbFile::new();
        
        // Add some test tracks
        pcb.tracks.push(Track {
            start: Point { x: 0.0, y: 0.0 },
            end: Point { x: 10.0, y: 10.0 },
            width: 0.25,
            layer: "F.Cu".to_string(),
            net: None,
        });
        
        pcb.tracks.push(Track {
            start: Point { x: 10.0, y: 10.0 },
            end: Point { x: 20.0, y: 20.0 },
            width: 0.15,
            layer: "B.Cu".to_string(),
            net: None,
        });
        
        pcb.tracks.push(Track {
            start: Point { x: 20.0, y: 20.0 },
            end: Point { x: 30.0, y: 30.0 },
            width: 0.2,
            layer: "F.Cu".to_string(),
            net: None,
        });
        
        let f_cu_tracks = pcb.get_tracks_on_layer("F.Cu");
        assert_eq!(f_cu_tracks.len(), 2);
        
        let b_cu_tracks = pcb.get_tracks_on_layer("B.Cu");
        assert_eq!(b_cu_tracks.len(), 1);
        
        let nonexistent_tracks = pcb.get_tracks_on_layer("NonExistent");
        assert_eq!(nonexistent_tracks.len(), 0);
    }

    #[test]
    fn test_pcb_file_footprint_filtering() {
        let mut pcb = PcbFile::new();
        
        // Add some test footprints
        pcb.footprints.push(Footprint {
            name: "Resistor".to_string(),
            uuid: "uuid1".to_string(),
            position: Point { x: 0.0, y: 0.0 },
            rotation: 0.0,
            layer: "F.Cu".to_string(),
            locked: false,
            placed: true,
            properties: HashMap::new(),
            pads: Vec::new(),
            graphics: Vec::new(),
            texts: Vec::new(),
        });
        
        pcb.footprints.push(Footprint {
            name: "Capacitor".to_string(),
            uuid: "uuid2".to_string(),
            position: Point { x: 10.0, y: 10.0 },
            rotation: 90.0,
            layer: "B.Cu".to_string(),
            locked: false,
            placed: true,
            properties: HashMap::new(),
            pads: Vec::new(),
            graphics: Vec::new(),
            texts: Vec::new(),
        });
        
        let f_cu_footprints = pcb.get_footprints_on_layer("F.Cu");
        assert_eq!(f_cu_footprints.len(), 1);
        assert_eq!(f_cu_footprints[0].name, "Resistor");
        
        let b_cu_footprints = pcb.get_footprints_on_layer("B.Cu");
        assert_eq!(b_cu_footprints.len(), 1);
        assert_eq!(b_cu_footprints[0].name, "Capacitor");
    }

    #[test]
    fn test_error_display() {
        let io_error = std::io::Error::new(std::io::ErrorKind::NotFound, "File not found");
        let kicad_error = KicadError::from(io_error);
        assert!(format!("{}", kicad_error).contains("IO error"));
        
        let parse_error = KicadError::ParseError("Invalid syntax".to_string());
        assert!(format!("{}", parse_error).contains("Parse error"));
        
        let format_error = KicadError::InvalidFormat("Bad format".to_string());
        assert!(format!("{}", format_error).contains("Invalid format"));
        
        let missing_error = KicadError::MissingField("layer".to_string());
        assert!(format!("{}", missing_error).contains("Missing field"));
        
        let token_error = KicadError::UnexpectedToken("(unknown)".to_string());
        assert!(format!("{}", token_error).contains("Unexpected token"));
    }

    // Test edge cases and malformed input
    #[test]
    fn test_malformed_layer_line() {
        // Test simple parser with malformed layer line
        let malformed = r#"(kicad_pcb
  (layers
    (0 "F.Cu" signal)
    (not_a_number "Invalid" signal)
    (31 "B.Cu" signal)
  )
)"#;
        
        let result = parse_layers_only(malformed);
        assert!(result.is_ok());
        
        let pcb = result.unwrap();
        // Should parse 2 valid layers, skip the malformed one
        assert_eq!(pcb.layers.len(), 2);
        assert!(pcb.layers.contains_key(&0));
        assert!(pcb.layers.contains_key(&31));
    }

    #[test]
    fn test_empty_pad_layers() {
        let pcb_with_empty_pad = r#"(kicad_pcb
  (version "20240108")
  (generator "pcbnew")
  (layers
    (0 "F.Cu" signal)
  )
  (footprint "Test"
    (layer "F.Cu")
    (at 0 0)
    (pad "1" smd rect (at 0 0) (size 1.0 1.0) (layers))
  )
)"#;
        
        // Simple parser only parses layers, so this should work
        let result = parse_layers_only(pcb_with_empty_pad);
        assert!(result.is_ok());
        
        let pcb = result.unwrap();
        assert_eq!(pcb.layers.len(), 1);
    }

    #[test]
    fn test_track_without_net() {
        let pcb_with_netless_track = r#"(kicad_pcb
  (version "20240108")
  (generator "pcbnew")
  (layers
    (0 "F.Cu" signal)
  )
  (segment (start 0.0 0.0) (end 10.0 10.0) (width 0.25) (layer "F.Cu"))
)"#;
        
        // Simple parser only parses layers, so this should work
        let result = parse_layers_only(pcb_with_netless_track);
        assert!(result.is_ok());
        
        let pcb = result.unwrap();
        assert_eq!(pcb.layers.len(), 1);
    }
}