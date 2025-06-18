//! 3D PCB integration system for KiForge
//!
//! This module provides the main integration point for converting 2D Gerber layers
//! to 3D meshes and rendering them in the WGPU 3D viewer. It implements the core
//! algorithm: Gerber Layer → Iterate Primitives → Convert to Meshes → Render in WGPU

use nalgebra::{Point2, Point3};
use std::collections::HashMap;
use std::path::Path;
use gerber_viewer::GerberLayer;
use crate::layer_operations::{LayerType, LayerManager};
use crate::renderer::mesh3d::{Mesh3D, Polygon2D, ExtrusionEngine, ExtrusionError};
use crate::renderer::gerber_extrudable::{layer_to_3d_meshes, combine_meshes};
use crate::renderer::gerber_components::{
    GerberLayerComponent, StackupLayer, LayerTypeExt, MaterialProperties,
    Position3D, Transform3D, Renderable3D, LayerMesh
};

/// Complete 3D PCB system that integrates Gerber layers and 3D rendering
pub struct Pcb3DSystem {
    /// 3D mesh generation engine
    pub extrusion_engine: ExtrusionEngine,
    /// Generated 3D data cache
    pub layer_meshes: HashMap<LayerType, Mesh3D>,
    /// Layer mesh metadata
    pub layer_mesh_info: HashMap<LayerType, LayerMesh>,
    /// PCB stackup configuration
    pub stackup_layers: HashMap<LayerType, StackupLayer>,
}

impl Pcb3DSystem {
    pub fn new() -> Self {
        Self {
            extrusion_engine: ExtrusionEngine::new(),
            layer_meshes: HashMap::new(),
            layer_mesh_info: HashMap::new(),
            stackup_layers: HashMap::new(),
        }
    }

    /// Generate complete 3D PCB from layer manager
    /// This implements the core algorithm:
    /// 1) Ingest gerber layers from LayerManager
    /// 2) For each layer, iterate on all primitives and convert to meshes
    /// 3) Create Vec<Mesh3D> for each layer
    /// 4) Prepare for WGPU rendering
    pub fn generate_3d_pcb(
        &mut self,
        layer_manager: &LayerManager,
    ) -> Result<Pcb3DGenerationResult, Pcb3DError> {
        log::info!("Starting 3D PCB generation from {} layers...", layer_manager.layers.len());
        
        let mut result = Pcb3DGenerationResult::default();

        // Step 1: Create stackup information from available layers
        self.create_stackup_from_layers(layer_manager);

        // Step 2: Process each visible layer
        for (layer_type, layer_info) in &layer_manager.layers {
            if !layer_info.visible {
                log::debug!("Skipping invisible layer: {:?}", layer_type);
                continue;
            }

            if let Some(ref gerber_layer) = layer_info.gerber_layer {
                log::info!("Processing layer {:?} with {} primitives", layer_type, gerber_layer.primitives().len());
                
                // Get layer properties
                let layer_height = layer_type.default_thickness() as f32;
                let material_id = layer_type.material_id();
                
                // Convert layer to 3D meshes - this is the core algorithm step
                let layer_meshes = layer_to_3d_meshes(
                    gerber_layer,
                    layer_height,
                    material_id,
                    &mut self.extrusion_engine
                );

                if !layer_meshes.is_empty() {
                    // Combine all meshes for this layer into a single mesh for efficient rendering
                    let combined_mesh = combine_meshes(layer_meshes);
                    
                    // Calculate bounds
                    let bounds = combined_mesh.bounding_box();
                    
                    // Store mesh information
                    let stats = combined_mesh.stats();
                    let mesh_info = LayerMesh::new(
                        layer_type.clone(),
                        material_id,
                        stats.vertex_count,
                        stats.triangle_count,
                    );
                    
                    self.layer_meshes.insert(layer_type.clone(), combined_mesh);
                    self.layer_mesh_info.insert(layer_type.clone(), mesh_info);
                    
                    result.layers_processed += 1;
                    result.total_vertices += stats.vertex_count;
                    result.total_triangles += stats.triangle_count;
                    
                    log::info!("Layer {:?}: {} vertices, {} triangles", 
                              layer_type, stats.vertex_count, stats.triangle_count);
                } else {
                    log::warn!("No meshes generated for layer {:?}", layer_type);
                }
            } else {
                log::debug!("Layer {:?} has no Gerber data", layer_type);
            }
        }

        // Step 3: Calculate overall PCB bounds
        result.pcb_bounds = self.calculate_pcb_bounds();

        log::info!("3D PCB generation completed: {} layers, {} total vertices, {} total triangles", 
                  result.layers_processed, result.total_vertices, result.total_triangles);

        Ok(result)
    }

