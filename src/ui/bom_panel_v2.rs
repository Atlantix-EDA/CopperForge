#![allow(dead_code)]
use crate::DemoLensApp;
use egui_lens::{ReactiveEventLogger, ReactiveEventLoggerState};
use egui_lens::LogColors;
use egui_mobius_reactive::*;
use egui_mobius::factory;
use egui_mobius::signals::Signal;
use egui_mobius::slot::Slot;
use egui_mobius::types::Value;
use egui_extras::TableBuilder;
use kicad_ecs::prelude::*;
use kicad_ecs::client::{KiCadClient, FootprintData};
use std::time::Duration;
use std::sync::Arc;

/// Component data for the BOM table
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
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

/// Events sent from UI to backend
#[derive(Debug, Clone)]
pub enum BomEvent {
    Connect,
    Disconnect,
    Refresh,
    UpdateRefreshInterval(Duration),
    SetAutoRefresh(bool),
}

/// Events sent from backend to UI
#[derive(Debug, Clone)]
pub enum BomBackendEvent {
    ConnectionStatus(ConnectionStatus),
    ComponentsUpdated(Vec<BomComponent>),
    Error(String),
    Info(String),
}

#[derive(Clone, Debug)]
pub enum ConnectionStatus {
    Disconnected,
    Connecting,
    Connected,
    Error(String),
}

/// BOM panel state using egui_mobius patterns
pub struct BomPanelState {
    // Shared state using Value<T>
    pub components: Value<Vec<BomComponent>>,
    pub connection_status: Value<ConnectionStatus>,
    pub last_update: Value<std::time::Instant>,
    pub auto_refresh: Value<bool>,
    pub refresh_interval: Value<Duration>,
    pub filter_text: Value<String>,
    
    // Signal/Slot communication
    pub signal_to_backend: Signal<BomEvent>,
    pub slot_from_backend: Slot<BomBackendEvent>,
    
    // Update tracking
    pub update_needed: Value<bool>,
    
    // Store last backend events for UI processing
    pub last_error: Value<Option<String>>,
    pub last_info: Value<Option<String>>,
}

impl BomPanelState {
    pub fn new() -> (Self, Slot<BomEvent>, Signal<BomBackendEvent>) {
        // Create signal/slot pairs
        let (signal_to_backend, slot_to_backend) = factory::create_signal_slot::<BomEvent>();
        let (signal_from_backend, mut slot_from_backend) = factory::create_signal_slot::<BomBackendEvent>();
        
        // Create shared state values
        let components = Value::new(Vec::new());
        let connection_status = Value::new(ConnectionStatus::Disconnected);
        let last_update = Value::new(std::time::Instant::now());
        let update_needed = Value::new(false);
        let last_error = Value::new(None);
        let last_info = Value::new(None);
        
        // Setup slot processing with cloned values
        let components_clone = components.clone();
        let connection_status_clone = connection_status.clone();
        let last_update_clone = last_update.clone();
        let update_needed_clone = update_needed.clone();
        let last_error_clone = last_error.clone();
        let last_info_clone = last_info.clone();
        
        // Start the slot processing immediately - it will run in the background
        slot_from_backend.start(move |event: BomBackendEvent| {
            match event {
                BomBackendEvent::ConnectionStatus(status) => {
                    *connection_status_clone.lock().unwrap() = status;
                    *update_needed_clone.lock().unwrap() = true;
                }
                BomBackendEvent::ComponentsUpdated(new_components) => {
                    // Update BOM components in shared state
                    *components_clone.lock().unwrap() = new_components;
                    *last_update_clone.lock().unwrap() = std::time::Instant::now();
                    *update_needed_clone.lock().unwrap() = true;
                }
                BomBackendEvent::Error(msg) => {
                    // Store error for UI thread to process
                    *last_error_clone.lock().unwrap() = Some(msg);
                    *update_needed_clone.lock().unwrap() = true;
                }
                BomBackendEvent::Info(msg) => {
                    // Store info for UI thread to process
                    *last_info_clone.lock().unwrap() = Some(msg);
                    *update_needed_clone.lock().unwrap() = true;
                }
            }
        });
        
        let state = Self {
            components,
            connection_status,
            last_update,
            auto_refresh: Value::new(false),
            refresh_interval: Value::new(Duration::from_millis(5000)),
            filter_text: Value::new(String::new()),
            signal_to_backend,
            slot_from_backend,
            update_needed,
            last_error,
            last_info,
        };
        
        (state, slot_to_backend, signal_from_backend)
    }
    
