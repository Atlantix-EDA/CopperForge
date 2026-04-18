use serde::{Serialize, Deserialize};
use std::path::{Path, PathBuf};
use chrono::{DateTime, Utc};
use crate::project_manager::bom::BomComponent;
use crate::project_manager::kicad_hierarchy::ProjectHierarchy;

/// Database manager for project storage
pub struct ProjectDatabase {
    db: sled::Db,
}

/// Old project metadata format (before parent_id was added)
#[derive(Debug, Clone, Serialize, Deserialize)]
struct OldProjectMetadata {
    pub id: String,
    pub name: String,
    pub description: String,
    pub pcb_file_path: PathBuf,
    pub created_at: DateTime<Utc>,
    pub last_modified: DateTime<Utc>,
    pub version: String,
    pub tags: Vec<String>,
}

/// Old project data format (before parent_id was added)
#[derive(Debug, Clone, Serialize, Deserialize)]
struct OldProjectData {
    pub metadata: OldProjectMetadata,
    pub bom_components: Vec<BomComponent>,
    pub notes: String,
}

/// Project metadata stored in the database
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectMetadata {
    pub id: String,
    pub name: String,
    pub description: String,
    pub pcb_file_path: PathBuf,
    pub created_at: DateTime<Utc>,
    pub last_modified: DateTime<Utc>,
    pub version: String,
    pub tags: Vec<String>,
    /// Optional parent project ID for hierarchical organization
    /// None means this is a root-level project
    /// Default to None for backward compatibility with old database entries
    #[serde(default)]
    pub parent_id: Option<String>,
}

/// Complete project data including BOM
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectData {
    pub metadata: ProjectMetadata,
    pub bom_components: Vec<BomComponent>,
    pub notes: String,
    /// Hierarchical structure of schematics and PCB files
    /// Cached for performance, can be regenerated from disk
    #[serde(skip)]
    pub hierarchy: Option<ProjectHierarchy>,
}

impl ProjectDatabase {
    /// Create a new project database
    pub fn new(db_path: &Path) -> Result<Self, ProjectDatabaseError> {
        let db = sled::open(db_path)
            .map_err(|e| ProjectDatabaseError::DatabaseOpen(e.to_string()))?;
        
        Ok(Self { db })
    }

    /// Save a project to the database
    pub fn save_project(&self, project: &ProjectData) -> Result<(), ProjectDatabaseError> {
        let key = format!("project:{}", project.metadata.id);
        let value = bincode::serialize(project)
            .map_err(|e| ProjectDatabaseError::Serialization(e.to_string()))?;
        
        self.db.insert(key.as_bytes(), value)
            .map_err(|e| ProjectDatabaseError::DatabaseWrite(e.to_string()))?;
        
        // Update index for quick lookups
        self.update_project_index(&project.metadata)?;
        
        Ok(())
    }

    /// Load a project from the database
    pub fn load_project(&self, project_id: &str) -> Result<Option<ProjectData>, ProjectDatabaseError> {
        let key = format!("project:{}", project_id);

        if let Some(value) = self.db.get(key.as_bytes())
            .map_err(|e| ProjectDatabaseError::DatabaseRead(e.to_string()))? {

            // Try to deserialize with current format
            match bincode::deserialize::<ProjectData>(&value) {
                Ok(project) => Ok(Some(project)),
                Err(_) => {
                    // Try to deserialize with old format (without parent_id)
                    match bincode::deserialize::<OldProjectData>(&value) {
                        Ok(old_project) => {
                            // Migrate to new format
                            let new_project = ProjectData {
                                metadata: ProjectMetadata {
                                    id: old_project.metadata.id,
                                    name: old_project.metadata.name,
                                    description: old_project.metadata.description,
                                    pcb_file_path: old_project.metadata.pcb_file_path,
                                    created_at: old_project.metadata.created_at,
                                    last_modified: old_project.metadata.last_modified,
                                    version: old_project.metadata.version,
                                    tags: old_project.metadata.tags,
                                    parent_id: None, // Default to root level
                                },
                                bom_components: old_project.bom_components,
                                notes: old_project.notes,
                                hierarchy: None, // Will be loaded on demand
                            };

                            // Save migrated project back to database
                            self.save_project(&new_project)?;

                            Ok(Some(new_project))
                        }
                        Err(e) => Err(ProjectDatabaseError::Deserialization(e.to_string()))
                    }
                }
            }
        } else {
            Ok(None)
        }
    }

    /// List all projects (metadata only for performance)
    pub fn list_projects(&self) -> Result<Vec<ProjectMetadata>, ProjectDatabaseError> {
        let mut projects = Vec::new();

        // Use index for efficient listing
        if let Some(index_data) = self.db.get(b"index:projects")
            .map_err(|e| ProjectDatabaseError::DatabaseRead(e.to_string()))? {

            let project_ids: Vec<String> = bincode::deserialize(&index_data)
                .map_err(|e| ProjectDatabaseError::Deserialization(e.to_string()))?;

            for project_id in project_ids {
                // Skip corrupted projects instead of failing completely
                match self.load_project(&project_id) {
                    Ok(Some(project)) => {
                        // last_modified is the DB record's own timestamp, bumped by
                        // update_project / update_project_bom. Do NOT overwrite it
                        // with the PCB file's filesystem mtime — that conflated two
                        // different things and made created_at appear later than
                        // last_modified when the PCB file predated the DB record.
                        projects.push(project.metadata);
                    }
                    Ok(None) => {
                        eprintln!("Warning: Project {} not found in database", project_id);
                    }
                    Err(e) => {
                        eprintln!("Warning: Failed to load project {}: {}. Skipping corrupted entry.", project_id, e);
                    }
                }
            }
        }

        // Sort by last modified (newest first)
        projects.sort_by(|a, b| b.last_modified.cmp(&a.last_modified));

        Ok(projects)
    }

