use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use egui_file_dialog::FileDialog;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ProjectState {
    /// No project loaded
    NoProject,

    /// PCB file selected but gerbers not generated
    PcbSelected {
        pcb_path: PathBuf,
    },

    /// Gerbers are being generated
    GeneratingGerbers {
        pcb_path: PathBuf,
    },

    /// Gerbers generated but not loaded
    GerbersGenerated {
        pcb_path: PathBuf,
        gerber_dir: PathBuf,
    },

    /// Loading gerbers into viewer
    LoadingGerbers {
        pcb_path: PathBuf,
        gerber_dir: PathBuf,
    },

    /// Project fully loaded and ready
    Ready {
        pcb_path: PathBuf,
        gerber_dir: PathBuf,
        last_modified: std::time::SystemTime,
    },
}

impl ProjectState {
    /// Current PCB path if the state carries one.
    pub fn pcb_path(&self) -> Option<&std::path::Path> {
        match self {
            ProjectState::NoProject => None,
            ProjectState::PcbSelected { pcb_path }
            | ProjectState::GeneratingGerbers { pcb_path }
            | ProjectState::GerbersGenerated { pcb_path, .. }
            | ProjectState::LoadingGerbers { pcb_path, .. }
            | ProjectState::Ready { pcb_path, .. } => Some(pcb_path.as_path()),
        }
    }

