use nalgebra::{Point2, Vector2};
use crate::layer_operations::LayerType;
use gerber_viewer::GerberLayer;

/// PCB layer stackup configuration
pub struct LayerStackup {
    pub substrate_thickness: f32,  // FR4 thickness in mm
    pub copper_thickness: f32,     // Copper layer thickness in mm
    pub mask_thickness: f32,       // Soldermask thickness in mm
    pub silk_thickness: f32,       // Silkscreen thickness in mm
    pub paste_thickness: f32,      // Paste stencil thickness in mm
}

impl Default for LayerStackup {
    fn default() -> Self {
        Self {
            substrate_thickness: 1.6,    // Standard 1.6mm PCB
            copper_thickness: 0.035,     // 1oz copper (35μm)
            mask_thickness: 0.025,       // Typical soldermask
            silk_thickness: 0.010,       // Silkscreen layer
            paste_thickness: 0.005,      // Paste stencil
        }
    }
}

/// Represents a 3D mesh for a PCB layer
pub struct LayerMesh {
    pub vertices: Vec<[f32; 3]>,
    pub indices: Vec<u32>,
    pub color: [f32; 4],  // RGBA
    pub layer_type: LayerType,
}

/// Convert 2D gerber geometry to 3D mesh
pub struct GerberTo3D {
    stackup: LayerStackup,
}

impl GerberTo3D {
    pub fn new(stackup: LayerStackup) -> Self {
        Self { stackup }
    }
    
    /// Get the Z position and thickness for a layer type
    pub fn get_layer_z_info(&self, layer_type: LayerType) -> (f32, f32) {
        let half_substrate = self.stackup.substrate_thickness / 2.0;
        
        match layer_type {
            // Bottom layers (negative Z)
            LayerType::BottomSilk => (-half_substrate - self.stackup.mask_thickness - self.stackup.silk_thickness, self.stackup.silk_thickness),
            LayerType::BottomPaste => (-half_substrate - self.stackup.mask_thickness - self.stackup.paste_thickness, self.stackup.paste_thickness),
            LayerType::BottomSoldermask => (-half_substrate - self.stackup.mask_thickness, self.stackup.mask_thickness),
            LayerType::BottomCopper => (-half_substrate - self.stackup.copper_thickness, self.stackup.copper_thickness),
            
            // Top layers (positive Z)
            LayerType::TopCopper => (half_substrate, self.stackup.copper_thickness),
            LayerType::TopSoldermask => (half_substrate + self.stackup.copper_thickness, self.stackup.mask_thickness),
            LayerType::TopSilk => (half_substrate + self.stackup.copper_thickness + self.stackup.mask_thickness, self.stackup.silk_thickness),
            LayerType::TopPaste => (half_substrate + self.stackup.copper_thickness + self.stackup.mask_thickness, self.stackup.paste_thickness),
            
            // Mechanical outline (full thickness)
            LayerType::MechanicalOutline => (-half_substrate, self.stackup.substrate_thickness),
        }
    }
    
    /// Create substrate (FR4) mesh
    pub fn create_substrate_mesh(&self, bounds: &gerber_viewer::BoundingBox) -> LayerMesh {
        let min_x = bounds.min.x as f32;
        let min_y = bounds.min.y as f32;
        let max_x = bounds.max.x as f32;
        let max_y = bounds.max.y as f32;
        
        let half_thickness = self.stackup.substrate_thickness / 2.0;
        
        // Create a box for the substrate
        let vertices = vec![
            // Bottom face
            [min_x, min_y, -half_thickness],
            [max_x, min_y, -half_thickness],
            [max_x, max_y, -half_thickness],
            [min_x, max_y, -half_thickness],
            // Top face
            [min_x, min_y, half_thickness],
            [max_x, min_y, half_thickness],
            [max_x, max_y, half_thickness],
            [min_x, max_y, half_thickness],
        ];
        
        let indices = vec![
            // Bottom face
            0, 1, 2,  0, 2, 3,
            // Top face
            4, 6, 5,  4, 7, 6,
            // Front face
            0, 4, 5,  0, 5, 1,
            // Back face
            2, 6, 7,  2, 7, 3,
            // Left face
            0, 3, 7,  0, 7, 4,
            // Right face
            1, 5, 6,  1, 6, 2,
        ];
        
        LayerMesh {
            vertices,
            indices,
            color: [0.235, 0.353, 0.157, 1.0], // FR4 green
            layer_type: LayerType::MechanicalOutline,
        }
    }
    
