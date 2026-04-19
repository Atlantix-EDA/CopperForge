pub mod database;
pub mod bom;
pub mod kicad_project;
pub mod kicad_global_libs;
pub mod kicad_metadata;
pub mod kicad_hierarchy;

use database::{ProjectDatabase, ProjectData, ProjectMetadata, generate_project_id, ProjectDatabaseError};
use bom::BomComponent;
use std::path::PathBuf;
use chrono::Utc;
use egui_file_dialog::FileDialog;

/// Project manager state. Create-new-project scaffolding was moved to the
/// Shell panel's `new-project` command; what's left here is strictly the
/// import / list / edit / delete flow for the CopperForge project DB.
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
    pub new_project_parent_id: Option<String>,
    pub last_error: Option<String>,
    /// File dialog for picking a .kicad_pro to import.
    pub pcb_file_dialog: FileDialog,
    /// Recent project names (for quick form re-fill).
    pub recent_project_names: Vec<String>,
    /// Last .kicad_pro path processed — gates the pedigree auto-fill so we
    /// don't re-run it every frame while the dialog reports the same pick.
    /// Cleared on `reset_create_dialog` so re-picking after a successful
    /// import actually re-populates the form.
    pub last_picked_pro_path: Option<PathBuf>,
    /// Project hierarchy tree view
    pub expanded_project_id: Option<String>,
    pub project_hierarchies: std::collections::HashMap<String, kicad_hierarchy::ProjectHierarchy>,
    /// Per-project release cache, keyed by project id. Rebuilt on
    /// `initialize_database()` and bumped by `record_release()` when a
    /// new release is cut via the Release modal. Drives the `outputs/`
    /// subtree in the Projects tab.
    pub project_releases: std::collections::HashMap<String, Vec<crate::release::Release>>,
}

impl Default for ProjectManagerState {
    fn default() -> Self {
        Self::with_config(&crate::project::manager::ProjectConfig::default())
    }
}

impl ProjectManagerState {
    /// Create a new ProjectManagerState. Most fields default to empty /
    /// None and get populated as the user interacts with the import form.
    /// `_config` is kept for signature stability (callers pass it today).
    pub fn with_config(_config: &crate::project::manager::ProjectConfig) -> Self {
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
            new_project_parent_id: None,
            last_error: None,
            pcb_file_dialog: FileDialog::new(),
            recent_project_names: Vec::new(),
            last_picked_pro_path: None,
            expanded_project_id: None,
            project_hierarchies: std::collections::HashMap::new(),
            project_releases: std::collections::HashMap::new(),
        }
    }

    /// Wire up the project database by cloning the shared handle.
    /// `ProjectDatabase` is `Clone` — the sled handle underneath is Arc-backed,
    /// so this does NOT re-acquire the directory lock. The bug was that the
    /// previous implementation called `ProjectDatabase::new(db_path)` here
    /// while `SharedServices` had already opened the same path at startup;
    /// sled's exclusive lock made the second open silently fail, leaving
    /// `self.database = None` and every subsequent DB call erroring with
    /// "Database not initialized."
    pub fn initialize_database(&mut self, db: &ProjectDatabase) -> Result<(), ProjectDatabaseError> {
        self.project_list = db.list_projects()?;
        self.database = Some(db.clone());
        self.update_recent_project_names();
        self.reload_all_releases(db);
        Ok(())
    }

    /// Rebuild the per-project release cache from the DB.
    /// O(N) full-project loads; fine at the scale we expect (dozens of
    /// projects at most). Silently skips any project that fails to load.
    pub fn reload_all_releases(&mut self, db: &ProjectDatabase) {
        self.project_releases.clear();
        for meta in &self.project_list {
            if let Ok(Some(data)) = db.load_project(&meta.id) {
                if !data.releases.is_empty() {
                    self.project_releases.insert(meta.id.clone(), data.releases);
                }
            }
        }
    }

    /// Record a freshly-created release in both the live cache and the DB's
    /// current_project snapshot. Called from the Release modal dispatcher.
    pub fn record_release(&mut self, project_id: &str, release: crate::release::Release) {
        self.project_releases
            .entry(project_id.to_string())
            .or_default()
            .push(release);
    }

    /// Update recent project names list with the 5 most recently modified projects
    fn update_recent_project_names(&mut self) {
        let mut sorted = self.project_list.clone();
        sorted.sort_by(|a, b| b.last_modified.cmp(&a.last_modified));
        self.recent_project_names = sorted.iter()
            .take(5)
            .map(|p| p.name.clone())
            .collect();
    }

    /// Load project metadata into create form fields by project name
    pub fn load_project_metadata_into_form(&mut self, project_name: &str) {
        if let Some(project) = self.project_list.iter().find(|p| p.name == project_name) {
            self.new_project_name = project.name.clone();
            self.new_project_description = project.description.clone();
            self.new_project_tags = project.tags.join(", ");
            // Note: Don't copy PCB path or parent_id - those should be fresh for new project
        }
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
            // Dedup: the same .kicad_pcb can only back one DB record.
            if let Some(existing) = database.find_project_by_pcb_path(&pcb_file_path)? {
                return Err(ProjectDatabaseError::DatabaseWrite(format!(
                    "Project '{}' is already imported (ID: {}). Open it from the Projects tab instead.",
                    existing.metadata.name, existing.metadata.id
                )));
            }

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
                parent_id: self.new_project_parent_id.clone(),
            };
            
            let project_data = ProjectData {
                metadata: metadata.clone(),
                bom_components,
                notes: String::new(),
                releases: Vec::new(),
                hierarchy: None, // Will be loaded on demand
            };
            
            database.save_project(&project_data)?;
            self.project_list = database.list_projects()?;
            self.current_project = Some(project_data);

            // Update recent project names
            self.update_recent_project_names();

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

    /// Reset the import form. Clears transient form state AND
    /// `last_picked_pro_path` — without the latter, re-picking the same
    /// .kicad_pro after a successful import would be skipped by the
    /// "already processed" guard, leaving the form empty and producing
    /// the "Project name cannot be empty" error.
    pub fn reset_create_dialog(&mut self) {
        self.show_create_dialog = false;
        self.new_project_name.clear();
        self.new_project_description.clear();
        self.new_project_tags.clear();
        self.new_project_pcb_path = None;
        self.new_project_parent_id = None;
        self.last_picked_pro_path = None;
    }

    /// Reset all fields (parity wrapper for call sites that distinguished
    /// between "cancel" and "reset"; behaviour is identical now).
    pub fn cancel_create_dialog(&mut self) {
        self.reset_create_dialog();
    }
}