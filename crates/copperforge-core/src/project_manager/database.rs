//! Project database — embedded KV store backing the CopperForge project list.
//!
//! Switched from sled (~40 transitive crates) to redb (~5) for simpler,
//! lighter storage. Behaviour preserved: one table keyed by
//! `project:<id>` for full `ProjectData` records plus a single
//! `index:projects` entry holding the ordered `Vec<String>` of project ids.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use chrono::{DateTime, Utc};
use redb::{Database, TableDefinition};
use serde::{Deserialize, Serialize};

use crate::project_manager::bom::BomComponent;
use crate::project_manager::kicad_hierarchy::ProjectHierarchy;

const PROJECTS_TABLE: TableDefinition<&str, &[u8]> = TableDefinition::new("projects");
const INDEX_KEY: &str = "index:projects";

/// Database manager for project storage.
///
/// `Clone` shares the underlying redb handle via `Arc` — safe because
/// redb serializes writes internally and supports concurrent reads.
/// This is the reason multiple app-level components (SharedServices +
/// ProjectManagerState) can each hold their own handle.
#[derive(Clone)]
pub struct ProjectDatabase {
    db: Arc<Database>,
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
    /// Optional parent project ID for hierarchical organization.
    /// None means this is a root-level project.
    #[serde(default)]
    pub parent_id: Option<String>,
}

/// Complete project data including BOM
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectData {
    pub metadata: ProjectMetadata,
    pub bom_components: Vec<BomComponent>,
    pub notes: String,
    /// Tagged fabrication releases (rev_01, rev_02, ...). Populated by the
    /// Release workflow. `#[serde(default)]` keeps old DB records readable.
    #[serde(default)]
    pub releases: Vec<crate::release::Release>,
    /// Hierarchical structure of schematics and PCB files.
    /// Cached for performance, regenerable from disk.
    #[serde(skip)]
    pub hierarchy: Option<ProjectHierarchy>,
}

impl ProjectDatabase {
    /// Open (or create) the redb database at `db_path`. The parent directory
    /// must exist. Initializes the `projects` table so reads on an empty DB
    /// don't trip on "table not found".
    pub fn new(db_path: &Path) -> Result<Self, ProjectDatabaseError> {
        let db = Database::create(db_path)
            .map_err(|e| ProjectDatabaseError::DatabaseOpen(e.to_string()))?;

        // Touch the table on first open so subsequent reads don't error on
        // a missing table definition.
        let write_txn = db
            .begin_write()
            .map_err(|e| ProjectDatabaseError::DatabaseWrite(e.to_string()))?;
        {
            let _ = write_txn
                .open_table(PROJECTS_TABLE)
                .map_err(|e| ProjectDatabaseError::DatabaseWrite(e.to_string()))?;
        }
        write_txn
            .commit()
            .map_err(|e| ProjectDatabaseError::DatabaseWrite(e.to_string()))?;

        Ok(Self { db: Arc::new(db) })
    }

    /// Save a project (serialized via bincode) + update the ID index.
    pub fn save_project(&self, project: &ProjectData) -> Result<(), ProjectDatabaseError> {
        let key = format!("project:{}", project.metadata.id);
        let value = bincode::serialize(project)
            .map_err(|e| ProjectDatabaseError::Serialization(e.to_string()))?;

        let write_txn = self.db.begin_write()
            .map_err(|e| ProjectDatabaseError::DatabaseWrite(e.to_string()))?;
        {
            let mut table = write_txn.open_table(PROJECTS_TABLE)
                .map_err(|e| ProjectDatabaseError::DatabaseWrite(e.to_string()))?;
            table.insert(key.as_str(), value.as_slice())
                .map_err(|e| ProjectDatabaseError::DatabaseWrite(e.to_string()))?;
        }
        write_txn.commit()
            .map_err(|e| ProjectDatabaseError::DatabaseWrite(e.to_string()))?;

        self.update_project_index(&project.metadata)?;
        Ok(())
    }

    /// Load a project by ID, transparently migrating pre-parent_id records.
    pub fn load_project(&self, project_id: &str) -> Result<Option<ProjectData>, ProjectDatabaseError> {
        let key = format!("project:{}", project_id);

        // Copy the bytes out of the read transaction so we can drop it before
        // save_project() (which starts a write transaction) during migration.
        let bytes: Option<Vec<u8>> = {
            let read_txn = self.db.begin_read()
                .map_err(|e| ProjectDatabaseError::DatabaseRead(e.to_string()))?;
            let table = read_txn.open_table(PROJECTS_TABLE)
                .map_err(|e| ProjectDatabaseError::DatabaseRead(e.to_string()))?;
            match table.get(key.as_str()).map_err(|e| ProjectDatabaseError::DatabaseRead(e.to_string()))? {
                Some(guard) => Some(guard.value().to_vec()),
                None => None,
            }
        };

        let bytes = match bytes {
            Some(b) => b,
            None => return Ok(None),
        };

        match bincode::deserialize::<ProjectData>(&bytes) {
            Ok(project) => Ok(Some(project)),
            Err(_) => {
                // Try the pre-parent_id format and migrate.
                match bincode::deserialize::<OldProjectData>(&bytes) {
                    Ok(old) => {
                        let migrated = ProjectData {
                            metadata: ProjectMetadata {
                                id: old.metadata.id,
                                name: old.metadata.name,
                                description: old.metadata.description,
                                pcb_file_path: old.metadata.pcb_file_path,
                                created_at: old.metadata.created_at,
                                last_modified: old.metadata.last_modified,
                                version: old.metadata.version,
                                tags: old.metadata.tags,
                                parent_id: None,
                            },
                            bom_components: old.bom_components,
                            notes: old.notes,
                            releases: Vec::new(),
                            hierarchy: None,
                        };
                        self.save_project(&migrated)?;
                        Ok(Some(migrated))
                    }
                    Err(e) => Err(ProjectDatabaseError::Deserialization(e.to_string())),
                }
            }
        }
    }

