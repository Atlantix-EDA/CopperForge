/// STEP file exporter for 3D PCB meshes
/// 
/// Exports the generated 3D meshes to STEP format for viewing in FreeCAD, KiCad, etc.

use crate::ecs::Mesh3D;
use std::fs::File;
use std::io::{Write, Result};
use std::path::Path;

#[derive(Debug, Clone)]
struct BoundingBox {
    min_x: f32,
    min_y: f32,
    min_z: f32,
    max_x: f32,
    max_y: f32,
    max_z: f32,
}

pub struct StepExporter;

impl StepExporter {
    /// Export meshes to STEP file (simple version that definitely works)
    pub fn export_meshes(meshes: &[Mesh3D], file_path: &Path) -> Result<()> {
        let mut file = File::create(file_path)?;
        
        // Write minimal STEP header
        writeln!(file, "ISO-10303-21;")?;
        writeln!(file, "HEADER;")?;
        writeln!(file, "FILE_DESCRIPTION(('PCB 3D Model'),'2;1');")?;
        writeln!(file, "FILE_NAME('{}','2024-01-01T00:00:00',('KiForge'),('User'),'','','');" , file_path.file_name().unwrap_or_default().to_string_lossy())?;
        writeln!(file, "FILE_SCHEMA(('CONFIG_CONTROL_DESIGN'));")?;
        writeln!(file, "ENDSEC;")?;
        writeln!(file, "DATA;")?;
        
        if meshes.is_empty() {
            // Create a simple test cube if no meshes
            Self::write_test_cube(&mut file)?;
        } else {
            // Group meshes by material_id (layer type) and create one box per layer
            println!("DEBUG: Processing {} total meshes", meshes.len());
            let layer_boxes = Self::calculate_layer_bounding_boxes(meshes);
            
            for (layer_idx, (material_id, bbox)) in layer_boxes.iter().enumerate() {
                Self::write_layer_box(&mut file, *material_id, bbox, layer_idx)?;
            }
        }
        
        // Write STEP footer
        writeln!(file, "ENDSEC;")?;
        writeln!(file, "END-ISO-10303-21;")?;
        
        println!("DEBUG: STEP file written with {} bytes", file.metadata().map(|m| m.len()).unwrap_or(0));
        Ok(())
    }
    
    /// Write a simple test cube (1x1x1 at origin)
    fn write_test_cube(file: &mut File) -> Result<()> {
        writeln!(file, "#1 = CARTESIAN_POINT('',(0.,0.,0.));")?;
        writeln!(file, "#2 = DIRECTION('',(0.,0.,1.));")?;
        writeln!(file, "#3 = DIRECTION('',(1.,0.,0.));")?;
        writeln!(file, "#4 = AXIS2_PLACEMENT_3D('',#1,#2,#3);")?;
        writeln!(file, "#5 = BLOCK('TestCube',#4,10.,10.,1.);")?;
        writeln!(file, "#6 = MANIFOLD_SOLID_BREP('TestCube',#5);")?;
        Ok(())
    }
    
