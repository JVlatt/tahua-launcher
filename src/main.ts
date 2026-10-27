import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { getVersion } from "@tauri-apps/api/app";
import { check, type Update } from "@tauri-apps/plugin-updater";
import { relaunch } from "@tauri-apps/plugin-process";

// Mirrors the Rust types serialized by src-tauri/src/lib.rs.
interface Plugin {
  id: string;
  name: string;
  displayName: string;
  type: "effect" | "source" | "mixer";
  version: string;
  description: string;
  minResolume: string;
  assets: Record<string, unknown>;
}
interface Catalog {
  manifest: { release: string; plugins: Plugin[] };
  offline: boolean;
  platform: string;
}
interface TargetStatus {
  key: string;
  label: string;
  extraEffects: string;
  detected: boolean;
  installed: Record<string, string>;
}
interface Overview {
  targets: TargetStatus[];
  running: string[];
}
interface Progress {
  id: string;
  downloaded: number;
  total: number;
}

const TYPE_LABELS: Record<Plugin["type"], string> = { effect: "Effet", source: "Source", mixer: "Mixer" };
const TARGETS_KEY = "enabledTargets";

let catalog: Catalog | null = null;
let overview: Overview | null = null;
const busy = new Set<string>();
const progress = new Map<string, number>();

const $ = <T extends HTMLElement>(id: string) => document.getElementById(id) as T;

function escape(text: string): string {
  const div = document.createElement("div");
  div.textContent = text;
  return div.innerHTML;
}

function showBanner(id: string, html: string | null) {
  const el = $(id);
  el.hidden = !html;
  el.innerHTML = html ?? "";
}

function showError(error: unknown) {
  showBanner("error", error ? escape(String(error)) : null);
}

// --- targets -------------------------------------------------------------------------

/** Targets offered in the UI: detected Resolume products, or Arena when none is detected. */
function availableTargets(): TargetStatus[] {
  const all = overview?.targets ?? [];
  const detected = all.filter((t) => t.detected);
  return detected.length ? detected : all.filter((t) => t.key === "arena");
}

function enabledTargets(): TargetStatus[] {
  const available = availableTargets();
  let saved: string[] | null = null;
  try {
    saved = JSON.parse(localStorage.getItem(TARGETS_KEY) ?? "null");
  } catch {
    saved = null;
  }
  const enabled = saved ? available.filter((t) => saved!.includes(t.key)) : available;
  return enabled.length ? enabled : available.slice(0, 1);
}

function saveEnabledTargets(keys: string[]) {
  try {
    localStorage.setItem(TARGETS_KEY, JSON.stringify(keys));
  } catch {
    // Not persisted: the selection still applies for this session.
  }
}

// --- plugin status -------------------------------------------------------------------

type Status = "install" | "update" | "uptodate";

/** Up to date when every selected target has the catalogue's version. */
function statusOf(plugin: Plugin, targets: TargetStatus[]): Status {
  const versions = targets.map((t) => t.installed[plugin.id]);
  if (versions.every((v) => v === plugin.version)) return "uptodate";
  return versions.some((v) => v !== undefined) ? "update" : "install";
}

function installedLabel(plugin: Plugin, targets: TargetStatus[]): string {
  const versions = [...new Set(targets.map((t) => t.installed[plugin.id]).filter((v) => v !== undefined))];
  if (!versions.length) return "Non installé";
  return "Installé : " + versions.map((v) => (v === "?" ? "version inconnue" : `v${v}`)).join(", ");
}

// --- rendering -----------------------------------------------------------------------