    /// List all project metadata (sorted by last_modified desc).
    /// Reads the index, then loads each project for its metadata.
    /// Skips corrupted / missing entries rather than failing the whole list.
    pub fn list_projects(&self) -> Result<Vec<ProjectMetadata>, ProjectDatabaseError> {
        let ids = self.read_index()?;
        let mut projects = Vec::with_capacity(ids.len());

        for project_id in ids {
            match self.load_project(&project_id) {
                Ok(Some(project)) => projects.push(project.metadata),
                Ok(None) => eprintln!("Warning: Project {} not found in database", project_id),
                Err(e) => eprintln!("Warning: Failed to load project {}: {}. Skipping.", project_id, e),
            }
        }

        projects.sort_by(|a, b| b.last_modified.cmp(&a.last_modified));
        Ok(projects)
    }

    pub fn delete_project(&self, project_id: &str) -> Result<(), ProjectDatabaseError> {
        let key = format!("project:{}", project_id);

        let write_txn = self.db.begin_write()
            .map_err(|e| ProjectDatabaseError::DatabaseWrite(e.to_string()))?;
        {
            let mut table = write_txn.open_table(PROJECTS_TABLE)
                .map_err(|e| ProjectDatabaseError::DatabaseWrite(e.to_string()))?;
            table.remove(key.as_str())
                .map_err(|e| ProjectDatabaseError::DatabaseWrite(e.to_string()))?;
        }
        write_txn.commit()
            .map_err(|e| ProjectDatabaseError::DatabaseWrite(e.to_string()))?;

        self.remove_from_project_index(project_id)?;
        Ok(())
    }

    pub fn search_projects(&self, query: &str) -> Result<Vec<ProjectMetadata>, ProjectDatabaseError> {
        let all_projects = self.list_projects()?;
        let q = query.to_lowercase();
        Ok(all_projects
            .into_iter()
            .filter(|p| {
                p.name.to_lowercase().contains(&q)
                    || p.description.to_lowercase().contains(&q)
                    || p.tags.iter().any(|t| t.to_lowercase().contains(&q))
            })
            .collect())
    }

    pub fn find_project_by_pcb_path(&self, pcb_path: &Path) -> Result<Option<ProjectData>, ProjectDatabaseError> {
        for meta in self.list_projects()? {
            if meta.pcb_file_path == pcb_path {
                if let Some(data) = self.load_project(&meta.id)? {
                    return Ok(Some(data));
                }
            }
        }
        Ok(None)
    }

    pub fn get_stats(&self) -> Result<DatabaseStats, ProjectDatabaseError> {
        let projects = self.list_projects()?;
        Ok(DatabaseStats {
            total_projects: projects.len(),
            // redb doesn't expose an exact on-disk size cheaply; report 0
            // (this field isn't surfaced in the UI today anyway).
            size_on_disk: 0,
            last_accessed: Utc::now(),
        })
    }

    // ── internals ────────────────────────────────────────────────────

    fn read_index(&self) -> Result<Vec<String>, ProjectDatabaseError> {
        let read_txn = self.db.begin_read()
            .map_err(|e| ProjectDatabaseError::DatabaseRead(e.to_string()))?;
        let table = read_txn.open_table(PROJECTS_TABLE)
            .map_err(|e| ProjectDatabaseError::DatabaseRead(e.to_string()))?;
        match table.get(INDEX_KEY).map_err(|e| ProjectDatabaseError::DatabaseRead(e.to_string()))? {
            Some(guard) => bincode::deserialize(guard.value())
                .map_err(|e| ProjectDatabaseError::Deserialization(e.to_string())),
            None => Ok(Vec::new()),
        }
    }

    fn write_index(&self, ids: &[String]) -> Result<(), ProjectDatabaseError> {
        let bytes = bincode::serialize(ids)
            .map_err(|e| ProjectDatabaseError::Serialization(e.to_string()))?;
        let write_txn = self.db.begin_write()
            .map_err(|e| ProjectDatabaseError::DatabaseWrite(e.to_string()))?;
        {
            let mut table = write_txn.open_table(PROJECTS_TABLE)
                .map_err(|e| ProjectDatabaseError::DatabaseWrite(e.to_string()))?;
            table.insert(INDEX_KEY, bytes.as_slice())
                .map_err(|e| ProjectDatabaseError::DatabaseWrite(e.to_string()))?;
        }
        write_txn.commit()
            .map_err(|e| ProjectDatabaseError::DatabaseWrite(e.to_string()))?;
        Ok(())
    }

    fn update_project_index(&self, metadata: &ProjectMetadata) -> Result<(), ProjectDatabaseError> {
        let mut ids = self.read_index()?;
        if !ids.contains(&metadata.id) {
            ids.push(metadata.id.clone());
        }
        self.write_index(&ids)
    }

    fn remove_from_project_index(&self, project_id: &str) -> Result<(), ProjectDatabaseError> {
        let mut ids = self.read_index()?;
        ids.retain(|id| id != project_id);
        self.write_index(&ids)
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
    /// Load the project hierarchy from the KiCad project file.
    /// Parses the .kicad_pro file and associated schematics.
    pub fn load_hierarchy(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        use crate::project_manager::kicad_metadata::get_kicad_pro_path;

        if let Some(kicad_pro_path) = get_kicad_pro_path(&self.metadata.pcb_file_path) {
            if kicad_pro_path.exists() {
                self.hierarchy = Some(ProjectHierarchy::from_kicad_pro(&kicad_pro_path)?);
            }
        }
        Ok(())
    }
}
