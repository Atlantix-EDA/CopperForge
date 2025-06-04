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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectConfig {
    pub state: ProjectState,
    pub auto_generate_on_startup: bool,
    pub auto_reload_on_change: bool,
}

impl Default for ProjectConfig {
    fn default() -> Self {
        Self {
            state: ProjectState::NoProject,
            auto_generate_on_startup: true,
            auto_reload_on_change: true,
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
    
    /// Auto-generate gerbers when project is loaded
    pub auto_generate_on_startup: bool,
    
    /// Auto-reload when files change
    pub auto_reload_on_change: bool,
    
    /// File dialog for project selection
    pub file_dialog: FileDialog,
    
    /// Last file picked (to avoid re-processing)
    pub last_picked_file: Option<PathBuf>,
}

impl ProjectManager {
    /// Create a new ProjectManager
    pub fn new() -> Self {
        Self {
            state: ProjectState::NoProject,
            auto_generate_on_startup: true,
            auto_reload_on_change: true,
            file_dialog: FileDialog::new(),
            last_picked_file: None,
        }
    }
    
    /// Create from a ProjectConfig
    pub fn from_config(config: ProjectConfig) -> Self {
        Self {
            state: config.state,
            auto_generate_on_startup: config.auto_generate_on_startup,
            auto_reload_on_change: config.auto_reload_on_change,
            file_dialog: FileDialog::new(),
            last_picked_file: None,
        }
    }
    
    /// Convert to ProjectConfig for saving
    pub fn to_config(&self) -> ProjectConfig {
        ProjectConfig {
            state: self.state.clone(),
            auto_generate_on_startup: self.auto_generate_on_startup,
            auto_reload_on_change: self.auto_reload_on_change,
        }
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
}