    /// Get filtered component count without cloning
    pub fn get_filtered_count(&self) -> (usize, usize) {
        let components = self.components.lock().unwrap();
        let filter_text = self.filter_text.lock().unwrap();
        
        let total = components.len();
        if filter_text.is_empty() {
            (total, total)
        } else {
            let filter_lower = filter_text.to_lowercase();
            let filtered = components.iter().filter(|comp| {
                comp.reference.to_lowercase().contains(&filter_lower) ||
                comp.description.to_lowercase().contains(&filter_lower) ||
                comp.value.to_lowercase().contains(&filter_lower) ||
                comp.footprint.to_lowercase().contains(&filter_lower)
            }).count();
            (filtered, total)
        }
    }
}

/// Backend thread for KiCad communication
pub fn bom_backend_thread(
    mut slot_from_ui: Slot<BomEvent>,
    signal_to_ui: Signal<BomBackendEvent>,
    auto_refresh: Value<bool>,
    refresh_interval: Value<Duration>,
) {
    // No runtime needed - we'll handle async operations differently
    
    // TODO: Replace with actual KiCad client when sync implementation is ready
    let connected: Arc<std::sync::Mutex<bool>> = Arc::new(std::sync::Mutex::new(false));
    let last_refresh: Arc<std::sync::Mutex<std::time::Instant>> = Arc::new(std::sync::Mutex::new(std::time::Instant::now()));
    
    let connected_clone = connected.clone();
    let last_refresh_clone = last_refresh.clone();
    let auto_refresh_clone = auto_refresh.clone();
    let refresh_interval_clone = refresh_interval.clone();
    
    // Start slot processing - NO runtime needed!
    slot_from_ui.start({
        let signal_to_ui = signal_to_ui.clone();
        
        move |event| {
            match event {
                BomEvent::Connect => {
                    signal_to_ui.send(BomBackendEvent::ConnectionStatus(ConnectionStatus::Connecting)).ok();
                    signal_to_ui.send(BomBackendEvent::Info("Attempting to connect to KiCad...".to_string())).ok();
                    
                    // Try real KiCad connection
                    match KiCadClient::connect() {
                        Ok(_client) => {
                            *connected_clone.lock().unwrap() = true;
                            signal_to_ui.send(BomBackendEvent::ConnectionStatus(ConnectionStatus::Connected)).ok();
                            signal_to_ui.send(BomBackendEvent::Info("Successfully connected to KiCad!".to_string())).ok();
                            
                            // Initial refresh with real data
                            fetch_real_kicad_components(&signal_to_ui);
                            *last_refresh_clone.lock().unwrap() = std::time::Instant::now();
                        }
                        Err(_) => {
                            let error_msg = format!("Connection failed: Make sure KiCad is running with a PCB open");
                            signal_to_ui.send(BomBackendEvent::ConnectionStatus(ConnectionStatus::Error(error_msg.clone()))).ok();
                            signal_to_ui.send(BomBackendEvent::Error(error_msg)).ok();
                        }
                    }
                }
                
                BomEvent::Disconnect => {
                    *connected_clone.lock().unwrap() = false;
                    signal_to_ui.send(BomBackendEvent::ConnectionStatus(ConnectionStatus::Disconnected)).ok();
                    signal_to_ui.send(BomBackendEvent::Info("Disconnected from KiCad".to_string())).ok();
                }
                
                BomEvent::Refresh => {
                    if *connected_clone.lock().unwrap() {
                        fetch_real_kicad_components(&signal_to_ui);
                        *last_refresh_clone.lock().unwrap() = std::time::Instant::now();
                    } else {
                        signal_to_ui.send(BomBackendEvent::Error("Not connected to KiCad".to_string())).ok();
                    }
                }
                
                BomEvent::UpdateRefreshInterval(interval) => {
                    *refresh_interval_clone.lock().unwrap() = interval;
                }
                
                BomEvent::SetAutoRefresh(enabled) => {
                    *auto_refresh_clone.lock().unwrap() = enabled;
                }
            }
        }
    });
    
    // Auto-refresh loop with adaptive sleep
    loop {
        let sleep_duration = {
            let auto = *auto_refresh.lock().unwrap();
            if !auto {
                // If auto-refresh is off, sleep longer to reduce CPU usage
                Duration::from_millis(1000)
            } else {
                let interval = *refresh_interval.lock().unwrap();
                let last_refresh_time = *last_refresh.lock().unwrap();
                let elapsed = last_refresh_time.elapsed();
                
                if elapsed >= interval {
                    // Time to refresh, minimal sleep
                    Duration::from_millis(10)
                } else {
                    // Calculate how long until next refresh and sleep for half that time
                    // This reduces CPU usage while still being responsive
                    let remaining = interval - elapsed;
                    remaining.min(Duration::from_millis(500)) / 2
                }
            }
        };
        
        std::thread::sleep(sleep_duration);
        
        let should_refresh = {
            let auto = *auto_refresh.lock().unwrap();
            let interval = *refresh_interval.lock().unwrap();
            let last_refresh_time = *last_refresh.lock().unwrap();
            let has_client = *connected.lock().unwrap();
            auto && last_refresh_time.elapsed() >= interval && has_client
        };
        
        if should_refresh {
            fetch_real_kicad_components(&signal_to_ui);
            *last_refresh.lock().unwrap() = std::time::Instant::now();
        }
    }
}

