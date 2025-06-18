use bevy_ecs::prelude::*;
use nalgebra::{Point2, Point3};
use std::collections::HashMap;
use std::path::Path;
use gerber_viewer::GerberLayer;
use crate::ecs::{
    KiForgeWorld, StackupMeshGenerator, Polygon2D, Mesh3D,
    Position3D, Transform3D, Renderable3D, StackupLayer,
    KiParseExtractor, ExtractedPcbData, KiCadComponent, Renderer3D,
    GerberLayerComponent, LayerTypeExt, PcbElement
};
use crate::layer_operations::{LayerType, LayerManager};

/// Complete 3D PCB system that integrates Gerber layers, KiCad components, and 3D rendering
pub struct Pcb3DSystem {
    /// 3D mesh generation
    pub stackup_generator: StackupMeshGenerator,
    /// KiCad file parsing
    pub kiparse_extractor: KiParseExtractor,
    /// 3D renderer
    pub renderer: Renderer3D,
    /// Generated 3D data cache
    pub layer_meshes: HashMap<LayerType, Mesh3D>,
    /// Component meshes cache
    pub component_meshes: HashMap<String, Mesh3D>,
}

impl Pcb3DSystem {
    pub fn new() -> Self {
        Self {
            stackup_generator: StackupMeshGenerator::new(),
            kiparse_extractor: KiParseExtractor::new(),
            renderer: Renderer3D::new(),
            layer_meshes: HashMap::new(),
            component_meshes: HashMap::new(),
        }
    }

    /// Generate complete 3D PCB from layer manager and optional KiCad file
    pub fn generate_3d_pcb(
        &mut self,
        layer_manager: &LayerManager,
        kicad_file: Option<&Path>,
        ecs_world: &mut KiForgeWorld,
    ) -> Result<Pcb3DGenerationResult, Pcb3DError> {
        log::info!("Starting 3D PCB generation...");
        
        let mut result = Pcb3DGenerationResult::default();

        // Step 1: Extract polygons from Gerber layers
        let layer_polygons = self.extract_gerber_polygons(layer_manager)?;
        result.layers_processed = layer_polygons.len();

        // Step 2: Create stackup information
        let stackup_layers = self.create_stackup_layers(&layer_polygons);

        // Step 3: Generate 3D meshes for each layer
        for (layer_type, polygons) in &layer_polygons {
            if let Some(stackup) = stackup_layers.get(layer_type) {
                log::info!("Generating 3D mesh for layer {:?}", layer_type);
                
                let mesh = self.stackup_generator.generate_layer_mesh(
                    polygons.clone(),
                    layer_type,
                    stackup,
                )?;

                // Create ECS entity for the layer mesh
                let entity = ecs_world.world_mut().spawn((
                    mesh.clone(),
                    Position3D::new(0.0, 0.0, (stackup.z_bottom + stackup.z_top) / 2.0),
                    Transform3D::default(),
                    Renderable3D::default(),
                    GerberLayerComponent::new(layer_type.clone()),
                    stackup.clone(),
                )).id();

                self.layer_meshes.insert(layer_type.clone(), mesh);
                result.layer_entities.push(entity);
                
                log::info!("Created 3D mesh entity for layer {:?} (entity: {:?})", layer_type, entity);
            }
        }

        // Step 4: Process KiCad file if provided
        if let Some(kicad_path) = kicad_file {
            match self.process_kicad_file(kicad_path, ecs_world) {
                Ok(kicad_result) => {
                    result.components_processed = kicad_result.components_converted;
                    result.component_entities.extend(kicad_result.component_entities);
                    log::info!("Successfully processed KiCad file: {} components", kicad_result.components_converted);
                }
                Err(e) => {
                    log::warn!("Failed to process KiCad file: {}", e);
                    result.warnings.push(format!("KiCad processing failed: {}", e));
                }
            }
        }

        // Step 5: Generate complete stackup mesh
        let complete_mesh = self.stackup_generator.generate_complete_stackup(
            layer_polygons,
            stackup_layers,
        )?;

        // Create entity for complete PCB mesh
        let pcb_entity = ecs_world.world_mut().spawn((
            complete_mesh,
            Position3D::new(0.0, 0.0, 0.0),
            Transform3D::default(),
            Renderable3D::default(),
            PcbAssembly {
                name: "Complete PCB".to_string(),
                layer_count: result.layers_processed,
                component_count: result.components_processed,
            },
        )).id();

        result.pcb_entity = Some(pcb_entity);

        // Step 6: Set up optimal camera view
        self.setup_camera_view(&result, ecs_world);

        log::info!("3D PCB generation completed: {} layers, {} components", 
                  result.layers_processed, result.components_processed);

        Ok(result)
    }