    /// Write a simple box for a mesh
    fn write_simple_box(file: &mut File, mesh: &Mesh3D, mesh_idx: usize) -> Result<()> {
        // Calculate bounding box
        let mut min_x = f32::INFINITY;
        let mut min_y = f32::INFINITY; 
        let mut min_z = f32::INFINITY;
        let mut max_x = f32::NEG_INFINITY;
        let mut max_y = f32::NEG_INFINITY;
        let mut max_z = f32::NEG_INFINITY;
        
        for vertex in &mesh.vertices {
            min_x = min_x.min(vertex.x);
            min_y = min_y.min(vertex.y);
            min_z = min_z.min(vertex.z);
            max_x = max_x.max(vertex.x);
            max_y = max_y.max(vertex.y);
            max_z = max_z.max(vertex.z);
        }
        
        // Ensure minimum size and convert to mm
        let width = ((max_x - min_x) * 10.0).max(0.1);  // Convert to mm, min 0.1mm
        let height = ((max_y - min_y) * 10.0).max(0.1);
        let depth = ((max_z - min_z) * 10.0).max(0.1);
        
        let center_x = (min_x + max_x) * 0.5 * 10.0; // Convert to mm
        let center_y = (min_y + max_y) * 0.5 * 10.0;
        let center_z = (min_z + max_z) * 0.5 * 10.0;
        
        let base_id = mesh_idx * 10 + 1;
        
        writeln!(file, "#{} = CARTESIAN_POINT('',({:.3},{:.3},{:.3}));", base_id, center_x, center_y, center_z)?;
        writeln!(file, "#{} = DIRECTION('',(0.,0.,1.));", base_id + 1)?;
        writeln!(file, "#{} = DIRECTION('',(1.,0.,0.));", base_id + 2)?;
        writeln!(file, "#{} = AXIS2_PLACEMENT_3D('',#{},#{},#{});", base_id + 3, base_id, base_id + 1, base_id + 2)?;
        writeln!(file, "#{} = BLOCK('Layer_{}',#{},{:.3},{:.3},{:.3});", base_id + 4, mesh_idx, base_id + 3, width, height, depth)?;
        writeln!(file, "#{} = MANIFOLD_SOLID_BREP('Layer_{}_Solid',#{});", base_id + 5, mesh_idx, base_id + 4)?;
        
        println!("DEBUG: Wrote box for mesh {}: {}x{}x{} at ({},{},{})", 
            mesh_idx, width, height, depth, center_x, center_y, center_z);
        
        Ok(())
    }
    
    /// Calculate bounding box for each layer (grouped by material_id)
    fn calculate_layer_bounding_boxes(meshes: &[Mesh3D]) -> Vec<(u32, BoundingBox)> {
        use std::collections::HashMap;
        
        let mut layer_bounds: HashMap<u32, BoundingBox> = HashMap::new();
        
        for mesh in meshes {
            if mesh.vertices.is_empty() {
                continue;
            }
            
            // Calculate mesh bounding box
            let mut min_x = f32::INFINITY;
            let mut min_y = f32::INFINITY;
            let mut min_z = f32::INFINITY;
            let mut max_x = f32::NEG_INFINITY;
            let mut max_y = f32::NEG_INFINITY;
            let mut max_z = f32::NEG_INFINITY;
            
            for vertex in &mesh.vertices {
                min_x = min_x.min(vertex.x);
                min_y = min_y.min(vertex.y);
                min_z = min_z.min(vertex.z);
                max_x = max_x.max(vertex.x);
                max_y = max_y.max(vertex.y);
                max_z = max_z.max(vertex.z);
            }
            
            // Use material_id or default to 0 for unknown materials
            let material_id = mesh.material_id.unwrap_or(0);
            
            // Expand layer bounding box
            let layer_bbox = layer_bounds.entry(material_id).or_insert(BoundingBox {
                min_x: f32::INFINITY,
                min_y: f32::INFINITY,
                min_z: f32::INFINITY,
                max_x: f32::NEG_INFINITY,
                max_y: f32::NEG_INFINITY,
                max_z: f32::NEG_INFINITY,
            });
            
            layer_bbox.min_x = layer_bbox.min_x.min(min_x);
            layer_bbox.min_y = layer_bbox.min_y.min(min_y);
            layer_bbox.min_z = layer_bbox.min_z.min(min_z);
            layer_bbox.max_x = layer_bbox.max_x.max(max_x);
            layer_bbox.max_y = layer_bbox.max_y.max(max_y);
            layer_bbox.max_z = layer_bbox.max_z.max(max_z);
        }
        
        let result: Vec<_> = layer_bounds.into_iter().collect();
        println!("DEBUG: Grouped {} meshes into {} layers", meshes.len(), result.len());
        result
    }
    
