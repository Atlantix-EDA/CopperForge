pub mod database;
pub mod bom;
pub mod kicad_project;

use database::{ProjectDatabase, ProjectData, ProjectMetadata, generate_project_id, ProjectDatabaseError};
use bom::BomComponent;
use std::path::{Path, PathBuf};
use chrono::Utc;

/// Project manager state
pub struct ProjectManagerState {
    pub database: Option<ProjectDatabase>,
    pub current_project: Option<ProjectData>,
    pub project_list: Vec<ProjectMetadata>,
    pub search_query: String,
    pub selected_project_id: Option<String>,
    pub show_create_dialog: bool,
    pub show_delete_confirmation: Option<String>,
    pub new_project_name: String,
    pub new_project_description: String,
    pub new_project_tags: String,
    pub new_project_pcb_path: Option<PathBuf>,
    pub show_pcb_file_dialog: bool,
    pub last_error: Option<String>,
    // New fields for creating KiCad projects from scratch
    pub create_new_kicad_project: bool,
    pub new_kicad_project_location: PathBuf,
    pub new_kicad_project_author: String,
    pub new_kicad_project_company: String,
    pub include_kiverse: bool,
    pub include_atlantix_resistors: bool,
    pub kiverse_path: PathBuf,
    pub show_location_dialog: bool,
}

impl Default for ProjectManagerState {
    fn default() -> Self {
        let home_dir = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
        Self {
            database: None,
            current_project: None,
            project_list: Vec::new(),
            search_query: String::new(),
            selected_project_id: None,
            show_create_dialog: false,
            show_delete_confirmation: None,
            new_project_name: String::new(),
            new_project_description: String::new(),
            new_project_tags: String::new(),
            new_project_pcb_path: None,
            show_pcb_file_dialog: false,
            last_error: None,
            create_new_kicad_project: true,  // Default to creating new projects
            new_kicad_project_location: home_dir.clone(),
            new_kicad_project_author: String::new(),
            new_kicad_project_company: String::new(),
            include_kiverse: true,
            include_atlantix_resistors: true,
            kiverse_path: home_dir.join(".kicad_libs/kiverse"),
            show_location_dialog: false,
        }
    }
}

impl ProjectManagerState {
    /// Initialize the project database
    pub fn initialize_database(&mut self, db_path: &Path) -> Result<(), ProjectDatabaseError> {
        let database = ProjectDatabase::new(db_path)?;
        self.project_list = database.list_projects()?;
        self.database = Some(database);
        Ok(())
    }

    /// Create a new project
    pub fn create_project(
        &mut self,
        name: String,
        description: String,
        pcb_file_path: PathBuf,
        tags: Vec<String>,
        bom_components: Vec<BomComponent>,
    ) -> Result<String, ProjectDatabaseError> {
        if let Some(ref database) = self.database {
            let project_id = generate_project_id();
            let now = Utc::now();
            
            let metadata = ProjectMetadata {
                id: project_id.clone(),
                name,
                description,
                pcb_file_path,
                created_at: now,
                last_modified: now,
                version: env!("CARGO_PKG_VERSION").to_string(),
                tags,
            };
            
            let project_data = ProjectData {
                metadata: metadata.clone(),
                bom_components,
                notes: String::new(),
            };
            
            database.save_project(&project_data)?;
            self.project_list = database.list_projects()?;
            self.current_project = Some(project_data);
            
            Ok(project_id)
        } else {
            Err(ProjectDatabaseError::DatabaseRead("Database not initialized".to_string()))
        }
    }

    /// Load a project
    pub fn load_project(&mut self, project_id: &str) -> Result<(), ProjectDatabaseError> {
        if let Some(ref database) = self.database {
            if let Some(project) = database.load_project(project_id)? {
                self.current_project = Some(project);
                self.selected_project_id = Some(project_id.to_string());
                Ok(())
            } else {
                Err(ProjectDatabaseError::DatabaseRead(format!("Project {} not found", project_id)))
            }
        } else {
            Err(ProjectDatabaseError::DatabaseRead("Database not initialized".to_string()))
        }
    }

    /// Delete a project
    pub fn delete_project(&mut self, project_id: &str) -> Result<(), ProjectDatabaseError> {
        if let Some(ref database) = self.database {
            database.delete_project(project_id)?;
            self.project_list = database.list_projects()?;
            
            // Clear current project if it was deleted
            if let Some(ref current) = self.current_project {
                if current.metadata.id == project_id {
                    self.current_project = None;
                    self.selected_project_id = None;
                }
            }
            
            Ok(())
        } else {
            Err(ProjectDatabaseError::DatabaseRead("Database not initialized".to_string()))
        }
    }

