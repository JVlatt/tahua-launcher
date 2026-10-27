//! Where Resolume loads third-party plugins: <Documents>/Resolume <Product>/Extra Effects.

use serde::Serialize;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Target {
    /// "arena" or "avenue".
    pub key: String,
    pub label: String,
    pub extra_effects: PathBuf,
    /// The Resolume user folder exists, i.e. this product has been run on this machine.
    pub detected: bool,
}

const PRODUCTS: [(&str, &str); 2] = [("arena", "Arena"), ("avenue", "Avenue")];

/// Real Documents folder (follows redirection, e.g. to another drive on Windows).
pub fn documents_dir() -> Result<PathBuf, String> {
    dirs::document_dir().ok_or_else(|| "Dossier Documents introuvable.".to_string())
}

/// Every product is listed; `detected` tells which ones exist. When none is detected,
/// installing into Arena's folder still works: it is created on first install.
pub fn find_targets(documents: &Path) -> Vec<Target> {
    PRODUCTS
        .iter()
        .map(|(key, product)| {
            let user_dir = documents.join(format!("Resolume {product}"));
            Target {
                key: key.to_string(),
                label: format!("Resolume {product}"),
                extra_effects: user_dir.join("Extra Effects"),
                detected: user_dir.is_dir(),
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_installed_products() {
        let docs = tempfile::tempdir().unwrap();
        std::fs::create_dir(docs.path().join("Resolume Arena")).unwrap();

        let targets = find_targets(docs.path());
        assert_eq!(targets.len(), 2);
        let arena = &targets[0];
        assert_eq!(arena.key, "arena");
        assert!(arena.detected);
        assert_eq!(arena.extra_effects, docs.path().join("Resolume Arena").join("Extra Effects"));
        assert!(!targets[1].detected);
    }
}