/// Connect to KiCad and fetch real components using BLOCKING operations only
fn fetch_real_kicad_components(signal_to_ui: &Signal<BomBackendEvent>) {
    // NO TOKIO! Use blocking operations only
    match try_fetch_components_blocking() {
        Ok(components) => {
            let count = components.len();
            signal_to_ui.send(BomBackendEvent::ComponentsUpdated(components)).ok();
            signal_to_ui.send(BomBackendEvent::Info(format!("Loaded {} components from KiCad", count))).ok();
        }
        Err(e) => {
            signal_to_ui.send(BomBackendEvent::Error(format!("Failed to fetch components: {}", e))).ok();
        }
    }
}

/// Blocking function to fetch components using std::sync approach
fn try_fetch_components_blocking() -> Result<Vec<BomComponent>, String> {
    use std::sync::mpsc;
    
    // Connect to KiCad
    let mut client = KiCadClient::connect().map_err(|e| format!("Connection failed: {}", e))?;
    
    // Use std::thread to handle async operations without tokio
    let (tx, rx) = mpsc::channel();
    
    std::thread::spawn(move || {
        // Create a simple async executor
        let rt = futures::executor::block_on(async {
            // Get board info
            let _board = client.get_board().await.map_err(|e| format!("Failed to get board: {}", e))?;
            
            // Get footprints  
            let footprints = client.get_footprints().await.map_err(|e| format!("Failed to get footprints: {}", e))?;
            
            // Convert to BOM components
            let mut components = Vec::new();
            for (idx, fp) in footprints.iter().enumerate() {
                let component = BomComponent {
                    item_number: format!("{:03}", idx + 1),
                    reference: fp.reference.clone(),
                    description: generate_description(fp),
                    x_location: fp.position.0,
                    y_location: fp.position.1,
                    orientation: fp.rotation,
                    value: fp.value.clone(),
                    footprint: fp.footprint_name.clone(),
                };
                components.push(component);
            }
            
            Ok::<Vec<BomComponent>, String>(components)
        });
        
        tx.send(rt).ok();
    });
    
    // Block until we get the result
    match rx.recv() {
        Ok(Ok(components)) => Ok(components),
        Ok(Err(e)) => Err(format!("Failed to fetch: {}", e)),
        Err(_) => Err("Thread communication failed".to_string()),
    }
}

