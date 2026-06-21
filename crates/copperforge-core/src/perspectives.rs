//! Named dock-layout perspectives — saveable, deletable, with a default that
//! loads on startup.
//!
//! Each perspective is a serialized egui_dock `DockState` (the same JSON the
//! working layout uses), keyed by name, all in one file so a user's set of
//! workspace layouts survives restarts. This is the egui_dock port of the
//! simcore GUI's Qt-ADS perspective system (Save current as…, Open, Delete,
//! Set default / Clear default).

use std::collections::BTreeMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

#[derive(Default, Debug, Clone, Serialize, Deserialize)]
pub struct PerspectiveStore {
    /// name → serialized `DockState` JSON.
    perspectives: BTreeMap<String, String>,
    /// Name of the perspective applied on startup, if any.
    default: Option<String>,
}

impl PerspectiveStore {
    fn path() -> Option<PathBuf> {
        dirs::config_dir().map(|d| d.join("copperforge").join("perspectives.json"))
    }

    /// Load from disk, or an empty store if none / unreadable.
    pub fn load() -> Self {
        Self::path()
            .and_then(|p| std::fs::read_to_string(p).ok())
            .and_then(|j| serde_json::from_str(&j).ok())
            .unwrap_or_default()
    }

    fn persist(&self) {
        if let Some(path) = Self::path() {
            if let Some(dir) = path.parent() {
                let _ = std::fs::create_dir_all(dir);
            }
            if let Ok(j) = serde_json::to_string_pretty(self) {
                let _ = std::fs::write(path, j);
            }
        }
    }

    pub fn names(&self) -> impl Iterator<Item = &String> {
        self.perspectives.keys()
    }

    pub fn is_empty(&self) -> bool {
        self.perspectives.is_empty()
    }

    pub fn get(&self, name: &str) -> Option<&String> {
        self.perspectives.get(name)
    }

    pub fn default_name(&self) -> Option<&str> {
        self.default.as_deref()
    }

    /// The JSON of the startup default, if one is set and still exists.
    pub fn default_json(&self) -> Option<&String> {
        self.default.as_ref().and_then(|n| self.perspectives.get(n))
    }

    /// Save (or overwrite) a named perspective and persist.
    pub fn save(&mut self, name: String, json: String) {
        self.perspectives.insert(name, json);
        self.persist();
    }

    /// Delete a perspective; clears the default if it was the one removed.
    pub fn delete(&mut self, name: &str) {
        self.perspectives.remove(name);
        if self.default.as_deref() == Some(name) {
            self.default = None;
        }
        self.persist();
    }

    /// Set (or clear, with `None`) the startup default.
    pub fn set_default(&mut self, name: Option<String>) {
        self.default = name;
        self.persist();
    }
}
