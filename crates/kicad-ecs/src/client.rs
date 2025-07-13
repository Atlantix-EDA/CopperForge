//! Real KiCad API client implementation
//! 
//! This module provides a working interface to KiCad's IPC API using the same approach as kicad-rs.

use eyre::{Result, WrapErr};
use nng::{Protocol, Socket};
use protobuf::{EnumOrUnknown, Message, MessageFull};
use protobuf::well_known_types::any::Any;
use rand::distributions::{Alphanumeric, DistString};
use std::env;
use thiserror::Error;
use tracing::{debug, error, info, instrument, warn};

// Include generated protobuf modules (same as kicad-rs approach)
mod protos {
    include!(concat!(env!("OUT_DIR"), "/proto/mod.rs"));
}

// Re-export the types we need
use protos::base_commands::*;
use protos::base_types::*;
use protos::editor_commands::*;
use protos::envelope::*;
use protos::board_types::*;
use protos::enums::*;

#[derive(Error, Debug)]
pub enum KiCadError {
    #[error("Connection failed: {0}")]
    ConnectionFailed(#[from] nng::Error),
    
    #[error("Protocol error: {0}")]
    ProtocolError(#[from] protobuf::Error),
    
    #[error("API error: {0}")]
    ApiError(String),
    
    #[error("No board open")]
    NoBoardOpen,
}

/// Configuration for connecting to KiCad
#[derive(Debug, Clone)]
pub struct KiCadConnectionConfig {
    /// IPC socket path - uses platform default if not specified
    pub socket_path: String,
    
    /// Client identifier - randomly generated if not specified  
    pub client_name: String,
    
    /// KiCad instance token for session validation
    pub kicad_token: String,
}

impl Default for KiCadConnectionConfig {
    fn default() -> Self {
        let socket_path = match env::consts::OS {
            "windows" => {
                format!(
                    "ipc://{}\\kicad\\api.sock",
                    env::temp_dir().to_str().unwrap()
                )
            }
            _ => String::from("ipc:///tmp/kicad/api.sock"),
        };

        let mut client_name: String = Alphanumeric.sample_string(&mut rand::thread_rng(), 8);
        client_name.insert_str(0, "kicad-ecs-");

        Self {
            socket_path,
            client_name,
            kicad_token: String::new(),
        }
    }
}

/// Main client for communicating with KiCad
pub struct KiCadClient {
    socket: Socket,
    config: KiCadConnectionConfig,
}

impl KiCadClient {
    /// Connect to KiCad using default configuration
    #[instrument]
    pub fn connect() -> Result<Self> {
        info!("Connecting to KiCad with default configuration");
        Self::connect_with_config(KiCadConnectionConfig::default())
    }
    
    /// Connect to KiCad with custom configuration
    #[instrument(skip(config), fields(socket_path = %config.socket_path, client_name = %config.client_name))]
    pub fn connect_with_config(config: KiCadConnectionConfig) -> Result<Self> {
        debug!("Creating nng socket");
        let socket = Socket::new(Protocol::Req0)
            .map_err(|e| {
                error!("Failed to create socket: {}", e);
                e
            })?;
            
        info!("Connecting to KiCad API at {}", config.socket_path);
        socket.dial(&config.socket_path)
            .wrap_err_with(|| {
                error!("Failed to connect to KiCad at {}", config.socket_path);
                format!("Failed to connect to KiCad at {}", config.socket_path)
            })?;
        
        info!("Successfully connected to KiCad API");
        
        // Test connection by getting version
        let mut client = Self { socket, config };
        let version = client.get_version_sync()?;
        info!("Connected to KiCad version: {}", version.full);
        
        Ok(client)
    }
    
    /// Send envelope and receive response
    fn send_envelope(&mut self, req: ApiRequest) -> Result<ApiResponse, KiCadError> {
        let bytes = req.write_to_bytes()?;
        self.socket.send(&bytes).map_err(|e| match e {
            (_, err) => KiCadError::ConnectionFailed(err),
        })?;
        let msg = self.socket.recv().map_err(|err| {
            KiCadError::ConnectionFailed(err)
        })?;
        let response = ApiResponse::parse_from_bytes(msg.as_slice())?;

        match response.status.status.enum_value_or_default() {
            ApiStatusCode::AS_OK => {
                if self.config.kicad_token.is_empty() {
                    self.config.kicad_token = String::from(&response.header.kicad_token);
                }
                Ok(response)
            }
            _ => Err(KiCadError::ApiError(format!(
                "KiCad API returned error: {}",
                response.status.error_message
            ))),
        }
    }

    /// Send typed request and receive typed response
    fn send_request<T: MessageFull, U: MessageFull>(&mut self, message: T) -> Result<U, KiCadError> {
        let mut req = ApiRequest::new();

        req.header = Some(ApiRequestHeader::new()).into();
        let header = req.header.as_mut().unwrap();

        header.client_name = self.config.client_name.clone();
        header.kicad_token = self.config.kicad_token.clone();

        req.message = Some(Any::pack(&message)?).into();
        let rep = self.send_envelope(req)?;
        let message = Any::unpack::<U>(rep.message.get_or_default())?;
        match message {
            Some(message) => Ok(message),
            None => Err(KiCadError::ApiError(format!(
                "could not unpack {} from API response",
                U::descriptor().name()
            ))),
        }
    }
    
    /// Get KiCad version information (synchronous)
    #[instrument(skip(self))]
    fn get_version_sync(&mut self) -> Result<KiCadVersionInfo> {
        info!("Requesting KiCad version information");
        
        let reply: GetVersionResponse = self.send_request(GetVersion::new())?;
        Ok(KiCadVersionInfo::from(reply.version.get_or_default()))
    }
    
    /// Get KiCad version information
    #[instrument(skip(self))]
    pub async fn get_version(&mut self) -> Result<KiCadVersionInfo> {
        // For now, just call the sync version since nng is sync
        self.get_version_sync()
    }
    
    /// Get list of open documents
    #[instrument(skip(self))]
    pub async fn get_open_documents(&mut self) -> Result<Vec<DocumentSpecifier>> {
        info!("Requesting list of open documents");
        
        let mut request = GetOpenDocuments::new();
        request.type_ = EnumOrUnknown::from(DocumentType::DOCTYPE_PCB);
        
        let response: GetOpenDocumentsResponse = self.send_request(request)?;
        Ok(response.documents)
    }
    
    /// Get the currently open board data
    #[instrument(skip(self))]
    pub async fn get_board(&mut self) -> Result<BoardData> {
        info!("Requesting board data");
        
        let docs = self.get_open_documents().await?;
        let doc = docs.first().ok_or(KiCadError::NoBoardOpen)?;
        
        Ok(BoardData {
            name: doc.identifier.as_ref().map(|id| match id {
                protos::base_types::document_specifier::Identifier::BoardFilename(f) => f.clone(),
                protos::base_types::document_specifier::Identifier::LibId(lib_id) => lib_id.to_string(),
                protos::base_types::document_specifier::Identifier::SheetPath(path) => format!("{:?}", path),
            }).unwrap_or_default(),
            project_name: doc.project.as_ref().map(|p| p.name.clone()),
            document: doc.clone(),
        })
    }
    
    /// Get all footprints from the current board
    #[instrument(skip(self))]
    pub async fn get_footprints(&mut self) -> Result<Vec<FootprintData>> {
        info!("Requesting footprint data");
        
        let docs = self.get_open_documents().await?;
        let doc = docs.first().ok_or(KiCadError::NoBoardOpen)?;
        
        // Get items from the board
        let mut request = GetItems::new();
        request.header = Some(ItemHeader::new()).into();
        let header = request.header.as_mut().unwrap();
        header.document = Some(doc.clone()).into();
        
        request.types.push(KiCadObjectType::KOT_PCB_FOOTPRINT.into());
        
        let response: GetItemsResponse = self.send_request(request)?;
        
        let mut footprints = Vec::new();
        for item in response.items {
            if let Ok(footprint) = self.unpack_footprint(&item) {
                footprints.push(footprint);
            }
        }
        
        info!("Retrieved {} footprints", footprints.len());
        Ok(footprints)
    }
    
    /// Unpack a footprint from an Any message
    fn unpack_footprint(&self, any: &Any) -> Result<FootprintData> {
        let footprint_instance: FootprintInstance = Any::unpack(any)?
            .ok_or_else(|| KiCadError::ApiError("Failed to unpack footprint".to_string()))?;
        
        let position = footprint_instance.position.get_or_default();
        let orientation = footprint_instance.orientation.get_or_default();
        let attributes = footprint_instance.attributes.get_or_default();
        
        // Extract reference text using proper field navigation
        let reference = footprint_instance.reference_field.as_ref()
            .and_then(|field| field.text.as_ref())
            .and_then(|board_text| board_text.text.as_ref())
            .map(|text| text.text.clone())
            .unwrap_or_else(|| format!("REF_{}", footprint_instance.id.get_or_default().value.get(..6).unwrap_or("")));
        
        // Extract value text using proper field navigation
        let value = footprint_instance.value_field.as_ref()
            .and_then(|field| field.text.as_ref())
            .and_then(|board_text| board_text.text.as_ref())
            .map(|text| text.text.clone())
            .unwrap_or_else(|| "UNKNOWN".to_string());
        
        // Extract description if available
        let description = footprint_instance.description_field.as_ref()
            .and_then(|field| field.text.as_ref())
            .and_then(|board_text| board_text.text.as_ref())
            .map(|text| text.text.clone())
            .filter(|s| !s.is_empty());

        Ok(FootprintData {
            id: footprint_instance.id.get_or_default().value.clone(),
            reference,
            value,
            footprint_name: footprint_instance.definition.get_or_default().id.get_or_default().entry_name.clone(),
            position: (
                position.x_nm as f64 / 1_000_000.0, // Convert nm to mm
                position.y_nm as f64 / 1_000_000.0,
            ),
            rotation: orientation.value_degrees,
            layer: self.layer_to_string(footprint_instance.layer.enum_value_or_default()),
            description,
            exclude_from_bom: attributes.exclude_from_bill_of_materials,
            do_not_populate: attributes.do_not_populate,
            locked: footprint_instance.locked.enum_value_or_default() == LockedState::LS_LOCKED,
        })
    }
    
    /// Convert board layer enum to string
    fn layer_to_string(&self, layer: BoardLayer) -> String {
        match layer {
            BoardLayer::BL_F_Cu => "F.Cu".to_string(),
            BoardLayer::BL_B_Cu => "B.Cu".to_string(),
            _ => format!("{:?}", layer),
        }
    }
}

/// KiCad version information
#[derive(Debug, Clone)]
pub struct KiCadVersionInfo {
    pub major: u32,
    pub minor: u32,
    pub patch: u32,
    pub full: String,
}

impl From<&KiCadVersion> for KiCadVersionInfo {
    fn from(v: &KiCadVersion) -> Self {
        Self {
            major: v.major,
            minor: v.minor,
            patch: v.patch,
            full: v.full_version.clone(),
        }
    }
}

/// Board data from KiCad
#[derive(Debug, Clone)]
pub struct BoardData {
    pub name: String,
    pub project_name: Option<String>,
    pub document: DocumentSpecifier,
}

/// Footprint data from KiCad
#[derive(Debug, Clone)]
pub struct FootprintData {
    pub id: String,
    pub reference: String,
    pub value: String,
    pub footprint_name: String,
    pub position: (f64, f64),  // x, y in millimeters
    pub rotation: f64,         // degrees
    pub layer: String,
    pub description: Option<String>,
    pub exclude_from_bom: bool,
    pub do_not_populate: bool,
    pub locked: bool,
}

// Coordinate conversion utilities (KiCad uses nanometers internally)
pub fn to_mm(nanometers: i64) -> f64 {
    nanometers as f64 / 1_000_000.0
}

pub fn from_mm(millimeters: f64) -> i64 {
    (millimeters * 1_000_000.0) as i64
}