    /// Convert gerber layer to 3D mesh
    pub fn extrude_layer(&self, gerber_layer: &GerberLayer, layer_type: LayerType) -> LayerMesh {
        let (z_start, thickness) = self.get_layer_z_info(layer_type);
        let mut vertices = Vec::new();
        let mut indices = Vec::new();
        
        // For now, create a simple rectangle based on bounding box
        // TODO: Implement actual gerber geometry conversion
        let bounds = gerber_layer.bounding_box();
        let min_x = bounds.min.x as f32;
        let min_y = bounds.min.y as f32;
        let max_x = bounds.max.x as f32;
        let max_y = bounds.max.y as f32;
        
        // Shrink slightly to show individual layers
        let inset = 0.5;
        let min_x = min_x + inset;
        let min_y = min_y + inset;
        let max_x = max_x - inset;
        let max_y = max_y - inset;
        
        // Create extruded rectangle
        self.create_box_mesh(
            min_x, min_y, max_x, max_y, 
            z_start, thickness,
            &mut vertices, &mut indices
        );
        
        LayerMesh {
            vertices,
            indices,
            color: self.get_layer_color(layer_type),
            layer_type,
        }
    }
    
    /// Create a box mesh
    fn create_box_mesh(
        &self,
        min_x: f32, min_y: f32, max_x: f32, max_y: f32,
        z_start: f32, thickness: f32,
        vertices: &mut Vec<[f32; 3]>,
        indices: &mut Vec<u32>
    ) {
        let z_end = z_start + thickness;
        let base_idx = vertices.len() as u32;
        
        // Add vertices
        vertices.extend_from_slice(&[
            // Bottom face
            [min_x, min_y, z_start],
            [max_x, min_y, z_start],
            [max_x, max_y, z_start],
            [min_x, max_y, z_start],
            // Top face
            [min_x, min_y, z_end],
            [max_x, min_y, z_end],
            [max_x, max_y, z_end],
            [min_x, max_y, z_end],
        ]);
        
        // Add indices
        indices.extend_from_slice(&[
            // Bottom face
            base_idx + 0, base_idx + 1, base_idx + 2,
            base_idx + 0, base_idx + 2, base_idx + 3,
            // Top face
            base_idx + 4, base_idx + 6, base_idx + 5,
            base_idx + 4, base_idx + 7, base_idx + 6,
            // Front face
            base_idx + 0, base_idx + 4, base_idx + 5,
            base_idx + 0, base_idx + 5, base_idx + 1,
            // Back face
            base_idx + 2, base_idx + 6, base_idx + 7,
            base_idx + 2, base_idx + 7, base_idx + 3,
            // Left face
            base_idx + 0, base_idx + 3, base_idx + 7,
            base_idx + 0, base_idx + 7, base_idx + 4,
            // Right face
            base_idx + 1, base_idx + 5, base_idx + 6,
            base_idx + 1, base_idx + 6, base_idx + 2,
        ]);
    }
    
    /// Get layer color in normalized RGBA
    fn get_layer_color(&self, layer_type: LayerType) -> [f32; 4] {
        let color32 = layer_type.color();
        [
            color32.r() as f32 / 255.0,
            color32.g() as f32 / 255.0,
            color32.b() as f32 / 255.0,
            color32.a() as f32 / 255.0,
        ]
    }
}