    /// Extract polygons from Gerber layers
    fn extract_gerber_polygons(
        &self,
        layer_manager: &LayerManager,
    ) -> Result<HashMap<LayerType, Vec<Polygon2D>>, Pcb3DError> {
        let mut layer_polygons = HashMap::new();

        for (layer_type, layer_info) in &layer_manager.layers {
            if let Some(ref gerber_layer) = layer_info.gerber_layer {
                log::debug!("Extracting polygons from layer {:?}", layer_type);
                
                let polygons = self.extract_polygons_from_gerber(gerber_layer)?;
                if !polygons.is_empty() {
                    let poly_count = polygons.len();
                    layer_polygons.insert(layer_type.clone(), polygons);
                    log::debug!("Extracted {} polygons from layer {:?}", poly_count, layer_type);
                }
            }
        }

        Ok(layer_polygons)
    }

    /// Extract polygons from a single Gerber layer
    fn extract_polygons_from_gerber(
        &self,
        gerber_layer: &GerberLayer,
    ) -> Result<Vec<Polygon2D>, Pcb3DError> {
        let mut polygons = Vec::new();
        
        // Get the bounding box to create a simple polygon representation
        let bbox = gerber_layer.bounding_box();
        
        // For now, create a simple rectangle polygon from the bounding box
        // In a real implementation, we'd parse the actual Gerber commands to extract polygons
        let polygon = Polygon2D::new(vec![
            Point2::new(bbox.min.x, bbox.min.y),
            Point2::new(bbox.max.x, bbox.min.y),
            Point2::new(bbox.max.x, bbox.max.y),
            Point2::new(bbox.min.x, bbox.max.y),
        ]);

        if polygon.is_valid() {
            polygons.push(polygon);
        }

        // TODO: Implement proper Gerber polygon extraction using gerber_types
        // This would involve:
        // 1. Parsing aperture definitions
        // 2. Following draw commands (lines, arcs, flashes)
        // 3. Building polygon regions from filled areas
        // 4. Handling polygon pours and cutouts

        Ok(polygons)
    }

    /// Create stackup layer information
    fn create_stackup_layers(
        &self,
        layer_polygons: &HashMap<LayerType, Vec<Polygon2D>>,
    ) -> HashMap<LayerType, StackupLayer> {
        let mut stackup_layers = HashMap::new();
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
            if layer_polygons.contains_key(layer_type) {
                let thickness = layer_type.default_thickness();
                let z_bottom = current_z;
                let z_top = current_z + thickness;

                let stackup = StackupLayer {
                    layer_index: index as u32,
                    z_bottom,
                    z_top,
                    material: layer_type.default_material(),
                };

                stackup_layers.insert(layer_type.clone(), stackup);
                current_z = z_top;
            }
        }

        // Add mechanical outline (substrate) in the middle
        if layer_polygons.contains_key(&LayerType::MechanicalOutline) {
            let substrate_thickness = LayerType::MechanicalOutline.default_thickness();
            let stackup = StackupLayer {
                layer_index: 100, // Special index for substrate
                z_bottom: -substrate_thickness / 2.0,
                z_top: substrate_thickness / 2.0,
                material: LayerType::MechanicalOutline.default_material(),
            };
            stackup_layers.insert(LayerType::MechanicalOutline, stackup);
        }

