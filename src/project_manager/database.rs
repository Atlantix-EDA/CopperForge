use serde::{Serialize, Deserialize};
use std::path::{Path, PathBuf};
use chrono::{DateTime, Utc};
use crate::ui::bom_panel_v2::BomComponent;

/// Database manager for project storage
pub struct ProjectDatabase {
    db: sled::Db,
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
}

/// Complete project data including BOM
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectData {
    pub metadata: ProjectMetadata,
    pub bom_components: Vec<BomComponent>,
    pub notes: String,
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
            
            let project: ProjectData = bincode::deserialize(&value)
                .map_err(|e| ProjectDatabaseError::Deserialization(e.to_string()))?;
            
            Ok(Some(project))
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
                if let Some(project) = self.load_project(&project_id)? {
                    projects.push(project.metadata);
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