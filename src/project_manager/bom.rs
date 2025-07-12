/// BOM (Bill of Materials) data structures for project management
/// 
/// This module contains the core data structures for managing BOM components
/// that were previously embedded in the UI layer.

use serde::{Serialize, Deserialize};
use std::time::Duration;

/// Component data for the BOM table
/// This represents a single component in the Bill of Materials
#[derive(Clone, Debug, Serialize, Deserialize)]
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

/// Events sent from UI to BOM backend
#[derive(Debug, Clone)]
pub enum BomEvent {
    Connect,
    Disconnect,
    Refresh,
    UpdateRefreshInterval(Duration),
    SetAutoRefresh(bool),
}

/// Events sent from BOM backend to UI
#[derive(Debug, Clone)]
pub enum BomBackendEvent {
    ConnectionStatus(ConnectionStatus),
    ComponentsUpdated(Vec<BomComponent>),
    Error(String),
    Info(String),
}

/// Connection status for the BOM data source
#[derive(Clone, Debug)]
pub enum ConnectionStatus {
    Disconnected,
    Connecting,
    Connected,
    Error(String),
}

impl BomComponent {
    /// Create a new BOM component with default values
    pub fn new(reference: String) -> Self {
        Self {
            item_number: String::new(),
            reference,
            description: String::new(),
            x_location: 0.0,
            y_location: 0.0,
            orientation: 0.0,
            value: String::new(),
            footprint: String::new(),
        }
    }
    
    /// Get the component's position as a tuple
    pub fn position(&self) -> (f64, f64) {
        (self.x_location, self.y_location)
    }
    
    /// Check if this component matches a filter string
    pub fn matches_filter(&self, filter: &str) -> bool {
        if filter.is_empty() {
            return true;
        }
        
        let filter_lower = filter.to_lowercase();
        self.reference.to_lowercase().contains(&filter_lower) ||
        self.description.to_lowercase().contains(&filter_lower) ||
        self.value.to_lowercase().contains(&filter_lower) ||
        self.footprint.to_lowercase().contains(&filter_lower)
    }
}

impl ConnectionStatus {
    /// Check if the connection is in a working state
    pub fn is_connected(&self) -> bool {
        matches!(self, ConnectionStatus::Connected)
    }
    
    /// Check if there's an error state
    pub fn is_error(&self) -> bool {
        matches!(self, ConnectionStatus::Error(_))
    }
    
    /// Get a human-readable status string
    pub fn status_text(&self) -> &str {
        match self {
            ConnectionStatus::Disconnected => "Disconnected",
            ConnectionStatus::Connecting => "Connecting...",
            ConnectionStatus::Connected => "Connected",
            ConnectionStatus::Error(_) => "Error",
        }
    }
}