//! Which plugin versions the launcher installed, per target (installed.json in the app data folder).
//!
//! The file system stays the source of truth: an entry whose plugin file was deleted by
//! hand is ignored, and a Tahua plugin present without an entry (installed manually from
//! a zip) is reported with an unknown version.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::Path;

use crate::manifest::Manifest;

/// target key -> plugin id -> installed version
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct InstallState {
    pub targets: BTreeMap<String, BTreeMap<String, String>>,
}

pub const UNKNOWN_VERSION: &str = "?";

impl InstallState {
    pub fn load(path: &Path) -> InstallState {
        std::fs::read_to_string(path)
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default()
    }

    pub fn save(&self, path: &Path) -> Result<(), String> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        }
        let json = serde_json::to_string_pretty(self).map_err(|e| e.to_string())?;
        std::fs::write(path, json).map_err(|e| format!("Impossible d'enregistrer l'état : {e}"))
    }

    pub fn set(&mut self, target: &str, id: &str, version: &str) {
        self.targets.entry(target.to_string()).or_default().insert(id.to_string(), version.to_string());
    }

    pub fn remove(&mut self, target: &str, id: &str) {
        if let Some(plugins) = self.targets.get_mut(target) {
            plugins.remove(id);
        }
    }

    /// Installed plugins of one target, checked against the files in `extra_effects`.
    pub fn installed(
        &self,
        target: &str,
        extra_effects: &Path,
        manifest: &Manifest,
        platform: &str,
    ) -> BTreeMap<String, String> {
        let recorded = self.targets.get(target);
        manifest
            .plugins
            .iter()
            .filter_map(|plugin| {
                let asset = plugin.asset(platform)?;
                if !extra_effects.join(&asset.file).exists() {
                    return None;
                }
                let version = recorded
                    .and_then(|r| r.get(&plugin.id))
                    .cloned()
                    .unwrap_or_else(|| UNKNOWN_VERSION.to_string());
                Some((plugin.id.clone(), version))
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn manifest() -> Manifest {
        Manifest::parse(
            r#"{ "schemaVersion": 1, "release": "v1", "plugins": [
              { "id": "TE01", "name": "Tint", "displayName": "Tahua Tint", "type": "effect",
                "version": "1.1", "description": "", "minResolume": "7.3.1",
                "assets": { "windows-x64": { "file": "TahuaTint.dll", "archive": "a.zip",
                            "url": "u", "sha256": "s", "size": 1 } } },
              { "id": "TE02", "name": "Blur", "displayName": "Tahua Blur", "type": "effect",
                "version": "1.0", "description": "", "minResolume": "7.3.1",
                "assets": { "windows-x64": { "file": "TahuaBlur.dll", "archive": "b.zip",
                            "url": "u", "sha256": "s", "size": 1 } } } ] }"#,
        )
        .unwrap()
    }

    #[test]
    fn reconciles_state_with_files() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("TahuaTint.dll"), b"x").unwrap();

        let mut state = InstallState::default();
        state.set("arena", "TE01", "1.0");
        state.set("arena", "TE02", "1.0"); // file deleted by hand

        let installed = state.installed("arena", dir.path(), &manifest(), "windows-x64");
        assert_eq!(installed.get("TE01").map(String::as_str), Some("1.0"));
        assert!(!installed.contains_key("TE02"));
    }

    #[test]
    fn manual_install_has_unknown_version() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("TahuaBlur.dll"), b"x").unwrap();

        let installed = InstallState::default().installed("arena", dir.path(), &manifest(), "windows-x64");
        assert_eq!(installed.get("TE02").map(String::as_str), Some(UNKNOWN_VERSION));
    }

    #[test]
    fn round_trips_through_disk() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("sub").join("installed.json");
        let mut state = InstallState::default();
        state.set("avenue", "TE01", "1.1");
        state.save(&path).unwrap();
        assert_eq!(InstallState::load(&path).targets["avenue"]["TE01"], "1.1");
        state.remove("avenue", "TE01");
        assert!(state.targets["avenue"].is_empty());
    }
}