    /// Create stackup layer information from available layers
    fn create_stackup_from_layers(&mut self, layer_manager: &LayerManager) {
        self.stackup_layers.clear();
        let mut current_z = 0.0f64;

        // Define layer order from bottom to top
        let layer_order = [
            LayerType::BottomCopper,
            LayerType::BottomSoldermask,
            LayerType::BottomSilk,
            LayerType::BottomPaste,
            LayerType::TopPaste,
            LayerType::TopSilk,
            LayerType::TopSoldermask,
            LayerType::TopCopper,
        ];

        for (index, layer_type) in layer_order.iter().enumerate() {
            if layer_manager.layers.contains_key(layer_type) {
                let thickness = layer_type.default_thickness();
                let material = layer_type.default_material();
                
                let stackup = StackupLayer::new(
                    index as u32,
                    current_z,
                    thickness,
                    material,
                );

                self.stackup_layers.insert(layer_type.clone(), stackup);
                current_z += thickness;
                
                log::debug!("Stackup layer {:?}: z={:.3} to {:.3}, thickness={:.3}", 
                           layer_type, current_z - thickness, current_z, thickness);
            }
        }

        // Add mechanical outline (substrate) in the middle if present
        if layer_manager.layers.contains_key(&LayerType::MechanicalOutline) {
            let substrate_thickness = LayerType::MechanicalOutline.default_thickness();
            let stackup = StackupLayer::new(
                100, // Special index for substrate
                -substrate_thickness / 2.0,
                substrate_thickness,
                LayerType::MechanicalOutline.default_material(),
            );
            self.stackup_layers.insert(LayerType::MechanicalOutline, stackup);
            
            log::debug!("Substrate layer: z={:.3} to {:.3}, thickness={:.3}", 
                       -substrate_thickness / 2.0, substrate_thickness / 2.0, substrate_thickness);
        }
    }

    /// Calculate overall PCB bounding box from all layer meshes
    fn calculate_pcb_bounds(&self) -> Option<(Point3<f32>, Point3<f32>)> {
        if self.layer_meshes.is_empty() {
            return None;
        }

        let mut min_point = Point3::new(f32::INFINITY, f32::INFINITY, f32::INFINITY);
        let mut max_point = Point3::new(f32::NEG_INFINITY, f32::NEG_INFINITY, f32::NEG_INFINITY);
        let mut has_bounds = false;

        for mesh in self.layer_meshes.values() {
            if let Some((mesh_min, mesh_max)) = mesh.bounding_box() {
                min_point.x = min_point.x.min(mesh_min.x);
                min_point.y = min_point.y.min(mesh_min.y);
                min_point.z = min_point.z.min(mesh_min.z);
                
                max_point.x = max_point.x.max(mesh_max.x);
                max_point.y = max_point.y.max(mesh_max.y);
                max_point.z = max_point.z.max(mesh_max.z);
                
                has_bounds = true;
            }
        }

        if has_bounds {
            Some((min_point, max_point))
        } else {
            None
        }
    }

    /// Get mesh for a specific layer type
    pub fn get_layer_mesh(&self, layer_type: &LayerType) -> Option<&Mesh3D> {
        self.layer_meshes.get(layer_type)
    }

    /// Get all layer meshes for rendering
    pub fn get_all_meshes(&self) -> &HashMap<LayerType, Mesh3D> {
        &self.layer_meshes
    }

    /// Get layer mesh information
    pub fn get_layer_mesh_info(&self, layer_type: &LayerType) -> Option<&LayerMesh> {
        self.layer_mesh_info.get(layer_type)
    }

    /// Get stackup layer information
    pub fn get_stackup_layer(&self, layer_type: &LayerType) -> Option<&StackupLayer> {
        self.stackup_layers.get(layer_type)
    }

    /// Clear all cached meshes and regenerate
    pub fn clear_cache(&mut self) {
        self.layer_meshes.clear();
        self.layer_mesh_info.clear();
        self.extrusion_engine.clear_cache();
        log::info!("Cleared 3D PCB cache");
    }

    /// Get statistics about the generated 3D PCB
    pub fn get_statistics(&self) -> Pcb3DStatistics {
        let total_vertices: usize = self.layer_meshes.values()
            .map(|mesh| mesh.vertices.len())
            .sum();
        
        let total_triangles: usize = self.layer_meshes.values()
            .map(|mesh| mesh.indices.len() / 3)
            .sum();

        let (cached_meshes, cached_vertices) = self.extrusion_engine.cache_stats();

        Pcb3DStatistics {
            layers_processed: self.layer_meshes.len(),
            total_vertices,
            total_triangles,
            cached_primitives: cached_meshes,
            cached_vertices,
            bounds: self.calculate_pcb_bounds(),
        }
    }