/// Generate description for a component based on its reference
fn generate_description(footprint: &FootprintData) -> String {
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

// Real KiCad IPC implementation using kicad-ecs with minimal tokio runtime
// This follows the pattern from the real_kicad_ecs example

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
        let (bom_state, slot_to_backend, signal_from_backend) = BomPanelState::new();
        
        // Clone values for the backend thread
        let auto_refresh = bom_state.auto_refresh.clone();
        let refresh_interval = bom_state.refresh_interval.clone();
        
        // Start backend thread
        std::thread::spawn(move || {
            bom_backend_thread(slot_to_backend, signal_from_backend, auto_refresh, refresh_interval);
        });
        
        app.bom_state = Some(bom_state);
        
        // Check for pending BOM components loaded from a project
        if let Some(pending_components) = app.pending_bom_components.take() {
            if let Some(ref mut bom_state) = app.bom_state {
                let mut components = bom_state.components.lock().unwrap();
                *components = pending_components;
                *bom_state.last_update.lock().unwrap() = std::time::Instant::now();
                logger.log_info(&format!("Loaded {} pending BOM components from project", components.len()));
            }
        }
    }
    
    if let Some(bom_state) = &mut app.bom_state {
        // Check for and process any pending backend events
        let logger = ReactiveEventLogger::with_colors(&logger_state, &log_colors);
        
        // Process any error messages that need to be logged
        if let Some(error_msg) = bom_state.last_error.lock().unwrap().take() {
            logger.log_error(&error_msg);
        }
        
        // Process any info messages that need to be logged
        if let Some(info_msg) = bom_state.last_info.lock().unwrap().take() {
            logger.log_info(&info_msg);
        }
        
        // Connection status header
        ui.horizontal(|ui| {
            ui.heading("📊 Bill of Materials (BOM)");
            ui.separator();
            
            // Connection status indicator
            let connection_status = bom_state.connection_status.lock().unwrap().clone();
            match connection_status {
                ConnectionStatus::Disconnected => {
                    ui.colored_label(egui::Color32::GRAY, "🔴 Disconnected");
                    if ui.button("Connect").clicked() {
                        bom_state.signal_to_backend.send(BomEvent::Connect).ok();
                    }
                }
                ConnectionStatus::Connecting => {
                    ui.colored_label(egui::Color32::YELLOW, "🟡 Connecting...");
                }
                ConnectionStatus::Connected => {
                    ui.colored_label(egui::Color32::GREEN, "🟢 Connected");
                    if ui.button("🔄 Refresh").clicked() {
                        bom_state.signal_to_backend.send(BomEvent::Refresh).ok();
                    }
                }
                ConnectionStatus::Error(msg) => {
                    ui.colored_label(egui::Color32::RED, &format!("🔴 Error: {}", msg));
                    if ui.button("Retry").clicked() {
                        bom_state.signal_to_backend.send(BomEvent::Connect).ok();
                    }
                }
            }
        });
        
        ui.separator();
        
        // Controls
        ui.horizontal(|ui| {
            ui.label("Filter:");
            let mut filter_text = bom_state.filter_text.lock().unwrap();
            if ui.text_edit_singleline(&mut *filter_text).changed() {
                *bom_state.update_needed.lock().unwrap() = true;
            }
            drop(filter_text);
            
            ui.separator();
            
            let mut auto_refresh = bom_state.auto_refresh.lock().unwrap();
            if ui.checkbox(&mut *auto_refresh, "Auto-refresh").changed() {
                bom_state.signal_to_backend.send(BomEvent::SetAutoRefresh(*auto_refresh)).ok();
            }
            
            if *auto_refresh {
                ui.label("Interval:");
                let refresh_interval = bom_state.refresh_interval.lock().unwrap();
                let mut interval_ms = refresh_interval.as_millis() as u64;
                drop(refresh_interval);
                
                if ui.add(egui::DragValue::new(&mut interval_ms)
                    .range(100..=10000)
                    .suffix(" ms")
                    .speed(100.0)
                    .min_decimals(0)
                    .max_decimals(0))
                    .changed() {
                    bom_state.signal_to_backend.send(BomEvent::UpdateRefreshInterval(Duration::from_millis(interval_ms))).ok();
                }
            }
            
            ui.separator();
            
            // Component count (optimized - no cloning)
            let (filtered_count, total_count) = bom_state.get_filtered_count();
            ui.label(format!("Components: {}/{}", filtered_count, total_count));
        });
        
        // Last update time
        let connection_status = bom_state.connection_status.lock().unwrap().clone();
        if matches!(connection_status, ConnectionStatus::Connected) {
            let last_update = bom_state.last_update.lock().unwrap();
            let elapsed = last_update.elapsed();
            ui.label(format!("Last updated: {}s ago", elapsed.as_secs()));
        }
        
        ui.separator();
        
        // BOM table - render directly from locked data
        {
            let components = bom_state.components.lock().unwrap();
            let filter_text = bom_state.filter_text.lock().unwrap();
            show_bom_table_optimized(ui, &components, &filter_text, app.global_units_mils);
        }
        
        // Request repaint if needed
        if *bom_state.update_needed.lock().unwrap() {
            ui.ctx().request_repaint();
            *bom_state.update_needed.lock().unwrap() = false;
        }
    }
}

