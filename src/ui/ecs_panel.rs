use crate::DemoLensApp;
use crate::ecs::{layer_to_3d_meshes, ExtrusionEngine};
use crate::viewer3d::PcbViewer;

/// Show the ECS (3D) panel with 3D PCB visualization
pub fn show_ecs_panel(ui: &mut egui::Ui, app: &mut DemoLensApp) {
    ui.heading("3D PCB Viewer");
    
    // Initialize 3D viewer if needed
    if app.pcb_viewer.is_none() {
        app.pcb_viewer = Some(PcbViewer::new());
    }
    
    // Button to generate 3D meshes from current layer
    ui.horizontal(|ui| {
        if ui.button("Generate 3D from Current Layer").clicked() {
            generate_3d_meshes_from_layer(app);
        }
        
        if ui.button("Generate 3D from All Layers").clicked() {
            generate_3d_meshes_from_all_layers(app);
        }
        
        if ui.button("Clear 3D Meshes").clicked() {
            if let Some(ref mut viewer) = app.pcb_viewer {
                viewer.set_meshes(Vec::new());
            }
        }
    });
    
    ui.separator();
    
    // Show the 3D viewer
    if let Some(ref mut viewer) = app.pcb_viewer {
        viewer.show(ui);
    } else {
        ui.label("3D Viewer not initialized");
    }
}

/// Generate 3D meshes from the current gerber layer
fn generate_3d_meshes_from_layer(app: &mut DemoLensApp) {
    if app.gerber_layer.is_empty() {
        return;
    }
    
    let mut engine = ExtrusionEngine::new();
    let layer_height = 0.1; // 0.1mm thick layer
    let material_id = 1; // Copper material
    
    let meshes = layer_to_3d_meshes(
        &app.gerber_layer,
        layer_height,
        material_id,
        &mut engine,
    );
    
    if let Some(ref mut viewer) = app.pcb_viewer {
        viewer.set_meshes(meshes);
    }
}

/// Generate 3D meshes from all loaded layers
fn generate_3d_meshes_from_all_layers(app: &mut DemoLensApp) {
    let mut all_meshes = Vec::new();
    let mut engine = ExtrusionEngine::new();
    
    // Process each layer type with different heights and materials
    let layer_configs = [
        ("F.Cu", 0.1, 1),      // Front copper
        ("B.Cu", 0.1, 1),      // Back copper
        ("F.Mask", 0.05, 2),   // Front solder mask
        ("B.Mask", 0.05, 2),   // Back solder mask
        ("F.SilkS", 0.02, 3),  // Front silkscreen
        ("B.SilkS", 0.02, 3),  // Back silkscreen
        ("F.Paste", 0.03, 4),  // Front solder paste
        ("B.Paste", 0.03, 4),  // Back solder paste
    ];
    
    let mut z_offset = 0.0;
    
    for (layer_name, thickness, material_id) in &layer_configs {
        if let Some(layer_info) = app.layer_manager.layers.iter()
            .find(|info| info.name.contains(layer_name)) {
            
            if let Some(layer) = &layer_info.gerber_layer {
                if !layer.is_empty() {
                    let mut layer_meshes = layer_to_3d_meshes(
                        layer,
                        *thickness,
                        *material_id,
                        &mut engine,
                    );
                    
                    // Offset the layer in Z direction
                    for mesh in &mut layer_meshes {
                        for vertex in &mut mesh.vertices {
                            vertex.z += z_offset;
                        }
                    }
                    
                    all_meshes.extend(layer_meshes);
                }
            }
        }
        
        z_offset += thickness;
    }
    
    if let Some(ref mut viewer) = app.pcb_viewer {
        viewer.set_meshes(all_meshes);
    }
}