    /// Delete a project
    pub fn delete_project(&self, project_id: &str) -> Result<(), ProjectDatabaseError> {
        let key = format!("project:{}", project_id);
        
        self.db.remove(key.as_bytes())
            .map_err(|e| ProjectDatabaseError::DatabaseWrite(e.to_string()))?;
        
        // Remove from index
        self.remove_from_project_index(project_id)?;
        
        Ok(())
    }

    /// Search projects by name or description
    pub fn search_projects(&self, query: &str) -> Result<Vec<ProjectMetadata>, ProjectDatabaseError> {
        let all_projects = self.list_projects()?;
        let query_lower = query.to_lowercase();
        
        let filtered: Vec<ProjectMetadata> = all_projects
            .into_iter()
            .filter(|project| {
                project.name.to_lowercase().contains(&query_lower) ||
                project.description.to_lowercase().contains(&query_lower) ||
                project.tags.iter().any(|tag| tag.to_lowercase().contains(&query_lower))
            })
            .collect();
        
        Ok(filtered)
    }

    /// Find project by PCB file path
    pub fn find_project_by_pcb_path(&self, pcb_path: &std::path::Path) -> Result<Option<ProjectData>, ProjectDatabaseError> {
        let all_projects = self.list_projects()?;
        
        for project_metadata in all_projects {
            if project_metadata.pcb_file_path == pcb_path {
                if let Some(project_data) = self.load_project(&project_metadata.id)? {
                    return Ok(Some(project_data));
                }
            }
        }
        
        Ok(None)
    }

    /// Update project index for quick listings
    fn update_project_index(&self, metadata: &ProjectMetadata) -> Result<(), ProjectDatabaseError> {
        let mut project_ids: Vec<String> = if let Some(index_data) = self.db.get(b"index:projects")
            .map_err(|e| ProjectDatabaseError::DatabaseRead(e.to_string()))? {
            
            bincode::deserialize(&index_data)
                .map_err(|e| ProjectDatabaseError::Deserialization(e.to_string()))?
        } else {
            Vec::new()
        };
        
        // Add project ID if not already present
        if !project_ids.contains(&metadata.id) {
            project_ids.push(metadata.id.clone());
        }
        
        let index_data = bincode::serialize(&project_ids)
            .map_err(|e| ProjectDatabaseError::Serialization(e.to_string()))?;
        
        self.db.insert(b"index:projects", index_data)
            .map_err(|e| ProjectDatabaseError::DatabaseWrite(e.to_string()))?;
        
        Ok(())
    }

    /// Remove project from index
    fn remove_from_project_index(&self, project_id: &str) -> Result<(), ProjectDatabaseError> {
        if let Some(index_data) = self.db.get(b"index:projects")
            .map_err(|e| ProjectDatabaseError::DatabaseRead(e.to_string()))? {
            
            let mut project_ids: Vec<String> = bincode::deserialize(&index_data)
                .map_err(|e| ProjectDatabaseError::Deserialization(e.to_string()))?;
            
            project_ids.retain(|id| id != project_id);
            
            let index_data = bincode::serialize(&project_ids)
                .map_err(|e| ProjectDatabaseError::Serialization(e.to_string()))?;
            
            self.db.insert(b"index:projects", index_data)
                .map_err(|e| ProjectDatabaseError::DatabaseWrite(e.to_string()))?;
        }
        
        Ok(())
    }

    /// Get database statistics
    pub fn get_stats(&self) -> Result<DatabaseStats, ProjectDatabaseError> {
        let projects = self.list_projects()?;
        let total_projects = projects.len();
        
        let size_on_disk = self.db.size_on_disk()
            .map_err(|e| ProjectDatabaseError::DatabaseRead(e.to_string()))?;
        
        Ok(DatabaseStats {
            total_projects,
            size_on_disk,
            last_accessed: Utc::now(),
        })
    }
}

/// Database statistics
#[derive(Debug, Clone)]
pub struct DatabaseStats {
    pub total_projects: usize,
    pub size_on_disk: u64,
    pub last_accessed: DateTime<Utc>,
}

/// Project database errors
#[derive(Debug, thiserror::Error)]
pub enum ProjectDatabaseError {
    #[error("Failed to open database: {0}")]
    DatabaseOpen(String),
    
    #[error("Failed to read from database: {0}")]
    DatabaseRead(String),
    
    #[error("Failed to write to database: {0}")]
    DatabaseWrite(String),
    
    #[error("Failed to serialize data: {0}")]
    Serialization(String),
    
    #[error("Failed to deserialize data: {0}")]
    Deserialization(String),
}

/// Helper function to generate unique project ID
pub fn generate_project_id() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_millis();

    format!("proj_{}", timestamp)
}

impl ProjectData {
    /// Load the project hierarchy from the KiCad project file
    /// This parses the .kicad_pro file and associated schematics
    pub fn load_hierarchy(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        use crate::project_manager::kicad_metadata::get_kicad_pro_path;

        // Get the .kicad_pro path from the PCB path
        if let Some(kicad_pro_path) = get_kicad_pro_path(&self.metadata.pcb_file_path) {
            if kicad_pro_path.exists() {
                self.hierarchy = Some(ProjectHierarchy::from_kicad_pro(&kicad_pro_path)?);
            }
        }

        Ok(())
    }
}