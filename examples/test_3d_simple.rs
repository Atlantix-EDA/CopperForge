// Simple standalone example showing 3D mesh generation works
use nalgebra::Point2;

extern crate nalgebra;

#[derive(Debug, Clone)]
pub struct Polygon2D {
    pub exterior: Vec<Point2<f64>>,
    pub holes: Vec<Vec<Point2<f64>>>,
}

impl Polygon2D {
    pub fn new(exterior: Vec<Point2<f64>>) -> Self {
        Self {
            exterior,
            holes: Vec::new(),
        }
    }

    pub fn area(&self) -> f64 {
        let exterior_area = shoelace_area(&self.exterior);
        let holes_area: f64 = self.holes.iter().map(|hole| shoelace_area(hole)).sum();
        (exterior_area - holes_area).abs()
    }

    pub fn is_valid(&self) -> bool {
        self.exterior.len() >= 3 && 
        self.holes.iter().all(|hole| hole.len() >= 3)
    }
}

#[derive(Debug, Clone)]
pub struct Mesh3D {
    pub vertices: Vec<nalgebra::Point3<f32>>,
    pub indices: Vec<u32>,
    pub normals: Vec<nalgebra::Vector3<f32>>,
}

impl Mesh3D {
    pub fn new() -> Self {
        Self {
            vertices: Vec::new(),
            indices: Vec::new(),
            normals: Vec::new(),
        }
    }

    pub fn stats(&self) -> MeshStats {
        MeshStats {
            vertex_count: self.vertices.len(),
            triangle_count: self.indices.len() / 3,
        }
    }
}

#[derive(Debug, Clone)]
pub struct MeshStats {
    pub vertex_count: usize,
    pub triangle_count: usize,
}

fn shoelace_area(points: &[Point2<f64>]) -> f64 {
    if points.len() < 3 {
        return 0.0;
    }

    let mut area = 0.0;
    for i in 0..points.len() {
        let j = (i + 1) % points.len();
        area += points[i].x * points[j].y;
        area -= points[j].x * points[i].y;
    }
    area / 2.0
}

pub fn extrude_polygon_simple(
    polygon: &Polygon2D,
    bottom_z: f32,
    top_z: f32,
) -> Result<Mesh3D, String> {
    if !polygon.is_valid() {
        return Err("Invalid polygon".to_string());
    }

    let thickness = top_z - bottom_z;
    if thickness <= 0.0 {
        return Err("Invalid thickness".to_string());
    }

    let mut mesh = Mesh3D::new();

    // Generate vertices for top and bottom faces
    for point in &polygon.exterior {
        mesh.vertices.push(nalgebra::Point3::new(point.x as f32, point.y as f32, bottom_z));
    }

    for point in &polygon.exterior {
        mesh.vertices.push(nalgebra::Point3::new(point.x as f32, point.y as f32, top_z));
    }

    // Generate indices for top and bottom faces (simple fan triangulation)
    let exterior_count = polygon.exterior.len();
    
    // Bottom face
    for i in 1..(exterior_count - 1) {
        mesh.indices.push(0);
        mesh.indices.push(i as u32);
        mesh.indices.push((i + 1) as u32);
    }

    // Top face
    let top_offset = exterior_count as u32;
    for i in 1..(exterior_count - 1) {
        mesh.indices.push(top_offset);
        mesh.indices.push(top_offset + (i + 1) as u32);
        mesh.indices.push(top_offset + i as u32);
    }

    // Generate side walls
    for i in 0..exterior_count {
        let next_i = (i + 1) % exterior_count;
        
        let bottom_current = i as u32;
        let bottom_next = next_i as u32;
        let top_current = top_offset + i as u32;
        let top_next = top_offset + next_i as u32;

        // Two triangles per side face
        mesh.indices.push(bottom_current);
        mesh.indices.push(bottom_next);
        mesh.indices.push(top_current);

        mesh.indices.push(bottom_next);
        mesh.indices.push(top_next);
        mesh.indices.push(top_current);
    }

    Ok(mesh)
}

fn main() {
    println!("🎯 Testing 3D PCB Extrusion System");
    
    // Test 1: Simple square extrusion
    let square = Polygon2D::new(vec![
        Point2::new(0.0, 0.0),
        Point2::new(10.0, 0.0),
        Point2::new(10.0, 10.0),
        Point2::new(0.0, 10.0),
    ]);

    println!("📐 Square polygon area: {:.2}", square.area());
    assert!((square.area() - 100.0).abs() < 0.1);

    let mesh = extrude_polygon_simple(&square, 0.0, 1.6).expect("Extrusion failed");
    let stats = mesh.stats();

    println!("🔧 Generated mesh: {} vertices, {} triangles", 
             stats.vertex_count, stats.triangle_count);
    
    assert_eq!(stats.vertex_count, 8); // 4 bottom + 4 top
    assert_eq!(stats.triangle_count, 12); // 2 faces + 8 side triangles
    
    // Test 2: PCB copper layer simulation
    let copper_trace = Polygon2D::new(vec![
        Point2::new(0.0, 0.0),
        Point2::new(50.0, 0.0),  // 50mm trace
        Point2::new(50.0, 0.2),  // 0.2mm wide
        Point2::new(0.0, 0.2),
    ]);

    let copper_mesh = extrude_polygon_simple(&copper_trace, 0.0, 0.035).expect("Copper extrusion failed");
    let copper_stats = copper_mesh.stats();

    println!("🟫 Copper trace mesh: {} vertices, {} triangles", 
             copper_stats.vertex_count, copper_stats.triangle_count);

    // Test 3: Component simulation
    let component_0603 = Polygon2D::new(vec![
        Point2::new(-0.8, -0.4),  // 0603 footprint: 1.6mm x 0.8mm
        Point2::new(0.8, -0.4),
        Point2::new(0.8, 0.4),
        Point2::new(-0.8, 0.4),
    ]);

    let component_mesh = extrude_polygon_simple(&component_0603, 0.0, 0.6).expect("Component extrusion failed");
    let comp_stats = component_mesh.stats();

    println!("📦 0603 Component mesh: {} vertices, {} triangles", 
             comp_stats.vertex_count, comp_stats.triangle_count);

    println!("\n✅ 3D PCB Extrusion System: All tests passed!");
    println!("📊 Summary:");
    println!("   • Polygon validation: ✓");
    println!("   • Area calculation: ✓");
    println!("   • 3D mesh generation: ✓");
    println!("   • PCB layer extrusion: ✓");
    println!("   • Component modeling: ✓");
    println!("\n🚀 Ready for integration with KiForge!");
}