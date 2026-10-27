//! Downloads, verifies and installs plugin archives into Extra Effects folders.

use sha2::{Digest, Sha256};
use std::io::{Cursor, Read};
use std::path::{Path, PathBuf};
use sysinfo::{ProcessesToUpdate, System};

/// Resolume processes that lock loaded plugins.
const RESOLUME_PROCESSES: [&str; 4] = ["arena.exe", "avenue.exe", "arena", "avenue"];

/// Names of the running Resolume applications (empty when none runs).
pub fn running_resolume() -> Vec<String> {
    let mut system = System::new();
    system.refresh_processes(ProcessesToUpdate::All, true);
    let mut names: Vec<String> = system
        .processes()
        .values()
        .map(|p| p.name().to_string_lossy().to_string())
        .filter(|name| RESOLUME_PROCESSES.contains(&name.to_lowercase().as_str()))
        .map(|name| name.trim_end_matches(".exe").to_string())
        .collect();
    names.sort();
    names.dedup();
    names
}

pub fn ensure_resolume_closed() -> Result<(), String> {
    let running = running_resolume();
    if running.is_empty() {
        Ok(())
    } else {
        Err(format!("Fermez Resolume {} avant d'installer ou de retirer un plugin.", running.join(", ")))
    }
}

pub fn verify_sha256(data: &[u8], expected: &str) -> Result<(), String> {
    let actual = hex::encode(Sha256::digest(data));
    if actual.eq_ignore_ascii_case(expected) {
        Ok(())
    } else {
        Err(format!("Fichier corrompu (sha256 {actual}, attendu {expected})."))
    }
}

/// Extracts `item` (a file, or a folder for macOS bundles) from the archive into `dir`.
/// Other entries and macOS metadata (__MACOSX, ._*) are skipped.
pub fn extract(archive: &[u8], item: &str, dir: &Path) -> Result<PathBuf, String> {
    let mut zip = zip::ZipArchive::new(Cursor::new(archive)).map_err(|e| format!("Archive invalide : {e}"))?;
    for i in 0..zip.len() {
        let mut entry = zip.by_index(i).map_err(|e| e.to_string())?;
        let Some(relative) = entry.enclosed_name() else { continue };
        let first = relative.components().next().map(|c| c.as_os_str().to_string_lossy().to_string());
        let is_metadata = relative
            .file_name()
            .is_some_and(|n| n.to_string_lossy().starts_with("._"));
        if first.as_deref() != Some(item) || is_metadata {
            continue;
        }
        let out = dir.join(&relative);
        if entry.is_dir() {
            std::fs::create_dir_all(&out).map_err(|e| e.to_string())?;
            continue;
        }
        if let Some(parent) = out.parent() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        let mut bytes = Vec::with_capacity(entry.size() as usize);
        entry.read_to_end(&mut bytes).map_err(|e| e.to_string())?;
        std::fs::write(&out, bytes).map_err(|e| e.to_string())?;
        #[cfg(unix)]
        if let Some(mode) = entry.unix_mode() {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&out, std::fs::Permissions::from_mode(mode)).map_err(|e| e.to_string())?;
        }
    }
    let extracted = dir.join(item);
    if extracted.exists() {
        Ok(extracted)
    } else {
        Err(format!("{item} absent de l'archive."))
    }
}

/// Removes a plugin file or bundle folder; missing is not an error.
pub fn remove_plugin(path: &Path) -> Result<(), String> {
    let result = if path.is_dir() {
        std::fs::remove_dir_all(path)
    } else if path.exists() {
        std::fs::remove_file(path)
    } else {
        Ok(())
    };
    result.map_err(|e| format!("Impossible de supprimer {} : {e}", path.display()))
}

