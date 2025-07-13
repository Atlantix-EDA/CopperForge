//! # Real KiCad ECS Integration
//!
//! This example connects to an actual running KiCad instance and loads the open board
//! into an ECS for analysis. This is the real deal - no fake data.
//!
//! Prerequisites:
//! 1. KiCad must be running
//! 2. A PCB board must be open in KiCad
//! 3. KiCad API must be enabled
//!
//! Run with: `cargo run --example real_kicad_ecs`

use kicad_ecs::prelude::*;
use prettytable::{Table, row, format, Cell};
use tracing::{info, warn, error};
use std::time::Duration;

/// Simple state machine for managing KiCad connection
#[derive(Debug, Clone, PartialEq)]
enum ConnectionState {
    Disconnected,
    Connecting,
    Connected,
    LoadingBoard,
    AnalyzingBoard,
    Error(String),
    Terminated,
}

/// Supervisor that manages the connection and ECS world
struct KiCadSupervisor {
    state: ConnectionState,
    client: Option<kicad_ecs::client::KiCadClient>,
    pcb_world: PcbWorld,
}

impl KiCadSupervisor {
    fn new() -> Self {
        Self {
            state: ConnectionState::Disconnected,
            client: None,
            pcb_world: PcbWorld::new(),
        }
    }

    /// Try to connect to KiCad
    async fn try_connect(&mut self) -> Result<(), String> {
        info!("Attempting to connect to KiCad");
        
        match kicad_ecs::client::KiCadClient::connect() {
            Ok(client) => {
                info!("Successfully connected to KiCad");
                self.client = Some(client);
                self.state = ConnectionState::Connected;
                Ok(())
            }
            Err(e) => {
                error!("Failed to connect to KiCad: {}", e);
                let err_msg = format!("Connection failed: {}", e);
                self.state = ConnectionState::Error(err_msg.clone());
                Err(err_msg)
            }
        }
    }

    /// Load the open board from KiCad
    async fn load_open_board(&mut self) -> Result<(), String> {
        info!("Loading open board from KiCad");
        
        if let Some(client) = &mut self.client {
            // Get the open board
            match client.get_board().await {
                Ok(board) => {
                    info!("Successfully got board: {}", board.name);
                    println!("📋 Board: {}", board.name);
                    
                    if let Some(project) = &board.project_name {
                        println!("📁 Project: {}", project);
                    }

                    // Get footprints from the board
                    match client.get_footprints().await {
                        Ok(footprints) => {
                            info!("Got {} footprints from board", footprints.len());
                            println!("🔍 Found {} components", footprints.len());

                            if footprints.is_empty() {
                                warn!("No components found on board");
                                return Err("No components found on the open board".to_string());
                            }

                            // Load into ECS
                            self.load_footprints_into_ecs(footprints)?;
                            Ok(())
                        }
                        Err(e) => {
                            error!("Failed to get footprints: {}", e);
                            Err(format!("Failed to get footprints: {}", e))
                        }
                    }
                }
                Err(e) => {
                    error!("Failed to get open board: {}", e);
                    Err(format!("Failed to get open board: {}. Make sure a PCB is open in KiCad!", e))
                }
            }
        } else {
            Err("KiCad client not initialized".to_string())
        }
    }

    /// Load footprint data into ECS world
    fn load_footprints_into_ecs(&mut self, footprints: Vec<kicad_ecs::client::FootprintData>) -> Result<(), String> {

        let mut _mounting_holes = 0;

        for fp in footprints {
            // Check if it's a mounting hole
            if self.is_mounting_hole(&fp) {
                _mounting_holes += 1;
                self.pcb_world.spawn_mounting_hole(
                    fp.id,
                    fp.reference,
                    (fp.position.0, fp.position.1, fp.rotation),
                    fp.layer,
                    3.2, // Default diameter - could parse from footprint name
                    "M3".to_string(), // Default screw size
                );
            } else {
                // Smart spawning based on component reference prefix
                match fp.reference.chars().next() {
                    Some('R') => {
                        self.pcb_world.spawn_resistor(
                            fp.id, fp.reference, fp.value, fp.footprint_name,
                            (fp.position.0, fp.position.1, fp.rotation), fp.layer
                        );
                    },
                    Some('C') => {
                        self.pcb_world.spawn_capacitor(
                            fp.id, fp.reference, fp.value, fp.footprint_name,
                            (fp.position.0, fp.position.1, fp.rotation), fp.layer
                        );
                    },
                    Some('U') => {
                        self.pcb_world.spawn_ic(
                            fp.id, fp.reference, fp.value, fp.footprint_name,
                            (fp.position.0, fp.position.1, fp.rotation), fp.layer
                        );
                    },
                    Some('J') => {
                        self.pcb_world.spawn_connector(
                            fp.id, fp.reference, fp.value, fp.footprint_name,
                            (fp.position.0, fp.position.1, fp.rotation), fp.layer
                        );
                    },
                    _ => {
                        // Generic footprint for unknown component types
                        self.pcb_world.spawn_footprint(fp);
                    },
                }
            }
        }

        
        Ok(())
    }