        stackup_layers
    }

    /// Process KiCad PCB file and extract component information
    fn process_kicad_file(
        &mut self,
        kicad_file: &Path,
        ecs_world: &mut KiForgeWorld,
    ) -> Result<KiCadProcessingResult, Pcb3DError> {
        log::info!("Processing KiCad file: {}", kicad_file.display());
        
        let extracted_data = self.kiparse_extractor.extract_from_file(kicad_file)
            .map_err(|e| Pcb3DError::KiParseError(e.to_string()))?;

        let summary = extracted_data.summary();
        log::info!("Extracted: {} components, {} traces, {} vias", 
                  summary.total_components, summary.total_traces, summary.total_vias);

        // Convert to ECS entities
        let conversion_stats = self.kiparse_extractor.convert_to_ecs_entities(
            &extracted_data,
            ecs_world.world_mut(),
        ).map_err(|e| Pcb3DError::KiParseError(e.to_string()))?;

        // Generate 3D meshes for components
        let component_entities = self.generate_component_meshes(&extracted_data, ecs_world)?;

        Ok(KiCadProcessingResult {
            components_converted: conversion_stats.components_converted,
            traces_converted: conversion_stats.traces_converted,
            vias_converted: conversion_stats.vias_converted,
            component_entities,
        })
    }

    /// Generate 3D meshes for components  
    fn generate_component_meshes(
        &mut self,
        data: &ExtractedPcbData,
        ecs_world: &mut KiForgeWorld,
    ) -> Result<Vec<Entity>, Pcb3DError> {
        let mut component_entities = Vec::new();

        for component in &data.components {
            // Generate simple box mesh for component
            let mesh = self.generate_component_box_mesh(component)?;
            
            let entity = ecs_world.world_mut().spawn((
                mesh.clone(),
                Position3D::new(0.0, 0.0, self.get_component_z_position(component)),
                Transform3D::default(),
                Renderable3D::default(),
                component.clone(),
            )).id();

            self.component_meshes.insert(component.reference.clone(), mesh);
            component_entities.push(entity);
            
            log::debug!("Generated 3D mesh for component {} (entity: {:?})", 
                       component.reference, entity);
        }

        Ok(component_entities)
    }

    /// Generate a simple box mesh for a component
    fn generate_component_box_mesh(&mut self, component: &KiCadComponent) -> Result<Mesh3D, Pcb3DError> {
        // Create a simple box based on component type
        let (width, height, depth) = self.get_component_dimensions(component);
        
        let polygon = Polygon2D::new(vec![
            Point2::new(-width / 2.0, -height / 2.0),
            Point2::new(width / 2.0, -height / 2.0),
            Point2::new(width / 2.0, height / 2.0),
            Point2::new(-width / 2.0, height / 2.0),
        ]);

        let mesh = self.stackup_generator.extrusion_engine.extrude_polygon(
            &polygon,
            0.0,
            depth as f32,
            Some(format!("component_{}", component.reference)),
        ).map_err(|e| Pcb3DError::MeshGenerationError(e.to_string()))?;

        Ok(mesh)
    }

    /// Get component dimensions based on footprint
    fn get_component_dimensions(&self, component: &KiCadComponent) -> (f64, f64, f64) {
        // Simple heuristic based on footprint name
        match component.footprint.as_str() {
            fp if fp.contains("0402") => (1.0, 0.5, 0.3),
            fp if fp.contains("0603") => (1.6, 0.8, 0.45),
            fp if fp.contains("0805") => (2.0, 1.25, 0.6),
            fp if fp.contains("1206") => (3.2, 1.6, 0.6),
            fp if fp.contains("SOIC") => (5.0, 4.0, 1.5),
            fp if fp.contains("QFP") => (10.0, 10.0, 1.0),
            fp if fp.contains("BGA") => (15.0, 15.0, 1.0),
            _ => (2.0, 2.0, 1.0), // Default size
        }
    }

    /// Get Z position for component based on layer
    fn get_component_z_position(&self, component: &KiCadComponent) -> f64 {
        match component.layer {
            crate::ecs::kiparse_integration::KiCadLayer::Front => 2.0,
            crate::ecs::kiparse_integration::KiCadLayer::Back => -2.0,
            crate::ecs::kiparse_integration::KiCadLayer::Both => 0.0, // Through-hole center
        }
    }

    /// Set up optimal camera view for the generated PCB
    fn setup_camera_view(&mut self, _result: &Pcb3DGenerationResult, _ecs_world: &KiForgeWorld) {
        // Simplified camera setup - just set to a default view
        // In a real implementation, we'd calculate bounding box from meshes
        let bbox_min = Point3::new(-50.0, -50.0, -5.0);
        let bbox_max = Point3::new(50.0, 50.0, 5.0);
        
        self.renderer.frame_pcb(bbox_min, bbox_max);
        log::info!("Set camera to default PCB view");
    }

    /// Get renderer for external access
    pub fn renderer(&self) -> &Renderer3D {
        &self.renderer
    }

    /// Get mutable renderer for external access
    pub fn renderer_mut(&mut self) -> &mut Renderer3D {
        &mut self.renderer
    }

    /// Get layer mesh cache
    pub fn layer_meshes(&self) -> &HashMap<LayerType, Mesh3D> {
        &self.layer_meshes
    }

    /// Get component mesh cache
    pub fn component_meshes(&self) -> &HashMap<String, Mesh3D> {
        &self.component_meshes
    }

    /// Clear all caches
    pub fn clear_caches(&mut self) {
        self.layer_meshes.clear();
        self.component_meshes.clear();
        self.stackup_generator.extrusion_engine.clear_cache();
    }
}