    /// Write a box for an entire layer
    fn write_layer_box(file: &mut File, material_id: u32, bbox: &BoundingBox, layer_idx: usize) -> Result<()> {
        // Convert to mm and ensure minimum size
        let width = ((bbox.max_x - bbox.min_x) * 10.0).max(0.1);
        let height = ((bbox.max_y - bbox.min_y) * 10.0).max(0.1);
        let depth = ((bbox.max_z - bbox.min_z) * 10.0).max(0.1);
        
        let center_x = (bbox.min_x + bbox.max_x) * 0.5 * 10.0;
        let center_y = (bbox.min_y + bbox.max_y) * 0.5 * 10.0;
        let center_z = (bbox.min_z + bbox.max_z) * 0.5 * 10.0;
        
        let layer_name = match material_id {
            1 => "Copper",
            2 => "Soldermask", 
            3 => "Silkscreen",
            4 => "SolderPaste",
            5 => "Substrate",
            _ => "Unknown",
        };
        
        let base_id = layer_idx * 10 + 1;
        
        writeln!(file, "#{} = CARTESIAN_POINT('',({:.3},{:.3},{:.3}));", base_id, center_x, center_y, center_z)?;
        writeln!(file, "#{} = DIRECTION('',(0.,0.,1.));", base_id + 1)?;
        writeln!(file, "#{} = DIRECTION('',(1.,0.,0.));", base_id + 2)?;
        writeln!(file, "#{} = AXIS2_PLACEMENT_3D('',#{},#{},#{});", base_id + 3, base_id, base_id + 1, base_id + 2)?;
        writeln!(file, "#{} = BLOCK('{}',#{},{:.3},{:.3},{:.3});", base_id + 4, layer_name, base_id + 3, width, height, depth)?;
        writeln!(file, "#{} = MANIFOLD_SOLID_BREP('{}_Solid',#{});", base_id + 5, layer_name, base_id + 4)?;
        
        println!("DEBUG: Wrote {} layer: {}x{}x{} mm at ({},{},{}) mm", 
            layer_name, width, height, depth, center_x, center_y, center_z);
        
        Ok(())
    }
    
    /// Write STEP header entities (application context, etc.)
    fn write_header_entities(file: &mut File, mut entity_id: usize) -> Result<usize> {
        // Application context
        writeln!(file, "#{} = APPLICATION_CONTEXT('automotive_design');" , entity_id)?;
        let app_context_id = entity_id;
        entity_id += 1;
        
        // Application protocol definition
        writeln!(file, "#{} = APPLICATION_PROTOCOL_DEFINITION('international standard', 'automotive_design', 2009, #{});" , entity_id, app_context_id)?;
        entity_id += 1;
        
        // Product definition context
        writeln!(file, "#{} = PRODUCT_DEFINITION_CONTEXT('part definition', #{}, 'design');" , entity_id, app_context_id)?;
        let prod_def_context_id = entity_id;
        entity_id += 1;
        
        // Product
        writeln!(file, "#{} = PRODUCT('PCB_3D_Model', 'PCB 3D Model from KiForge', '', (#{}) );" , entity_id, app_context_id)?;
        let product_id = entity_id;
        entity_id += 1;
        
        // Product definition formation
        writeln!(file, "#{} = PRODUCT_DEFINITION_FORMATION('', '', #{});" , entity_id, product_id)?;
        let prod_def_form_id = entity_id;
        entity_id += 1;
        
        // Product definition
        writeln!(file, "#{} = PRODUCT_DEFINITION('design', '', #{}, #{});" , entity_id, prod_def_form_id, prod_def_context_id)?;
        let _prod_def_id = entity_id;
        entity_id += 1;
        
        Ok(entity_id)
    }
    