    /// Search projects
    pub fn search_projects(&mut self, query: &str) -> Result<(), ProjectDatabaseError> {
        if let Some(ref database) = self.database {
            self.project_list = if query.is_empty() {
                database.list_projects()?
            } else {
                database.search_projects(query)?
            };
            Ok(())
        } else {
            Err(ProjectDatabaseError::DatabaseRead("Database not initialized".to_string()))
        }
    }

    /// Update current project with new BOM data
    pub fn update_project_bom(&mut self, bom_components: Vec<BomComponent>) -> Result<(), ProjectDatabaseError> {
        if let Some(ref mut current_project) = self.current_project {
            if let Some(ref database) = self.database {
                current_project.bom_components = bom_components;
                current_project.metadata.last_modified = Utc::now();
                
                database.save_project(current_project)?;
                self.project_list = database.list_projects()?;
                
                Ok(())
            } else {
                Err(ProjectDatabaseError::DatabaseRead("Database not initialized".to_string()))
            }
        } else {
            Err(ProjectDatabaseError::DatabaseRead("No current project loaded".to_string()))
        }
    }

    /// Update project metadata
    pub fn update_project(&mut self, project_id: &str, name: String, description: String, tags: Vec<String>) -> Result<(), ProjectDatabaseError> {
        if let Some(ref database) = self.database {
            if let Some(mut project) = database.load_project(project_id)? {
                project.metadata.name = name;
                project.metadata.description = description;
                project.metadata.tags = tags;
                project.metadata.last_modified = chrono::Utc::now();
                
                database.save_project(&project)?;
                self.project_list = database.list_projects()?;
                
                // Update current project if it's the one being edited
                if let Some(ref current) = self.current_project {
                    if current.metadata.id == project_id {
                        self.current_project = Some(project);
                    }
                }
                
                Ok(())
            } else {
                Err(ProjectDatabaseError::DatabaseRead(format!("Project {} not found", project_id)))
            }
        } else {
            Err(ProjectDatabaseError::DatabaseRead("Database not initialized".to_string()))
        }
    }

    /// Create a new KiCad project from scratch
    pub fn create_new_kicad_project_from_scratch(
        &mut self,
        name: String,
        description: String,
        tags: Vec<String>,
    ) -> Result<String, ProjectDatabaseError> {
        use kicad_project::{NewKicadProjectInfo, create_kicad_project};

        // Create project info
        let mut project_info = NewKicadProjectInfo::new(name.clone(), self.new_kicad_project_location.clone());
        project_info.author = self.new_kicad_project_author.clone();
        project_info.description = description.clone();
        project_info.company = self.new_kicad_project_company.clone();
        project_info.include_kiverse = self.include_kiverse;
        project_info.include_atlantix_resistors = self.include_atlantix_resistors;
        project_info.kiverse_path = Some(self.kiverse_path.clone());

        // Create the KiCad project files
        let _project_dir = create_kicad_project(&project_info)
            .map_err(|e| ProjectDatabaseError::DatabaseWrite(format!("Failed to create KiCad project: {}", e)))?;

        // Now add to our project database
        let pcb_file_path = project_info.pcb_file_path();

        if let Some(ref database) = self.database {
            let project_id = generate_project_id();
            let now = Utc::now();

            let metadata = ProjectMetadata {
                id: project_id.clone(),
                name,
                description,
                pcb_file_path,
                created_at: now,
                last_modified: now,
                version: env!("CARGO_PKG_VERSION").to_string(),
                tags,
            };

            let project_data = ProjectData {
                metadata: metadata.clone(),
                bom_components: Vec::new(),
                notes: String::new(),
            };

            database.save_project(&project_data)?;
            self.project_list = database.list_projects()?;
            self.current_project = Some(project_data);

            Ok(project_id)
        } else {
            Err(ProjectDatabaseError::DatabaseRead("Database not initialized".to_string()))
        }
    }

    /// Reset create dialog
    pub fn reset_create_dialog(&mut self) {
        self.show_create_dialog = false;
        self.new_project_name.clear();
        self.new_project_description.clear();
        self.new_project_tags.clear();
        self.new_project_pcb_path = None;
        self.show_pcb_file_dialog = false;
        self.show_location_dialog = false;
    }
}