function render() {
  const running = overview?.running ?? [];
  showBanner(
    "running",
    running.length
      ? `Resolume ${escape(running.join(", "))} est ouvert : fermez-le pour installer ou retirer des plugins.`
      : null,
  );
  const locked = running.length > 0;

  const available = availableTargets();
  const enabled = enabledTargets();
  $("targets").innerHTML = available
    .map(
      (t) => `<label title="${escape(t.extraEffects)}">
        <input type="checkbox" data-target="${t.key}" ${enabled.some((e) => e.key === t.key) ? "checked" : ""} />
        ${escape(t.label)}${t.detected ? "" : ' <span class="muted">(sera créé)</span>'}
      </label>`,
    )
    .join("");

  if (!catalog) return;
  const plugins = catalog.manifest.plugins;
  if (!plugins.length) {
    $("plugins").innerHTML = '<p class="empty">Aucun plugin publié pour le moment.</p>';
  } else {
    $("plugins").innerHTML = plugins
      .map((p) => {
        const status = statusOf(p, enabled);
        const pct = progress.get(p.id);
        const installedSomewhere = enabled.some((t) => t.installed[p.id] !== undefined);
        const disabled = locked || busy.size > 0 ? "disabled" : "";
        const mainLabel = busy.has(p.id)
          ? pct !== undefined
            ? `${pct} %`
            : "…"
          : { install: "Installer", update: "Mettre à jour", uptodate: "À jour" }[status];
        return `<article class="plugin">
          <div class="info">
            <h2>${escape(p.displayName)} <span class="badge ${escape(p.type)}">${TYPE_LABELS[p.type] ?? escape(p.type)}</span></h2>
            <p class="description">${escape(p.description)}</p>
            <p class="versions">${escape(installedLabel(p, enabled))} · Disponible : v${escape(p.version)}</p>
          </div>
          <div class="buttons">
            <button data-action="install" data-id="${escape(p.id)}" class="${status}" ${status === "uptodate" ? "disabled" : disabled}>${mainLabel}</button>
            ${installedSomewhere ? `<button data-action="uninstall" data-id="${escape(p.id)}" class="ghost danger" ${disabled}>Désinstaller</button>` : ""}
          </div>
        </article>`;
      })
      .join("");
  }

  const pending = plugins.filter((p) => statusOf(p, enabled) === "update");
  const updateAll = $<HTMLButtonElement>("update-all");
  updateAll.disabled = locked || busy.size > 0 || pending.length === 0;
  updateAll.textContent = pending.length ? `Tout mettre à jour (${pending.length})` : "Tout mettre à jour";
}

async function renderFooter() {
  const version = await getVersion().catch(() => "?");
  const release = catalog ? ` · catalogue ${escape(catalog.manifest.release)}` : "";
  const offline = catalog?.offline ? " · hors ligne (dernier catalogue connu)" : "";
  $("footer-info").innerHTML = `Launcher v${escape(version)}${release}${offline}`;
}

// --- actions -------------------------------------------------------------------------

async function refreshOverview() {
  overview = await invoke<Overview>("overview");
}

async function refresh() {
  try {
    catalog = await invoke<Catalog>("fetch_catalog");
    await refreshOverview();
    showError(null);
  } catch (error) {
    showError(error);
  }
  render();
  renderFooter();
}

async function run(id: string, command: "install_plugin" | "uninstall_plugin") {
  busy.add(id);
  render();
  try {
    await invoke(command, { id, targets: enabledTargets().map((t) => t.key) });
    showError(null);
  } catch (error) {
    showError(error);
  } finally {
    busy.delete(id);
    progress.delete(id);
    await refreshOverview().catch(showError);
    render();
  }
}

/** Updates the plugins already installed; never installs new ones. */
async function updateAll() {
  const enabled = enabledTargets();
  for (const plugin of catalog?.manifest.plugins ?? []) {
    if (statusOf(plugin, enabled) === "update") await run(plugin.id, "install_plugin");
  }
}

async function checkLauncherUpdate() {
  let update: Update | null = null;
  try {
    update = await check();
  } catch (error) {
    console.info("Launcher update check skipped:", error);
    return;
  }
  if (!update) return;
  const found = update;
  showBanner(
    "launcher-update",
    `Tahua Launcher v${escape(found.version)} est disponible. <button id="install-update" type="button">Mettre à jour le launcher</button>`,
  );
  $("install-update").addEventListener("click", async (event) => {
    const button = event.currentTarget as HTMLButtonElement;
    button.disabled = true;
    button.textContent = "Téléchargement…";
    try {
      await found.downloadAndInstall();
      await relaunch();
    } catch (error) {
      showError(`Mise à jour du launcher impossible : ${error}`);
      button.disabled = false;
      button.textContent = "Réessayer";
    }
  });
}

window.addEventListener("DOMContentLoaded", () => {
  $("refresh").addEventListener("click", refresh);
  $("update-all").addEventListener("click", updateAll);

  $("plugins").addEventListener("click", (event) => {
    const button = (event.target as HTMLElement).closest<HTMLButtonElement>("button[data-action]");
    if (!button || button.disabled) return;
    const id = button.dataset.id!;
    run(id, button.dataset.action === "install" ? "install_plugin" : "uninstall_plugin");
  });

  $("targets").addEventListener("change", () => {
    const keys = [...document.querySelectorAll<HTMLInputElement>("#targets input:checked")].map(
      (input) => input.dataset.target!,
    );
    saveEnabledTargets(keys);
    render();
  });

  listen<Progress>("download-progress", ({ payload }) => {
    progress.set(payload.id, payload.total ? Math.floor((payload.downloaded * 100) / payload.total) : 0);
    render();
  });

  // Keep the "Resolume is running" banner current while the launcher is open.
  setInterval(async () => {
    if (!catalog || busy.size > 0) return;
    const before = overview?.running.join();
    await refreshOverview().catch(() => {});
    if (overview?.running.join() !== before) render();
  }, 4000);

  refresh();
  checkLauncherUpdate();
});