    /// Current gerber output directory if the state carries one.
    pub fn gerber_dir(&self) -> Option<&std::path::Path> {
        match self {
            ProjectState::GerbersGenerated { gerber_dir, .. }
            | ProjectState::LoadingGerbers { gerber_dir, .. }
            | ProjectState::Ready { gerber_dir, .. } => Some(gerber_dir.as_path()),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectConfig {
    pub state: ProjectState,
    pub user_timezone: Option<String>,
    pub use_24_hour_clock: bool,
    pub global_units_mils: bool, // true = mils, false = mm
    #[serde(default)]
    pub preferred_projects_directory: Option<PathBuf>, // Preferred location for PCB projects
    #[serde(default = "default_author")]
    pub default_author: String,
    #[serde(default = "default_company")]
    pub default_company: String,
    #[serde(default = "default_true")]
    pub include_kiverse: bool,
    #[serde(default = "default_true")]
    pub include_atlantix_resistors: bool,
    /// Explicit kicad-cli pick. If `Some`, overrides auto-discovery — value is
    /// a method key (`"path"`, `"path-nightly"`, `"flatpak"`, `"snap"`) or
    /// `"custom:/abs/path/to/kicad-cli"`. Set via the KiCad info modal.
    #[serde(default)]
    pub kicad_cli_override: Option<String>,
}

fn default_author() -> String {
    "Nichola Tesla-Maxwell".to_string()
}

fn default_company() -> String {
    "Starfleet".to_string()
}

fn default_true() -> bool {
    true
}

impl Default for ProjectConfig {
    fn default() -> Self {
        Self {
            state: ProjectState::NoProject,
            user_timezone: None,
            use_24_hour_clock: false, // Default to 12-hour
            global_units_mils: false, // Default to mm
            preferred_projects_directory: None, // Will use home directory if not set
            default_author: default_author(),
            default_company: default_company(),
            include_kiverse: true,
            include_atlantix_resistors: true,
            kicad_cli_override: None,
        }
    }
}

impl ProjectConfig {
    pub fn save_to_file(&self, path: &PathBuf) -> Result<(), Box<dyn std::error::Error>> {
        let json = serde_json::to_string_pretty(self)?;
        std::fs::create_dir_all(path.parent().unwrap())?;
        std::fs::write(path.join("project_config.json"), json)?;
        Ok(())
    }
    
    pub fn load_from_file(path: &PathBuf) -> Result<Self, Box<dyn std::error::Error>> {
        let json_path = path.join("project_config.json");
        if json_path.exists() {
            let json = std::fs::read_to_string(json_path)?;
            let config: ProjectConfig = serde_json::from_str(&json)?;
            Ok(config)
        } else {
            Ok(ProjectConfig::default())
        }
    }
}

/// Manager for all project-related functionality
pub struct ProjectManager {
    /// Current project state
    pub state: ProjectState,

    /// File dialog for project selection
    pub file_dialog: FileDialog,

    /// Last file picked (to avoid re-processing)
    pub last_picked_file: Option<PathBuf>,

    /// Full config for persistence
    pub config: ProjectConfig,
}

impl ProjectManager {
    /// Create a new ProjectManager
    pub fn new() -> Self {
        let config = ProjectConfig::default();
        Self {
            state: config.state.clone(),
            file_dialog: FileDialog::new(),
            last_picked_file: None,
            config,
        }
    }

    /// Create from a ProjectConfig
    pub fn from_config(config: ProjectConfig) -> Self {
        Self {
            state: config.state.clone(),
            file_dialog: FileDialog::new(),
            last_picked_file: None,
            config,
        }
    }
    
    /// Convert to ProjectConfig for saving
    pub fn to_config(&self) -> ProjectConfig {
        self.config.clone()
    }
    
    /// Save project configuration to disk
    pub fn save_to_file(&self, path: &PathBuf) -> Result<(), Box<dyn std::error::Error>> {
        self.to_config().save_to_file(path)
    }
    
    /// Load project configuration from disk
    pub fn load_from_file(path: &PathBuf) -> Result<Self, Box<dyn std::error::Error>> {
        let config = ProjectConfig::load_from_file(path)?;
        Ok(Self::from_config(config))
    }
    
    /// Check if a PCB file is selected
    pub fn has_pcb_selected(&self) -> bool {
        !matches!(self.state, ProjectState::NoProject)
    }
    
    /// Get the current PCB path if available
    pub fn get_pcb_path(&self) -> Option<&PathBuf> {
        match &self.state {
            ProjectState::NoProject => None,
            ProjectState::PcbSelected { pcb_path } |
            ProjectState::GeneratingGerbers { pcb_path } |
            ProjectState::GerbersGenerated { pcb_path, .. } |
            ProjectState::LoadingGerbers { pcb_path, .. } |
            ProjectState::Ready { pcb_path, .. } => Some(pcb_path),
        }
    }
    
    /// Get the current gerber directory if available
    pub fn get_gerber_dir(&self) -> Option<&PathBuf> {
        match &self.state {
            ProjectState::NoProject |
            ProjectState::PcbSelected { .. } |
            ProjectState::GeneratingGerbers { .. } => None,
            ProjectState::GerbersGenerated { gerber_dir, .. } |
            ProjectState::LoadingGerbers { gerber_dir, .. } |
            ProjectState::Ready { gerber_dir, .. } => Some(gerber_dir),
        }
    }
    
    /// Update the file dialog and check for newly selected files
    pub fn update_file_dialog(&mut self, ctx: &egui::Context) -> Option<PathBuf> {
        if let Some(path) = self.file_dialog.update(ctx).picked() {
            let path_buf = path.to_path_buf();
            
            if self.last_picked_file.as_ref() != Some(&path_buf) {
                self.last_picked_file = Some(path_buf.clone());
                
                if path.extension().and_then(|s| s.to_str()) == Some("kicad_pcb") {
                    self.state = ProjectState::PcbSelected { pcb_path: path_buf.clone() };
                    return Some(path_buf);
                }
            }
        }
        None
    }
    
    /// Open the file dialog for PCB selection
    pub fn open_file_dialog(&mut self) {
        self.file_dialog.pick_file();
    }
    
    /// Verify the on-disk artifacts referenced by the current state still exist.
    /// Gerber generation and loading are now driven by explicit user action in
    /// the Gerber Viewer ribbon, so this function only prunes stale state.
    pub fn manage_project_state(&mut self) {
        match &self.state {
            ProjectState::NoProject
            | ProjectState::GeneratingGerbers { .. }
            | ProjectState::LoadingGerbers { .. } => {}
            ProjectState::PcbSelected { pcb_path } => {
                if !pcb_path.exists() {
                    self.state = ProjectState::NoProject;
                }
            }
            ProjectState::GerbersGenerated { pcb_path, gerber_dir }
            | ProjectState::Ready { pcb_path, gerber_dir, .. } => {
                if !pcb_path.exists() || !gerber_dir.exists() {
                    self.state = ProjectState::NoProject;
                }
            }
        }
    }
}