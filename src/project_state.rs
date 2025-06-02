use serde::{Deserialize, Serialize};
use std::path::PathBuf;

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