/// Copies an extracted plugin (file or bundle folder) into `extra_effects`, replacing any previous one.
pub fn install_into(extracted: &Path, extra_effects: &Path) -> Result<(), String> {
    std::fs::create_dir_all(extra_effects)
        .map_err(|e| format!("Impossible de créer {} : {e}", extra_effects.display()))?;
    let name = extracted.file_name().ok_or("Nom de plugin invalide.")?;
    let dest = extra_effects.join(name);
    remove_plugin(&dest)?;
    copy_recursive(extracted, &dest).map_err(|e| format!("Impossible de copier vers {} : {e}", dest.display()))
}

fn copy_recursive(src: &Path, dest: &Path) -> std::io::Result<()> {
    if src.is_dir() {
        std::fs::create_dir_all(dest)?;
        for entry in std::fs::read_dir(src)? {
            let entry = entry?;
            copy_recursive(&entry.path(), &dest.join(entry.file_name()))?;
        }
        Ok(())
    } else {
        std::fs::copy(src, dest).map(|_| ())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn make_zip(entries: &[(&str, &[u8])]) -> Vec<u8> {
        let mut buffer = Cursor::new(Vec::new());
        {
            let mut zip = zip::ZipWriter::new(&mut buffer);
            let options = zip::write::SimpleFileOptions::default();
            for (name, data) in entries {
                zip.start_file(*name, options).unwrap();
                zip.write_all(data).unwrap();
            }
            zip.finish().unwrap();
        }
        buffer.into_inner()
    }

    #[test]
    fn verifies_checksum() {
        let sha = "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824"; // "hello"
        assert!(verify_sha256(b"hello", sha).is_ok());
        assert!(verify_sha256(b"hello", &sha.to_uppercase()).is_ok());
        assert!(verify_sha256(b"hellO", sha).is_err());
    }

    #[test]
    fn extracts_and_installs_a_single_file_plugin() {
        let zip = make_zip(&[("TahuaTint.dll", b"dll"), ("README.txt", b"ignored")]);
        let tmp = tempfile::tempdir().unwrap();
        let extracted = extract(&zip, "TahuaTint.dll", tmp.path()).unwrap();
        assert!(!tmp.path().join("README.txt").exists());

        let extra = tmp.path().join("Extra Effects");
        install_into(&extracted, &extra).unwrap();
        install_into(&extracted, &extra).unwrap(); // replacing works
        assert_eq!(std::fs::read(extra.join("TahuaTint.dll")).unwrap(), b"dll");
    }

    #[test]
    fn extracts_bundle_folder_and_skips_metadata() {
        let zip = make_zip(&[
            ("TahuaTint.bundle/Contents/Info.plist", b"plist"),
            ("TahuaTint.bundle/Contents/MacOS/TahuaTint", b"bin"),
            ("TahuaTint.bundle/Contents/._Info.plist", b"meta"),
            ("__MACOSX/TahuaTint.bundle/._Contents", b"meta"),
        ]);
        let tmp = tempfile::tempdir().unwrap();
        let extracted = extract(&zip, "TahuaTint.bundle", tmp.path()).unwrap();
        assert!(extracted.join("Contents/MacOS/TahuaTint").is_file());
        assert!(!extracted.join("Contents/._Info.plist").exists());
        assert!(!tmp.path().join("__MACOSX").exists());

        let extra = tmp.path().join("Extra Effects");
        std::fs::create_dir_all(extra.join("TahuaTint.bundle/Contents/Old")).unwrap();
        install_into(&extracted, &extra).unwrap();
        assert!(!extra.join("TahuaTint.bundle/Contents/Old").exists()); // old bundle replaced
        assert!(extra.join("TahuaTint.bundle/Contents/Info.plist").is_file());

        remove_plugin(&extra.join("TahuaTint.bundle")).unwrap();
        assert!(!extra.join("TahuaTint.bundle").exists());
        remove_plugin(&extra.join("TahuaTint.bundle")).unwrap(); // already gone
    }

    #[test]
    fn missing_item_is_an_error() {
        let zip = make_zip(&[("Other.dll", b"x")]);
        let tmp = tempfile::tempdir().unwrap();
        assert!(extract(&zip, "TahuaTint.dll", tmp.path()).is_err());
    }
}