/// Component representing a complete PCB assembly
#[derive(Component, Debug, Clone)]
pub struct PcbAssembly {
    pub name: String,
    pub layer_count: usize,
    pub component_count: usize,
}

/// Result of 3D PCB generation
#[derive(Debug, Default)]
pub struct Pcb3DGenerationResult {
    pub layers_processed: usize,
    pub components_processed: usize,
    pub layer_entities: Vec<Entity>,
    pub component_entities: Vec<Entity>,
    pub pcb_entity: Option<Entity>,
    pub warnings: Vec<String>,
}

/// Result of KiCad file processing
#[derive(Debug, Default)]
pub struct KiCadProcessingResult {
    pub components_converted: usize,
    pub traces_converted: usize,
    pub vias_converted: usize,
    pub component_entities: Vec<Entity>,
}

/// Errors that can occur during 3D PCB generation
#[derive(Debug, Clone)]
pub enum Pcb3DError {
    GerberExtractionError(String),
    MeshGenerationError(String),
    KiParseError(String),
    StackupError(String),
    RenderingError(String),
}

impl std::fmt::Display for Pcb3DError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Pcb3DError::GerberExtractionError(msg) => write!(f, "Gerber extraction error: {}", msg),
            Pcb3DError::MeshGenerationError(msg) => write!(f, "Mesh generation error: {}", msg),
            Pcb3DError::KiParseError(msg) => write!(f, "KiCad parsing error: {}", msg),
            Pcb3DError::StackupError(msg) => write!(f, "Stackup error: {}", msg),
            Pcb3DError::RenderingError(msg) => write!(f, "Rendering error: {}", msg),
        }
    }
}

impl From<crate::ecs::mesh3d::ExtrusionError> for Pcb3DError {
    fn from(err: crate::ecs::mesh3d::ExtrusionError) -> Self {
        Pcb3DError::MeshGenerationError(err.to_string())
    }
}

impl std::error::Error for Pcb3DError {}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn test_pcb3d_system_creation() {
        let system = Pcb3DSystem::new();
        assert_eq!(system.layer_meshes.len(), 0);
        assert_eq!(system.component_meshes.len(), 0);
    }

    #[test]
    fn test_component_dimensions() {
        let system = Pcb3DSystem::new();
        
        let mut component = KiCadComponent::new(
            "R1".to_string(),
            "R_0603".to_string(),
            crate::ecs::kiparse_integration::KiCadLayer::Front,
        );
        
        let (w, h, d) = system.get_component_dimensions(&component);
        assert!(w > 0.0 && h > 0.0 && d > 0.0);
    }

    #[test]
    fn test_component_z_position() {
        let system = Pcb3DSystem::new();
        
        let front_component = KiCadComponent::new(
            "U1".to_string(),
            "SOIC-8".to_string(),
            crate::ecs::kiparse_integration::KiCadLayer::Front,
        );
        
        let back_component = KiCadComponent::new(
            "U2".to_string(),
            "SOIC-8".to_string(),
            crate::ecs::kiparse_integration::KiCadLayer::Back,
        );
        
        assert!(system.get_component_z_position(&front_component) > 0.0);
        assert!(system.get_component_z_position(&back_component) < 0.0);
    }
}