    /// Check if a footprint is a mounting hole
    fn is_mounting_hole(&self, fp: &kicad_ecs::client::FootprintData) -> bool {
        let ref_name = &fp.reference;
        let value = &fp.value;
        let footprint = &fp.footprint_name;
        
        ref_name.starts_with("H") || 
        ref_name.starts_with("MH") ||
        ref_name.contains("MountingHole") ||
        value.contains("MountingHole") ||
        footprint.contains("MountingHole") ||
        footprint.contains("Hole_")
    }

    /// Perform ECS-based analysis
    fn perform_analysis(&mut self) {

        let stats = self.pcb_world.get_statistics();
        
        // Summary table
        let mut summary_table = Table::new();
        summary_table.set_format(*format::consts::FORMAT_BOX_CHARS);
        summary_table.add_row(row![b->"Board Analysis Summary"]);
        summary_table.add_row(row!["Total Components", stats.total_components]);
        summary_table.add_row(row!["Front Layer (F.Cu)", stats.front_components]);
        summary_table.add_row(row!["Back Layer (B.Cu)", stats.back_components]);
        summary_table.add_row(row!["Other Layers", stats.other_layer_components]);
        summary_table.add_row(row!["Mounting Holes", stats.mounting_holes]);
        
        if stats.dnp_components > 0 {
            summary_table.add_row(row!["Do Not Populate (DNP)", stats.dnp_components]);
        }
        if stats.exclude_bom_components > 0 {
            summary_table.add_row(row!["Exclude from BOM", stats.exclude_bom_components]);
        }
        if stats.locked_components > 0 {
            summary_table.add_row(row!["Locked Components", stats.locked_components]);
        }

        println!("\n📊 ECS Analysis Results:");
        summary_table.printstd();

        // Component breakdown by type
        self.analyze_component_types();
        
        // Detailed component table
        self.print_components();
    }

    /// Analyze component types using ECS queries
    fn analyze_component_types(&mut self) {
        let mut type_counts = std::collections::HashMap::new();
        
        let mut query = self.pcb_world.world.query::<&ComponentInfo>();
        for info in query.iter(&self.pcb_world.world) {
            let comp_type = match info.reference.chars().next() {
                Some('U') => "ICs",
                Some('R') => "Resistors",
                Some('C') => "Capacitors", 
                Some('L') => "Inductors",
                Some('D') => "Diodes/LEDs",
                Some('Q') => "Transistors",
                Some('Y') => "Crystals/Oscillators",
                Some('J') => "Connectors",
                Some('S') if info.reference.starts_with("SW") => "Switches",
                Some('T') if info.reference.starts_with("TP") => "Test Points",
                _ => "Other",
            };
            *type_counts.entry(comp_type).or_insert(0) += 1;
        }

        if !type_counts.is_empty() {
            let mut type_table = Table::new();
            type_table.set_format(*format::consts::FORMAT_BOX_CHARS);
            type_table.add_row(row![b->"Component Type", b->"Count"]);
            
            let mut sorted_types: Vec<_> = type_counts.iter().collect();
            sorted_types.sort_by(|a, b| b.1.cmp(a.1)); // Sort by count descending
            
            for (comp_type, count) in sorted_types {
                type_table.add_row(row![comp_type, count]);
            }
            
            println!("\n🔧 Component Breakdown:");
            type_table.printstd();
        }
    }

