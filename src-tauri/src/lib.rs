mod installer;
mod manifest;
mod state;
mod targets;

use manifest::{current_platform, Asset, Manifest, Plugin, MANIFEST_URL};
use serde::Serialize;
use state::InstallState;
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Mutex;
use targets::Target;
use tauri::{AppHandle, Emitter, Manager, State};

/// Manifest currently shown in the UI; install/uninstall act on it.
#[derive(Default)]
struct AppState {
    manifest: Mutex<Option<Manifest>>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Catalog {
    manifest: Manifest,
    /// The online manifest could not be fetched: this is the last cached copy.
    offline: bool,
    platform: &'static str,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct TargetStatus {
    #[serde(flatten)]
    target: Target,
    /// plugin id -> installed version ("?" when installed outside the launcher)
    installed: BTreeMap<String, String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Overview {
    targets: Vec<TargetStatus>,
    /// Running Resolume applications: installing is blocked while they run.
    running: Vec<String>,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct Progress {
    id: String,
    downloaded: u64,
    total: u64,
}

fn data_file(app: &AppHandle, name: &str) -> Result<PathBuf, String> {
    let dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
    Ok(dir.join(name))
}

fn manifest_url() -> String {
    // Override for testing against another manifest.
    std::env::var("TAHUA_MANIFEST_URL").unwrap_or_else(|_| MANIFEST_URL.to_string())
}

/// The plugin and its asset for this platform, from the manifest shown in the UI.
fn plugin_asset(state: &State<'_, AppState>, id: &str) -> Result<(Plugin, Asset), String> {
    let guard = state.manifest.lock().unwrap();
    let manifest = guard.as_ref().ok_or("Catalogue non chargé.")?;
    let plugin = manifest.plugin(id).ok_or_else(|| format!("Plugin {id} inconnu."))?;
    let asset = plugin
        .asset(current_platform())
        .ok_or_else(|| format!("{} n'est pas disponible pour cette plateforme.", plugin.display_name))?;
    Ok((plugin.clone(), asset.clone()))
}

fn selected_targets(keys: &[String]) -> Result<Vec<Target>, String> {
    if keys.is_empty() {
        return Err("Choisissez au moins une destination (Arena ou Avenue).".into());
    }
    let all = targets::find_targets(&targets::documents_dir()?);
    keys.iter()
        .map(|key| all.iter().find(|t| &t.key == key).cloned().ok_or_else(|| format!("Destination inconnue : {key}")))
        .collect()
}

async fn download(app: &AppHandle, id: &str, url: &str, expected_size: u64) -> Result<Vec<u8>, String> {
    let fail = |e: reqwest::Error| format!("Téléchargement impossible : {e}");
    let mut response = reqwest::get(url).await.map_err(fail)?.error_for_status().map_err(fail)?;
    let total = response.content_length().unwrap_or(expected_size);
    let mut data = Vec::with_capacity(total as usize);
    let mut last_percent = u64::MAX;
    while let Some(chunk) = response.chunk().await.map_err(fail)? {
        data.extend_from_slice(&chunk);
        let percent = if total > 0 { data.len() as u64 * 100 / total } else { 0 };
        if percent != last_percent {
            last_percent = percent;
            let _ = app.emit("download-progress", Progress { id: id.into(), downloaded: data.len() as u64, total });
        }
    }
    Ok(data)
}

#[tauri::command]
async fn fetch_catalog(app: AppHandle, state: State<'_, AppState>) -> Result<Catalog, String> {
    let cache = data_file(&app, "manifest-cache.json")?;
    let online = async {
        let fail = |e: reqwest::Error| e.to_string();
        reqwest::get(manifest_url()).await.map_err(fail)?.error_for_status().map_err(fail)?.text().await.map_err(fail)
    }
    .await;

    let (manifest, offline) = match online {
        Ok(text) => {
            let manifest = Manifest::parse(&text)?;
            if let Some(dir) = cache.parent() {
                let _ = std::fs::create_dir_all(dir);
            }
            let _ = std::fs::write(&cache, &text);
            (manifest, false)
        }
        Err(error) => match std::fs::read_to_string(&cache) {
            Ok(text) => (Manifest::parse(&text)?, true),
            Err(_) => return Err(format!("Catalogue des plugins indisponible : {error}")),
        },
    };
    *state.manifest.lock().unwrap() = Some(manifest.clone());
    Ok(Catalog { manifest, offline, platform: current_platform() })
}

#[tauri::command]
fn overview(app: AppHandle, state: State<'_, AppState>) -> Result<Overview, String> {
    let guard = state.manifest.lock().unwrap();
    let manifest = guard.as_ref().ok_or("Catalogue non chargé.")?;
    let install_state = InstallState::load(&data_file(&app, "installed.json")?);
    let targets = targets::find_targets(&targets::documents_dir()?)
        .into_iter()
        .map(|target| TargetStatus {
            installed: install_state.installed(&target.key, &target.extra_effects, manifest, current_platform()),
            target,
        })
        .collect();
    Ok(Overview { targets, running: installer::running_resolume() })
}

/// Installs or updates a plugin in the given targets ("arena", "avenue").
#[tauri::command]
async fn install_plugin(
    app: AppHandle,
    state: State<'_, AppState>,
    id: String,
    targets: Vec<String>,
) -> Result<(), String> {
    installer::ensure_resolume_closed()?;
    let (plugin, asset) = plugin_asset(&state, &id)?;
    let destinations = selected_targets(&targets)?;

    let data = download(&app, &id, &asset.url, asset.size).await?;
    installer::verify_sha256(&data, &asset.sha256)?;
    let tmp = tempfile::tempdir().map_err(|e| e.to_string())?;
    let extracted = installer::extract(&data, &asset.file, tmp.path())?;

    let state_file = data_file(&app, "installed.json")?;
    let mut install_state = InstallState::load(&state_file);
    for target in &destinations {
        installer::install_into(&extracted, &target.extra_effects)?;
        install_state.set(&target.key, &plugin.id, &plugin.version);
    }
    install_state.save(&state_file)
}

#[tauri::command]
fn uninstall_plugin(
    app: AppHandle,
    state: State<'_, AppState>,
    id: String,
    targets: Vec<String>,
) -> Result<(), String> {
    installer::ensure_resolume_closed()?;
    let (plugin, asset) = plugin_asset(&state, &id)?;
    let state_file = data_file(&app, "installed.json")?;
    let mut install_state = InstallState::load(&state_file);
    for target in selected_targets(&targets)? {
        installer::remove_plugin(&target.extra_effects.join(&asset.file))?;
        install_state.remove(&target.key, &plugin.id);
    }
    install_state.save(&state_file)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init())
        .manage(AppState::default())
        .invoke_handler(tauri::generate_handler![fetch_catalog, overview, install_plugin, uninstall_plugin])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