    /// Convert layer meshes to format suitable for WGPU rendering
    pub fn prepare_for_wgpu_rendering(&self) -> Vec<WgpuMeshData> {
        let mut wgpu_meshes = Vec::new();

        for (layer_type, mesh) in &self.layer_meshes {
            // Convert to WGPU-compatible format
            let vertices: Vec<WgpuVertex> = mesh.vertices.iter().zip(mesh.normals.iter()).zip(mesh.uvs.iter())
                .map(|((vertex, normal), uv)| WgpuVertex {
                    position: [vertex.x, vertex.y, vertex.z],
                    normal: [normal.x, normal.y, normal.z],
                    uv: [uv.x, uv.y],
                    material_id: mesh.material_id.unwrap_or(0),
                })
                .collect();

            let mesh_data = WgpuMeshData {
                layer_type: layer_type.clone(),
                vertices,
                indices: mesh.indices.clone(),
                material_id: mesh.material_id.unwrap_or(0),
                visible: true,
            };

            wgpu_meshes.push(mesh_data);
        }

        log::info!("Prepared {} meshes for WGPU rendering", wgpu_meshes.len());
        wgpu_meshes
    }
}

/// Result of 3D PCB generation
#[derive(Debug, Default)]
pub struct Pcb3DGenerationResult {
    pub layers_processed: usize,
    pub total_vertices: usize,
    pub total_triangles: usize,
    pub pcb_bounds: Option<(Point3<f32>, Point3<f32>)>,
    pub warnings: Vec<String>,
}

/// 3D PCB statistics
#[derive(Debug)]
pub struct Pcb3DStatistics {
    pub layers_processed: usize,
    pub total_vertices: usize,
    pub total_triangles: usize,
    pub cached_primitives: usize,
    pub cached_vertices: usize,
    pub bounds: Option<(Point3<f32>, Point3<f32>)>,
}

/// WGPU-compatible vertex format
#[repr(C)]
#[derive(Copy, Clone, Debug)]
pub struct WgpuVertex {
    pub position: [f32; 3],
    pub normal: [f32; 3],
    pub uv: [f32; 2],
    pub material_id: u32,
}

/// WGPU mesh data ready for rendering
#[derive(Debug)]
pub struct WgpuMeshData {
    pub layer_type: LayerType,
    pub vertices: Vec<WgpuVertex>,
    pub indices: Vec<u32>,
    pub material_id: u32,
    pub visible: bool,
}

/// Errors that can occur during 3D PCB generation
#[derive(Debug, Clone)]
pub enum Pcb3DError {
    GerberExtractionError(String),
    MeshGenerationError(String),
    StackupError(String),
    RenderingError(String),
}

impl std::fmt::Display for Pcb3DError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Pcb3DError::GerberExtractionError(msg) => write!(f, "Gerber extraction error: {}", msg),
            Pcb3DError::MeshGenerationError(msg) => write!(f, "Mesh generation error: {}", msg),
            Pcb3DError::StackupError(msg) => write!(f, "Stackup error: {}", msg),
            Pcb3DError::RenderingError(msg) => write!(f, "Rendering error: {}", msg),
        }
    }
}

impl From<ExtrusionError> for Pcb3DError {
    fn from(err: ExtrusionError) -> Self {
        Pcb3DError::MeshGenerationError(err.to_string())
    }
}

impl std::error::Error for Pcb3DError {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layer_operations::{LayerInfo, LayerManager};

    #[test]
    fn test_pcb3d_system_creation() {
        let system = Pcb3DSystem::new();
        assert_eq!(system.layer_meshes.len(), 0);
        assert_eq!(system.layer_mesh_info.len(), 0);
    }

    #[test]
    fn test_empty_layer_manager() {
        let mut system = Pcb3DSystem::new();
        let layer_manager = LayerManager::new();
        
        let result = system.generate_3d_pcb(&layer_manager);
        assert!(result.is_ok());
        
        let result = result.unwrap();
        assert_eq!(result.layers_processed, 0);
        assert_eq!(result.total_vertices, 0);
    }

    #[test]
    fn test_statistics() {
        let system = Pcb3DSystem::new();
        let stats = system.get_statistics();
        
        assert_eq!(stats.layers_processed, 0);
        assert_eq!(stats.total_vertices, 0);
        assert_eq!(stats.total_triangles, 0);
    }
}