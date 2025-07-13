//! Basic LibrePCB ECS example
//! 
//! Demonstrates the placeholder functionality for LibrePCB integration.

use librepcb_ecs::*;

fn main() -> Result<()> {
    // Initialize tracing
    tracing_subscriber::fmt::init();
    
    println!("LibrePCB ECS Basic Example");
    println!("==========================");
    
    // Create a new LibrePCB ECS world
    let mut pcb_world = LibrePcbWorld::new();
    
    // Create some placeholder component data
    let component_info = LibrePcbComponentInfo {
        name: "R1".to_string(),
        value: "10k".to_string(),
        device_name: "Resistor_SMD_0805".to_string(),
        library: "Standard".to_string(),
    };
    
    let position = LibrePcbPosition {
        x: 10.0,
        y: 5.0,
        rotation: 0.0,
    };
    
    let layer = LibrePcbLayer {
        name: "Top".to_string(),
        layer_type: LibrePcbLayerType::TopCopper,
        visible: true,
    };
    
    // Spawn the component
    let entity = pcb_world.spawn_component(
        "R1_ID".to_string(),
        component_info,
        position,
        layer,
    );
    
    println!("Spawned component entity: {:?}", entity);
    
    // Update the world (runs classification and other systems)
    pcb_world.update();
    
    // Get and display all components
    let components = pcb_world.get_components();
    println!("Components in world: {}", components.len());
    
    for (id, info, pos) in components {
        println!("  {} ({}): {} at ({:.1}, {:.1})", 
                 info.name, id, info.value, pos.x, pos.y);
    }
    
    // Try to connect to LibrePCB (will fail with placeholder)
    match pcb_world.connect_to_librepcb() {
        Ok(()) => println!("Connected to LibrePCB!"),
        Err(LibrePcbError::ApiNotAvailable) => {
            println!("LibrePCB API not yet implemented (placeholder mode)");
        }
        Err(e) => println!("LibrePCB connection error: {}", e),
    }
    
    Ok(())
}