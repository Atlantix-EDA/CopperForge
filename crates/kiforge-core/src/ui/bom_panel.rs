use crate::DemoLensApp;
use egui_lens::{ReactiveEventLogger, ReactiveEventLoggerState};
use egui_lens::LogColors;
use egui_mobius_reactive::Dynamic;
use egui_extras::TableBuilder;
use kicad_ecs::prelude::*;
use kicad_ecs::client::{KiCadClient, FootprintData};
use tokio::runtime::Runtime;
use std::time::Duration;

/// Component data for the BOM table
#[derive(Clone, Debug)]
pub struct BomComponent {
    pub item_number: String,
    pub reference: String,
    pub description: String,
    pub x_location: f64,
    pub y_location: f64,
    pub orientation: f64,
    pub value: String,
    pub footprint: String,
}

/// BOM panel state
pub struct BomPanelState {
    pub components: Vec<BomComponent>,
    pub connection_status: ConnectionStatus,
    pub last_update: std::time::Instant,
    pub auto_refresh: bool,
    pub refresh_interval: Duration,
    pub filter_text: String,
    pub kicad_client: Option<KiCadClient>,
    pub runtime: Option<Runtime>,
}

#[derive(Clone, Debug)]
pub enum ConnectionStatus {
    Disconnected,
    Connecting,
    Connected,
    Error(String),
}

impl Default for BomPanelState {
    fn default() -> Self {
        Self {
            components: Vec::new(),
            connection_status: ConnectionStatus::Disconnected,
            last_update: std::time::Instant::now(),
            auto_refresh: true,
            refresh_interval: Duration::from_millis(5000), // 5 seconds
            filter_text: String::new(),
            kicad_client: None,
            runtime: None,
        }
    }
}

impl BomPanelState {
    pub fn new() -> Self {
        Self::default()
    }

    /// Initialize the runtime and attempt to connect to KiCad
    pub fn initialize(&mut self, logger: &ReactiveEventLogger) {
        // Create tokio runtime for async operations
        match Runtime::new() {
            Ok(runtime) => {
                self.runtime = Some(runtime);
                self.connect_to_kicad(logger);
            }
            Err(e) => {
                self.connection_status = ConnectionStatus::Error(format!("Failed to create runtime: {}", e));
                logger.log_error(&format!("Failed to create async runtime: {}", e));
            }
        }
    }

    /// Attempt to connect to KiCad
    pub fn connect_to_kicad(&mut self, logger: &ReactiveEventLogger) {
        if let Some(runtime) = &self.runtime {
            self.connection_status = ConnectionStatus::Connecting;
            logger.log_info("Attempting to connect to KiCad...");
            
            match runtime.block_on(async {
                KiCadClient::connect()
            }) {
                Ok(client) => {
                    self.kicad_client = Some(client);
                    self.connection_status = ConnectionStatus::Connected;
                    logger.log_info("Successfully connected to KiCad!");
                    
                    // Initial refresh
                    self.refresh(logger);
                }
                Err(e) => {
                    self.connection_status = ConnectionStatus::Error(format!("Connection failed: {}", e));
                    logger.log_error(&format!("Failed to connect to KiCad: {}", e));
                }
            }
        }
    }

    /// Auto-refresh if enabled and enough time has passed
    pub fn maybe_auto_refresh(&mut self, logger: &ReactiveEventLogger) {
        if self.auto_refresh && 
           self.last_update.elapsed() >= self.refresh_interval &&
           matches!(self.connection_status, ConnectionStatus::Connected) {
            self.refresh(logger);
        }
    }

    /// Fetch components from KiCad
    async fn fetch_components(client: &mut KiCadClient) -> Result<Vec<BomComponent>, String> {
        let mut components = Vec::new();
        
        // Get board information
        let _board = client.get_board().await
            .map_err(|e| format!("Failed to get board: {}", e))?;
        
        // Get footprints
        let footprints = client.get_footprints().await
            .map_err(|e| format!("Failed to get footprints: {}", e))?;
        
        // Convert footprints to BOM components
        for (idx, footprint) in footprints.iter().enumerate() {
            let component = BomComponent {
                item_number: format!("{:03}", idx + 1),
                reference: footprint.reference.clone(),
                description: Self::generate_description(footprint),
                x_location: footprint.position.0,
                y_location: footprint.position.1,
                orientation: footprint.rotation,
                value: footprint.value.clone(),
                footprint: footprint.footprint_name.clone(),
            };
            components.push(component);
        }
        
        Ok(components)
    }