/// Show the BOM table - optimized version with virtual scrolling for large lists
fn show_bom_table_optimized(ui: &mut egui::Ui, components: &[BomComponent], filter_text: &str, global_units_mils: bool) {
    let filter_lower = filter_text.to_lowercase();
    let should_filter = !filter_text.is_empty();
    
    // Pre-filter components to avoid doing it twice
    let filtered_components: Vec<&BomComponent> = if should_filter {
        components.iter().filter(|comp| {
            comp.reference.to_lowercase().contains(&filter_lower) ||
            comp.description.to_lowercase().contains(&filter_lower) ||
            comp.value.to_lowercase().contains(&filter_lower) ||
            comp.footprint.to_lowercase().contains(&filter_lower)
        }).collect()
    } else {
        components.iter().collect()
    };
    
    if filtered_components.is_empty() {
        ui.centered_and_justified(|ui| {
            if components.is_empty() {
                ui.label("No components available. Make sure KiCad is running with a PCB open.");
            } else {
                ui.label("No components match the current filter.");
            }
        });
        return;
    }
    
    // Use virtual scrolling for large lists to improve performance
    let use_virtual_scrolling = filtered_components.len() > 100;
    
    if use_virtual_scrolling {
        // Virtual scrolling version for large lists
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
                header.col(|ui| { ui.strong("Item"); });
                header.col(|ui| { ui.strong("Reference"); });
                header.col(|ui| { ui.strong("Description"); });
                header.col(|ui| { ui.strong("X (mm)"); });
                header.col(|ui| { ui.strong("Y (mm)"); });
                header.col(|ui| { ui.strong("Rotation (°)"); });
                header.col(|ui| { ui.strong("Value"); });
                header.col(|ui| { ui.strong("Footprint"); });
            })
            .body(|body| {
                body.heterogeneous_rows(
                    filtered_components.iter().map(|_| 18.0),
                    |row| {
                        let row_index = row.index();
                        if let Some(component) = filtered_components.get(row_index) {
                            render_component_row(row, component, global_units_mils);
                        }
                    },
                );
            });
    } else {
        // Regular rendering for smaller lists
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
                header.col(|ui| { ui.strong("Item"); });
                header.col(|ui| { ui.strong("Reference"); });
                header.col(|ui| { ui.strong("Description"); });
                header.col(|ui| { ui.strong("X (mm)"); });
                header.col(|ui| { ui.strong("Y (mm)"); });
                header.col(|ui| { ui.strong("Rotation (°)"); });
                header.col(|ui| { ui.strong("Value"); });
                header.col(|ui| { ui.strong("Footprint"); });
            })
            .body(|mut body| {
                for component in filtered_components {
                    body.row(18.0, |row| {
                        render_component_row(row, component, global_units_mils);
                    });
                }
            });
    }
}

/// Render a single component row - extracted for reuse
fn render_component_row(mut row: egui_extras::TableRow, component: &BomComponent, global_units_mils: bool) {
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
}

/// Show the BOM table - legacy version
fn show_bom_table(ui: &mut egui::Ui, components: &[BomComponent], global_units_mils: bool) {
    if components.is_empty() {
        ui.centered_and_justified(|ui| {
            ui.label("No components available. Make sure KiCad is running with a PCB open.");
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
            for component in components {
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

/// ECS Component for PCB components (to integrate with KiForge's ECS)
#[derive(Component, Debug, Clone)]
pub struct PcbComponent {
    pub reference: String,
    pub value: String,
    pub footprint: String,
    pub description: String,
}

#[derive(Component, Debug, Clone)]
pub struct PcbPosition {
    pub x: f64,
    pub y: f64,
    pub rotation: f64,
}

/// Update ECS world with BOM components
pub fn update_ecs_with_bom_components(world: &mut World, components: &[BomComponent]) {
    // Clear existing PCB components
    let mut entities_to_remove = Vec::new();
    let mut query = world.query::<(Entity, &PcbComponent)>();
    for (entity, _) in query.iter(world) {
        entities_to_remove.push(entity);
    }
    
    for entity in entities_to_remove {
        world.despawn(entity);
    }
    
    // Add new components
    for component in components {
        world.spawn((
            PcbComponent {
                reference: component.reference.clone(),
                value: component.value.clone(),
                footprint: component.footprint.clone(),
                description: component.description.clone(),
            },
            PcbPosition {
                x: component.x_location,
                y: component.y_location,
                rotation: component.orientation,
            },
        ));
    }
}