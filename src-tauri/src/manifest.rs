//! Plugin catalogue published by JVlatt/tahua-plugins with each release (manifest.json).

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub const MANIFEST_URL: &str =
    "https://github.com/JVlatt/tahua-plugins/releases/latest/download/manifest.json";

/// Manifest schema versions this launcher understands.
const SUPPORTED_SCHEMA: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Manifest {
    pub schema_version: u32,
    pub release: String,
    pub plugins: Vec<Plugin>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Plugin {
    pub id: String,
    pub name: String,
    pub display_name: String,
    #[serde(rename = "type")]
    pub kind: String,
    pub version: String,
    pub description: String,
    pub min_resolume: String,
    pub assets: BTreeMap<String, Asset>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Asset {
    /// Plugin file or bundle folder at the root of the archive, e.g. TahuaTint.dll.
    pub file: String,
    pub archive: String,
    pub url: String,
    pub sha256: String,
    pub size: u64,
}

/// Asset key of the platform this launcher runs on.
pub fn current_platform() -> &'static str {
    if cfg!(target_os = "macos") {
        "macos-universal"
    } else {
        "windows-x64"
    }
}

impl Manifest {
    pub fn parse(json: &str) -> Result<Manifest, String> {
        let manifest: Manifest =
            serde_json::from_str(json).map_err(|e| format!("Manifeste invalide : {e}"))?;
        if manifest.schema_version > SUPPORTED_SCHEMA {
            return Err(format!(
                "Ce manifeste (schéma {}) demande une version plus récente du launcher.",
                manifest.schema_version
            ));
        }
        Ok(manifest)
    }

    pub fn plugin(&self, id: &str) -> Option<&Plugin> {
        self.plugins.iter().find(|p| p.id == id)
    }
}

impl Plugin {
    pub fn asset(&self, platform: &str) -> Option<&Asset> {
        self.assets.get(platform)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"{
      "schemaVersion": 1, "release": "v0.2.0",
      "plugins": [{
        "id": "TE01", "name": "Tint", "displayName": "Tahua Tint", "type": "effect",
        "version": "1.0", "description": "Hue shift", "minResolume": "7.3.1",
        "assets": {
          "windows-x64": { "file": "TahuaTint.dll", "archive": "TahuaTint-1.0-windows-x64.zip",
                           "url": "https://example.com/w.zip", "sha256": "ab", "size": 10 },
          "macos-universal": { "file": "TahuaTint.bundle", "archive": "TahuaTint-1.0-macos-universal.zip",
                               "url": "https://example.com/m.zip", "sha256": "cd", "size": 20 }
        }
      }]
    }"#;

    #[test]
    fn parses_release_manifest() {
        let m = Manifest::parse(SAMPLE).unwrap();
        assert_eq!(m.release, "v0.2.0");
        let tint = m.plugin("TE01").unwrap();
        assert_eq!(tint.kind, "effect");
        assert_eq!(tint.asset("windows-x64").unwrap().file, "TahuaTint.dll");
        assert_eq!(tint.asset("macos-universal").unwrap().size, 20);
        assert!(m.plugin("TE99").is_none());
    }

    #[test]
    fn rejects_newer_schema() {
        let newer = SAMPLE.replace("\"schemaVersion\": 1", "\"schemaVersion\": 2");
        assert!(Manifest::parse(&newer).is_err());
    }

    #[test]
    fn platform_has_an_asset_key() {
        let m = Manifest::parse(SAMPLE).unwrap();
        assert!(m.plugins[0].asset(current_platform()).is_some());
    }
}