    /// Generate description for a component
    fn generate_description(footprint: &FootprintData) -> String {
        // Try to create a meaningful description from available data
        let mut description = String::new();
        
        // Add component type based on reference
        match footprint.reference.chars().next() {
            Some('R') => description.push_str("Resistor"),
            Some('C') => description.push_str("Capacitor"),
            Some('U') => description.push_str("Integrated Circuit"),
            Some('J') => description.push_str("Connector"),
            Some('L') => description.push_str("Inductor"),
            Some('D') => description.push_str("Diode"),
            Some('Q') => description.push_str("Transistor"),
            Some('Y') => description.push_str("Crystal/Oscillator"),
            Some('S') if footprint.reference.starts_with("SW") => description.push_str("Switch"),
            Some('T') if footprint.reference.starts_with("TP") => description.push_str("Test Point"),
            Some('H') | Some('M') if footprint.reference.starts_with("MH") => description.push_str("Mounting Hole"),
            _ => description.push_str("Component"),
        }
        
        // Add value if available and meaningful
        if !footprint.value.is_empty() && footprint.value != footprint.reference {
            description.push_str(&format!(" - {}", footprint.value));
        }
        
        description
    }


    /// Manual refresh
    pub fn refresh(&mut self, logger: &ReactiveEventLogger) {
        if let Some(client) = &mut self.kicad_client {
            if let Some(runtime) = &self.runtime {
                match runtime.block_on(async {
                    Self::fetch_components(client).await
                }) {
                    Ok(components) => {
                        self.components = components;
                        self.last_update = std::time::Instant::now();
                        logger.log_info(&format!("Refreshed {} components from KiCad", self.components.len()));
                    }
                    Err(e) => {
                        logger.log_error(&format!("Failed to refresh components: {}", e));
                    }
                }
            }
        }
    }

    /// Get filtered components
    pub fn get_filtered_components(&self) -> Vec<&BomComponent> {
        if self.filter_text.is_empty() {
            self.components.iter().collect()
        } else {
            let filter_lower = self.filter_text.to_lowercase();
            self.components.iter().filter(|comp| {
                comp.reference.to_lowercase().contains(&filter_lower) ||
                comp.description.to_lowercase().contains(&filter_lower) ||
                comp.value.to_lowercase().contains(&filter_lower) ||
                comp.footprint.to_lowercase().contains(&filter_lower)
            }).collect()
        }
    }
}

/// Show the BOM panel
pub fn show_bom_panel(
    ui: &mut egui::Ui,
    app: &mut DemoLensApp,
    logger_state: &Dynamic<ReactiveEventLoggerState>,
    log_colors: &Dynamic<LogColors>,
) {
    let logger = ReactiveEventLogger::with_colors(logger_state, log_colors);
    
    // Initialize BOM state if not already done
    if app.bom_state.is_none() {
        let mut bom_state = BomPanelState::new();
        bom_state.initialize(&logger);
        app.bom_state = Some(bom_state);
    }
    
    if let Some(bom_state) = &mut app.bom_state {
        // Auto-refresh if enabled
        bom_state.maybe_auto_refresh(&logger);
        
        // Connection status header
        ui.horizontal(|ui| {
            ui.heading("📊 Bill of Materials (BOM)");
            ui.separator();
            
            // Connection status indicator
            match &bom_state.connection_status {
                ConnectionStatus::Disconnected => {
                    ui.colored_label(egui::Color32::GRAY, "🔴 Disconnected");
                    if ui.button("Connect").clicked() {
                        bom_state.connect_to_kicad(&logger);
                    }
                }
                ConnectionStatus::Connecting => {
                    ui.colored_label(egui::Color32::YELLOW, "🟡 Connecting...");
                }
                ConnectionStatus::Connected => {
                    ui.colored_label(egui::Color32::GREEN, "🟢 Connected");
                    if ui.button("🔄 Refresh").clicked() {
                        bom_state.refresh(&logger);
                    }
                }
                ConnectionStatus::Error(msg) => {
                    ui.colored_label(egui::Color32::RED, &format!("🔴 Error: {}", msg));
                    if ui.button("Retry").clicked() {
                        bom_state.connect_to_kicad(&logger);
                    }
                }
            }
        });
        
        ui.separator();
        
        // Controls
        ui.horizontal(|ui| {
            ui.label("Filter:");
            ui.text_edit_singleline(&mut bom_state.filter_text);
            
            ui.separator();
            
            ui.checkbox(&mut bom_state.auto_refresh, "Auto-refresh");
            
            if bom_state.auto_refresh {
                ui.label("Interval:");
                let mut interval_ms = bom_state.refresh_interval.as_millis() as u64;
                if ui.add(egui::DragValue::new(&mut interval_ms)
                    .range(100..=10000)
                    .suffix(" ms")
                    .speed(100.0)
                    .min_decimals(0)
                    .max_decimals(0))
                    .changed() {
                    bom_state.refresh_interval = Duration::from_millis(interval_ms);
                }
            }
            
            ui.separator();
            
            // Component count
            let filtered_count = bom_state.get_filtered_components().len();
            let total_count = bom_state.components.len();
            ui.label(format!("Components: {}/{}", filtered_count, total_count));
        });
        
        // Last update time
        if matches!(bom_state.connection_status, ConnectionStatus::Connected) {
            let elapsed = bom_state.last_update.elapsed();
            ui.label(format!("Last updated: {:.1}s ago", elapsed.as_secs_f32()));
        }
        
        ui.separator();
        
        // BOM table
        show_bom_table(ui, bom_state, app.global_units_mils);
    }
}