    fn print_components(&mut self) {
        let mut table = Table::new();
        table.set_format(*format::consts::FORMAT_BOX_CHARS);
        
        table.add_row(row![b->"Ref", b->"Value", b->"Footprint", b->"Description", b->"X(mm)", b->"Y(mm)", b->"Rot°", b->"Layer"]);
        
        let mut query = self.pcb_world.world.query::<(&ComponentInfo, &Position, &Layer, &ComponentDescription)>();
        for (info, pos, layer, desc) in query.iter(&self.pcb_world.world) {
            
            let layer_name = &layer.layer_name;
            
            // Create a formatted description that wraps at word boundaries
            let desc_wrapped = if desc.description.len() > 12 {
                let words: Vec<&str> = desc.description.split_whitespace().collect();
                let mut lines = Vec::new();
                let mut current_line = String::new();
                
                for word in words {
                    if current_line.len() + word.len() + 1 > 12 {
                        if !current_line.is_empty() {
                            lines.push(current_line);
                            current_line = String::new();
                        }
                    }
                    if !current_line.is_empty() {
                        current_line.push(' ');
                    }
                    current_line.push_str(word);
                }
                if !current_line.is_empty() {
                    lines.push(current_line);
                }
                lines.join("\n")
            } else {
                desc.description.clone()
            };
            
            let desc_cell = if desc_wrapped.contains('\n') {
                let lines: Vec<&str> = desc_wrapped.split('\n').collect();
                let padded_lines: Vec<String> = lines.iter()
                    .map(|line| format!("{:<14}", line))
                    .collect();
                Cell::new(&padded_lines.join("\n"))
            } else {
                Cell::new(&format!("{:<14}", desc_wrapped))
            };
            
            // Truncate columns for reasonable width
            let value_short = if info.value.len() > 12 {
                format!("{}...", &info.value[..9])
            } else {
                info.value.clone()
            };
            
            let footprint_short = if info.footprint_name.len() > 20 {
                format!("{}...", &info.footprint_name[..17])
            } else {
                info.footprint_name.clone()
            };
            
            table.add_row(row![
                info.reference,
                value_short,
                footprint_short,
                desc_cell,
                format!("{:.1}", pos.x),
                format!("{:.1}", pos.y),
                format!("{:.0}", pos.rotation),
                layer_name
            ]);
        }
        
        println!("\nComponent List (all {} components):", self.pcb_world.component_count());
        table.printstd();
    }

    /// Main state machine update
    async fn update(&mut self) {
        match &self.state {
            ConnectionState::Disconnected => {
                println!("🔌 Connecting to KiCad...");
                self.state = ConnectionState::Connecting;
            }
            ConnectionState::Connecting => {
                match self.try_connect().await {
                    Ok(_) => {
                        println!("✅ Connected to KiCad!");
                        self.state = ConnectionState::LoadingBoard;
                    }
                    Err(e) => {
                        println!("❌ Connection failed: {}", e);
                        println!("   Make sure KiCad is running and the API is enabled!");
                        self.state = ConnectionState::Error(e);
                    }
                }
            }
            ConnectionState::Connected => {
                // Move to loading board
                self.state = ConnectionState::LoadingBoard;
            }
            ConnectionState::LoadingBoard => {
                println!("📥 Loading board data...");
                match self.load_open_board().await {
                    Ok(_) => {
                        self.state = ConnectionState::AnalyzingBoard;
                    }
                    Err(e) => {
                        println!("❌ Failed to load board: {}", e);
                        self.state = ConnectionState::Error(e);
                    }
                }
            }
            ConnectionState::AnalyzingBoard => {
                println!("🔍 Analyzing board with ECS...");
                self.perform_analysis();
                println!("\n✅ Analysis complete!");
                self.state = ConnectionState::Terminated;
            }
            ConnectionState::Error(e) => {
                println!("💥 Error: {}", e);
                self.state = ConnectionState::Terminated;
            }
            ConnectionState::Terminated => {
                // Final state - do nothing
            }
        }
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize logging - suppress debug messages
    use tracing_subscriber::EnvFilter;
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env().add_directive("kicad_ecs=info".parse().unwrap()))
        .init();
    
    println!("🚀 KiCad ECS Integration");
    println!("========================");
    println!("Connecting to your open KiCad board...\n");

    let mut supervisor = KiCadSupervisor::new();

    // Run the state machine
    loop {
        supervisor.update().await;

        if supervisor.state == ConnectionState::Terminated {
            break;
        }

        // Small delay between state transitions
        tokio::time::sleep(Duration::from_millis(100)).await;
    }

    println!("\n👋 Exiting KiCad ECS integration");
    Ok(())
}