use kiforge::ecs::{
    ExtrusionEngine, Polygon2D, Mesh3D, StackupMeshGenerator, 
    KiCadComponent, KiCadLayer, ComponentCategory, Camera3D, RenderMaterial
};
use kiforge::layer_operations::LayerType;
use nalgebra::Point2;
use std::collections::HashMap;

#[test]
fn test_3d_mesh_generation() {
    let mut engine = ExtrusionEngine::new();
    
    // Create a simple square polygon
    let polygon = Polygon2D::new(vec![
        Point2::new(0.0, 0.0),
        Point2::new(10.0, 0.0),
        Point2::new(10.0, 10.0),
        Point2::new(0.0, 10.0),
    ]);
    
    // Extrude to create 3D mesh
    let result = engine.extrude_polygon(&polygon, 0.0, 1.6, None);
    assert!(result.is_ok());
    
    let mesh = result.unwrap();
    assert_eq!(mesh.vertices.len(), 8); // 4 bottom + 4 top vertices
    assert!(!mesh.indices.is_empty());
    assert!(!mesh.normals.is_empty());
    
    // Check bounding box
    let bbox = mesh.bounding_box();
    assert!(bbox.is_some());
    let (min, max) = bbox.unwrap();
    assert_eq!(min.z, 0.0);
    assert_eq!(max.z, 1.6);
}

#[test]
fn test_polygon_with_holes() {
    let mut engine = ExtrusionEngine::new();
    
    // Create polygon with hole
    let exterior = vec![
        Point2::new(0.0, 0.0),
        Point2::new(10.0, 0.0),
        Point2::new(10.0, 10.0),
        Point2::new(0.0, 10.0),
    ];
    
    let hole = vec![
        Point2::new(3.0, 3.0),
        Point2::new(7.0, 3.0),
        Point2::new(7.0, 7.0),
        Point2::new(3.0, 7.0),
    ];
    
    let polygon = Polygon2D::new(exterior).with_holes(vec![hole]);
    
    // Check area calculation
    let area = polygon.area();
    assert!((area - 84.0).abs() < 0.1); // 100 - 16 = 84
    
    let result = engine.extrude_polygon(&polygon, 0.0, 1.0, None);
    assert!(result.is_ok());
}

#[test]
fn test_basic_stackup() {
    let generator = StackupMeshGenerator::new();
    // Just test that we can create the generator
    assert!(generator.extrusion_engine.mesh_cache.is_empty());
}

#[test]
fn test_kiparse_component_category() {
    let resistor = KiCadComponent::new(
        "R1".to_string(), 
        "R_0603".to_string(), 
        KiCadLayer::Front
    );
    assert_eq!(resistor.category(), ComponentCategory::Resistor);
    
    let capacitor = KiCadComponent::new(
        "C10".to_string(), 
        "C_0805".to_string(), 
        KiCadLayer::Back
    );
    assert_eq!(capacitor.category(), ComponentCategory::Capacitor);
    
    let ic = KiCadComponent::new(
        "U1".to_string(), 
        "SOIC-8".to_string(), 
        KiCadLayer::Front
    );
    assert_eq!(ic.category(), ComponentCategory::IntegratedCircuit);
}

#[test]
fn test_component_type_detection() {
    let smd_component = KiCadComponent::new(
        "R1".to_string(), 
        "R_0603".to_string(), 
        KiCadLayer::Front
    );
    assert!(smd_component.is_smd());
    assert!(!smd_component.is_through_hole());
    
    let through_hole_component = KiCadComponent::new(
        "J1".to_string(), 
        "Pin_Header_2x5".to_string(), 
        KiCadLayer::Both
    );
    assert!(!through_hole_component.is_smd());
    assert!(through_hole_component.is_through_hole());
}

#[test]
fn test_camera_controls() {
    let mut camera = Camera3D::default();
    let initial_position = camera.position;
    let initial_distance = (camera.position - camera.target).magnitude();
    
    // Test orbit
    camera.orbit(0.1, 0.1);
    assert_ne!(camera.position, initial_position);
    
    // Distance should remain roughly the same after orbit
    let new_distance = (camera.position - camera.target).magnitude();
    assert!((initial_distance - new_distance).abs() < 0.01);
    
    // Test zoom
    camera.zoom(0.5);
    let zoomed_distance = (camera.position - camera.target).magnitude();
    assert!(zoomed_distance < new_distance);
    
    // Test pan
    let initial_target = camera.target;
    camera.pan(1.0, 1.0);
    assert_ne!(camera.target, initial_target);
}

#[test]
fn test_render_materials() {
    let copper = RenderMaterial::copper();
    assert_eq!(copper.id, 1);
    assert!(copper.metallic > 0.5);
    assert!(!copper.transparent);
    
    let soldermask = RenderMaterial::soldermask([0.0, 0.8, 0.0, 0.9]);
    assert_eq!(soldermask.id, 2);
    assert!(soldermask.transparent);
    
    let fr4 = RenderMaterial::fr4();
    assert_eq!(fr4.id, 5);
    assert!(!fr4.transparent);
}

#[test]
fn test_mesh_statistics() {
    let mut mesh = Mesh3D::new();
    
    // Add some vertices
    mesh.vertices = vec![
        nalgebra::Point3::new(0.0, 0.0, 0.0),
        nalgebra::Point3::new(1.0, 0.0, 0.0),
        nalgebra::Point3::new(0.0, 1.0, 0.0),
        nalgebra::Point3::new(0.0, 0.0, 1.0),
    ];
    
    // Add two triangles
    mesh.indices = vec![0, 1, 2, 0, 2, 3];
    
    let stats = mesh.stats();
    assert_eq!(stats.vertex_count, 4);
    assert_eq!(stats.triangle_count, 2);
    assert!(!stats.has_normals);
    assert!(!stats.has_uvs);
}

#[test]
fn test_pcb3d_system_creation() {
    let system = Pcb3DSystem::new();
    assert_eq!(system.layer_meshes().len(), 0);
    assert_eq!(system.component_meshes().len(), 0);
}

#[test]
fn test_basic_component_data() {
    let mut component = KiCadComponent::new(
        "R1".to_string(),
        "R_0603".to_string(),
        KiCadLayer::Front,
    );
    component.value = Some("100".to_string());
    
    assert_eq!(component.reference, "R1");
    assert_eq!(component.value, Some("100".to_string()));
    assert_eq!(component.category(), ComponentCategory::Resistor);
    assert!(component.is_smd());
}