/// Show the BOM table
fn show_bom_table(ui: &mut egui::Ui, bom_state: &BomPanelState, global_units_mils: bool) {
    let filtered_components = bom_state.get_filtered_components();
    
    if filtered_components.is_empty() {
        ui.centered_and_justified(|ui| {
            if bom_state.components.is_empty() {
                ui.label("No components available. Make sure KiCad is running with a PCB open.");
            } else {
                ui.label("No components match the current filter.");
            }
        });
        return;
    }
    
    // Create table using egui_extras::TableBuilder
    TableBuilder::new(ui)
        .striped(true)
        .cell_layout(egui::Layout::left_to_right(egui::Align::Center))
        .column(egui_extras::Column::exact(60.0))    // Item
        .column(egui_extras::Column::exact(80.0))    // Reference
        .column(egui_extras::Column::remainder())    // Description
        .column(egui_extras::Column::exact(80.0))    // X Location
        .column(egui_extras::Column::exact(80.0))    // Y Location
        .column(egui_extras::Column::exact(80.0))    // Orientation
        .column(egui_extras::Column::exact(100.0))   // Value
        .column(egui_extras::Column::remainder())    // Footprint
        .header(20.0, |mut header| {
            header.col(|ui| {
                ui.strong("Item");
            });
            header.col(|ui| {
                ui.strong("Reference");
            });
            header.col(|ui| {
                ui.strong("Description");
            });
            header.col(|ui| {
                ui.strong("X (mm)");
            });
            header.col(|ui| {
                ui.strong("Y (mm)");
            });
            header.col(|ui| {
                ui.strong("Rotation (°)");
            });
            header.col(|ui| {
                ui.strong("Value");
            });
            header.col(|ui| {
                ui.strong("Footprint");
            });
        })
        .body(|mut body| {
            for component in filtered_components {
                body.row(18.0, |mut row| {
                    row.col(|ui| {
                        ui.label(&component.item_number);
                    });
                    row.col(|ui| {
                        ui.label(&component.reference);
                    });
                    row.col(|ui| {
                        ui.label(&component.description);
                    });
                    row.col(|ui| {
                        let x_text = if global_units_mils {
                            format!("{:.0}", component.x_location / 0.0254)
                        } else {
                            format!("{:.2}", component.x_location)
                        };
                        ui.label(x_text);
                    });
                    row.col(|ui| {
                        let y_text = if global_units_mils {
                            format!("{:.0}", component.y_location / 0.0254)
                        } else {
                            format!("{:.2}", component.y_location)
                        };
                        ui.label(y_text);
                    });
                    row.col(|ui| {
                        ui.label(format!("{:.1}", component.orientation));
                    });
                    row.col(|ui| {
                        ui.label(&component.value);
                    });
                    row.col(|ui| {
                        ui.label(&component.footprint);
                    });
                });
            }
        });
}