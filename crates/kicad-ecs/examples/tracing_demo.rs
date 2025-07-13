//! Tracing demonstration example
//!
//! This example shows how to use structured logging with kicad-ecs.
//!
//! Run with different log levels:
//! ```bash
//! # Basic info logging
//! cargo run --example tracing_demo
//!
//! # Debug logging for kicad-ecs
//! RUST_LOG=kicad_ecs=debug cargo run --example tracing_demo
//!
//! # JSON structured output
//! KICAD_ECS_JSON_LOGS=1 RUST_LOG=debug cargo run --example tracing_demo
//!
//! # Very verbose
//! RUST_LOG=trace cargo run --example tracing_demo
//! ```

use kicad_ecs::prelude::*;
use tracing::{debug, error, info, instrument, span, warn, Level};

#[tokio::main]
async fn main() -> Result<()> {
    // Try different initialization methods:
    
    // Option 1: Simple init
    // kicad_ecs::tracing::init();
    
    // Option 2: Custom filter
    // kicad_ecs::tracing::init_with_filter("debug");
    
    // Option 3: Example-friendly (what we'll use)
    kicad_ecs::tracing::init_for_examples();

    info!("Starting tracing demonstration");
    
    // Demonstrate different log levels
    debug!("This is a debug message");
    info!("This is an info message");
    warn!("This is a warning message");
    
    // Demonstrate structured logging
    demonstrate_client_connection().await?;
    
    // Demonstrate ECS analysis with tracing
    demonstrate_ecs_analysis().await?;
    
    info!("Tracing demonstration complete");
    Ok(())
}

/// Demonstrates client connection with tracing
#[instrument]
async fn demonstrate_client_connection() -> Result<()> {
    info!("Demonstrating client connection with tracing");
    
    // Create a span for this operation
    let _span = span!(Level::INFO, "kicad_connection").entered();
    
    debug!("Creating KiCad client");
    let client = KiCadClient::connect();
    
    match client {
        Ok(_client) => {
            info!("Successfully created client (but API not implemented yet)");
        }
        Err(e) => {
            warn!("Expected error creating client: {}", e);
            debug!("Error details: {:?}", e);
        }
    }
    
    Ok(())
}

/// Demonstrates ECS analysis with structured logging
#[instrument]
async fn demonstrate_ecs_analysis() -> Result<()> {
    info!("Starting ECS analysis demonstration");
    
    let mut world = PcbWorld::new();
    
    // Create some demo components with logging
    create_demo_components(&mut world);
    
    // Analyze components
    analyze_components(&mut world);
    
    info!("ECS analysis complete");
    Ok(())
}

#[instrument(skip(world))]
fn create_demo_components(world: &mut PcbWorld) {
    use kicad_ecs::components::*;
    
    info!("Creating demo components");
    
    let components = [
        ("R1", "10k", "Resistor_SMD:R_0603"),
        ("R2", "4.7k", "Resistor_SMD:R_0603"),
        ("C1", "100nF", "Capacitor_SMD:C_0603"),
        ("C2", "10uF", "Capacitor_SMD:C_1206"),
        ("U1", "STM32F401", "Package_QFP:LQFP-64"),
    ];
    
    for (i, (ref_des, value, footprint)) in components.iter().enumerate() {
        debug!("Adding component: {} = {}", ref_des, value);
        
        world.world.spawn((
            ComponentId { uuid: format!("uuid-{}", i) },
            ComponentInfo {
                reference: ref_des.to_string(),
                value: value.to_string(),
                footprint_name: footprint.to_string(),
            },
            Position { 
                x: (i as f64) * 10.0, 
                y: (i as f64) * 5.0, 
                rotation: 0.0 
            },
            Layer { layer_name: "F.Cu".to_string() },
            ComponentFlags {
                exclude_from_bom: false,
                do_not_populate: false,
                locked: false,
            },
        ));
    }
    
    info!("Created {} demo components", components.len());
}

#[instrument(skip(world))]
fn analyze_components(world: &mut PcbWorld) {
    info!("Analyzing components with ECS queries");
    
    // Count components by type
    let mut query = world.world.query::<&kicad_ecs::components::ComponentInfo>();
    let total_components = query.iter(&world.world).count();
    
    info!(component_count = total_components, "Component analysis");
    
    // Count by component type prefix
    let mut resistors = 0;
    let mut capacitors = 0;
    let mut ics = 0;
    let mut others = 0;
    
    for info in query.iter(&world.world) {
        match info.reference.chars().next() {
            Some('R') => {
                resistors += 1;
                debug!("Found resistor: {} = {}", info.reference, info.value);
            }
            Some('C') => {
                capacitors += 1;
                debug!("Found capacitor: {} = {}", info.reference, info.value);
            }
            Some('U') => {
                ics += 1;
                debug!("Found IC: {} = {}", info.reference, info.value);
            }
            _ => {
                others += 1;
                debug!("Found other component: {} = {}", info.reference, info.value);
            }
        }
    }
    
    // Log the analysis results
    info!(
        resistors = resistors,
        capacitors = capacitors,
        ics = ics,
        others = others,
        "Component type breakdown"
    );
    
    if resistors == 0 && capacitors == 0 && ics == 0 {
        error!("No components found - this shouldn't happen!");
    }
}