    /// Write a mesh as a STEP solid
    fn write_mesh_as_solid(file: &mut File, mesh: &Mesh3D, mesh_idx: usize, mut entity_id: usize) -> Result<usize> {
        // Create coordinate system
        let origin_id = entity_id;
        writeln!(file, "#{} = CARTESIAN_POINT('', (0.0, 0.0, 0.0));", entity_id)?;
        entity_id += 1;
        
        let dir_x_id = entity_id;
        writeln!(file, "#{} = DIRECTION('', (1.0, 0.0, 0.0));", entity_id)?;
        entity_id += 1;
        
        let dir_z_id = entity_id;
        writeln!(file, "#{} = DIRECTION('', (0.0, 0.0, 1.0));", entity_id)?;
        entity_id += 1;
        
        let axis_id = entity_id;
        writeln!(file, "#{} = AXIS2_PLACEMENT_3D('', #{}, #{}, #{});", entity_id, origin_id, dir_z_id, dir_x_id)?;
        entity_id += 1;
        
        // Write vertices as Cartesian points
        let mut vertex_ids = Vec::new();
        for vertex in &mesh.vertices {
            writeln!(file, "#{} = CARTESIAN_POINT('', ({:.6}, {:.6}, {:.6}));", 
                entity_id, vertex.x, vertex.y, vertex.z)?;
            vertex_ids.push(entity_id);
            entity_id += 1;
        }
        
        // Create a simple box or extrusion for the mesh
        // For now, we'll create a simplified representation
        entity_id = Self::write_simplified_solid(file, mesh, &vertex_ids, mesh_idx, entity_id)?;
        
        Ok(entity_id)
    }
    
    /// Write a simplified solid representation (box bounding the mesh)
    fn write_simplified_solid(file: &mut File, mesh: &Mesh3D, _vertex_ids: &[usize], mesh_idx: usize, mut entity_id: usize) -> Result<usize> {
        // Calculate bounding box
        let mut min_x = f32::INFINITY;
        let mut min_y = f32::INFINITY;
        let mut min_z = f32::INFINITY;
        let mut max_x = f32::NEG_INFINITY;
        let mut max_y = f32::NEG_INFINITY;
        let mut max_z = f32::NEG_INFINITY;
        
        for vertex in &mesh.vertices {
            min_x = min_x.min(vertex.x);
            min_y = min_y.min(vertex.y);
            min_z = min_z.min(vertex.z);
            max_x = max_x.max(vertex.x);
            max_y = max_y.max(vertex.y);
            max_z = max_z.max(vertex.z);
        }
        
        let width = max_x - min_x;
        let height = max_y - min_y;
        let depth = max_z - min_z;
        
        if width <= 0.0 || height <= 0.0 || depth <= 0.0 {
            return Ok(entity_id);
        }
        
        // Create bounding box solid
        let center_x = (min_x + max_x) * 0.5;
        let center_y = (min_y + max_y) * 0.5;
        let center_z = (min_z + max_z) * 0.5;
        
        // Box center point
        let box_center_id = entity_id;
        writeln!(file, "#{} = CARTESIAN_POINT('', ({:.6}, {:.6}, {:.6}));", 
            entity_id, center_x, center_y, center_z)?;
        entity_id += 1;
        
        // Box directions
        let box_dir_x_id = entity_id;
        writeln!(file, "#{} = DIRECTION('', (1.0, 0.0, 0.0));", entity_id)?;
        entity_id += 1;
        
        let box_dir_z_id = entity_id;
        writeln!(file, "#{} = DIRECTION('', (0.0, 0.0, 1.0));", entity_id)?;
        entity_id += 1;
        
        // Box axis placement
        let box_axis_id = entity_id;
        writeln!(file, "#{} = AXIS2_PLACEMENT_3D('', #{}, #{}, #{});", 
            entity_id, box_center_id, box_dir_z_id, box_dir_x_id)?;
        entity_id += 1;
        
        // Block solid
        let block_id = entity_id;
        writeln!(file, "#{} = BLOCK('Layer_{}', #{}, {:.6}, {:.6}, {:.6});", 
            entity_id, mesh_idx, box_axis_id, width, height, depth)?;
        entity_id += 1;
        
        // Manifold solid brep
        let brep_id = entity_id;
        writeln!(file, "#{} = MANIFOLD_SOLID_BREP('Layer_{}_Solid', #{});", 
            entity_id, mesh_idx, block_id)?;
        entity_id += 1;
        
        Ok(entity_id)
    }
}

/// Simple STEP export function for quick testing
pub fn export_meshes_to_step(meshes: &[Mesh3D], file_path: &str) -> std::io::Result<()> {
    let path = Path::new(file_path);
    StepExporter::export_meshes(meshes, path)
}