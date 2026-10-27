# Tahua Launcher

Application Windows et macOS pour installer, mettre à jour et désinstaller les plugins FFGL [Tahua](https://github.com/JVlatt/tahua-plugins) dans Resolume Arena / Avenue 7.

- La liste des plugins est dynamique : elle vient du `manifest.json` de la dernière release de tahua-plugins. Un nouveau plugin publié y apparaît sans mettre à jour le launcher.
- Le launcher installe le bon binaire selon l'OS (`.dll` sous Windows, `.bundle` universel sous macOS) dans `Documents/Resolume Arena/Extra Effects` et/ou `Documents/Resolume Avenue/Extra Effects`.
- Chaque téléchargement est vérifié par sha256.
- Les installations sont bloquées tant que Resolume tourne, car il verrouille les plugins chargés.
- Le launcher se met lui-même à jour : un bandeau propose la nouvelle version au démarrage.

## Installer le launcher

Télécharge la dernière version dans les [Releases](https://github.com/JVlatt/tahua-launcher/releases/latest) :

| OS | Fichier |
|---|---|
| Windows 10/11 x64 | `Tahua.Launcher_<version>_x64-setup.exe`. L'installation se fait pour l'utilisateur courant, sans droits administrateur. |
| macOS 10.15+, Intel et Apple Silicon | `Tahua.Launcher_<version>_universal.dmg` |

L'application n'est pas signée par un certificat payant, donc l'OS avertit au **premier lancement** :

- **Windows (SmartScreen)** : « Informations complémentaires », puis « Exécuter quand même ».
- **macOS (Gatekeeper)** : copie l'app dans Applications. Au premier lancement, fais clic droit sur l'app puis **Ouvrir**, et confirme. Si macOS dit que l'app est endommagée :
  ```bash
  xattr -dr com.apple.quarantine "/Applications/Tahua Launcher.app"
  ```

Les mises à jour suivantes du launcher se font depuis l'app.

Les plugins installés par le launcher ne sont pas mis en quarantaine par macOS : aucune commande `xattr` n'est nécessaire pour eux.

## Utilisation

1. Ferme Resolume.
2. Coche la ou les destinations (Arena, Avenue). Seuls les produits détectés sur la machine sont proposés.
3. Clique **Installer**, **Mettre à jour** ou **Désinstaller** sur un plugin. **Tout mettre à jour** ne met à jour que les plugins déjà installés.
4. Relance Resolume.

Un plugin copié à la main, sans le launcher, apparaît comme « version inconnue ». Un clic sur **Mettre à jour** le remplace par la version du catalogue.

## Développement

Prérequis :
- [Rust](https://rustup.rs) stable ;
- Node.js LTS ;
- sous Windows, les Build Tools C++ (MSVC) et WebView2, présent par défaut sur Windows 10/11 ;
- sous macOS, les outils Xcode en ligne de commande.

```bash
npm install
npm run tauri dev      # lance l'app en mode développement
cd src-tauri && cargo test   # tests unitaires du backend
```

Structure :

```
src-tauri/src/
  manifest.rs   format du manifest.json publié par tahua-plugins (schemaVersion 1)
  targets.rs    dossiers Extra Effects d'Arena / Avenue (Documents réel, même redirigé)
  state.rs      versions installées (installed.json dans le dossier de données de l'app)
  installer.rs  vérification sha256, extraction, remplacement, détection de Resolume
  lib.rs        commandes appelées par l'UI
src/main.ts     UI (TypeScript sans framework), index.html, src/styles.css
```

Pour tester avec un autre catalogue, définis la variable d'environnement `TAHUA_MANIFEST_URL` avant de lancer l'app.

Le manifeste est généré par la CI de tahua-plugins. Un changement incompatible de son format doit incrémenter `schemaVersion`, et le launcher doit être mis à jour en conséquence.

## Publier une version

1. Mets la même version dans `src-tauri/tauri.conf.json`, `src-tauri/Cargo.toml` et `package.json`. C'est celle de `tauri.conf.json` qui fait foi.
2. Fais le commit, puis le tag :
   ```bash
   git tag v0.1.1
   git push origin v0.1.1
   ```
3. La CI vérifie que le tag correspond à la version, compile les installeurs Windows et macOS universel, puis publie la release avec le `latest.json` lu par la mise à jour automatique.

### Clé de signature des mises à jour

Les mises à jour du launcher sont signées. La clé publique est dans `tauri.conf.json` (`plugins.updater.pubkey`). La clé privée est stockée :
- dans le secret GitHub `TAURI_SIGNING_PRIVATE_KEY` ;
- dans `~/.tauri/tahua-launcher.key` sur la machine où elle a été générée.

**Garde-en une sauvegarde hors de cette machine.** Sans elle, plus aucune mise à jour ne pourra être publiée aux launchers déjà installés.

## Licence

[MIT](LICENSE)
