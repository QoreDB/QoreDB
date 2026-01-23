# Publication d'une nouvelle version (QoreDB)

## Méthode recommandée : GitHub Actions (automatique)

### Prérequis (une seule fois)

1. **Générer une paire de clés de signature** :
   ```bash
   pnpm tauri signer generate -w ~/.tauri/qoredb.key
   ```

2. **Ajouter les secrets GitHub** :
   - Aller sur [Settings → Secrets → Actions](https://github.com/raphplt/QoreDB/settings/secrets/actions)
   - Ajouter `TAURI_SIGNING_PRIVATE_KEY` : contenu de `~/.tauri/qoredb.key`
   - Ajouter `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` : le mot de passe choisi

3. **La clé publique** est déjà dans `src-tauri/tauri.conf.json` (champ `pubkey`).

### Publier une release

1. **Bump les versions** dans :
   - `src-tauri/tauri.conf.json` → `version`
   - `src-tauri/Cargo.toml` → `version`
   - `package.json` → `version`

2. **Commit et tag** :
   ```bash
   git add .
   git commit -m "chore: bump version to X.Y.Z"
   git tag vX.Y.Z
   git push && git push --tags
   ```

3. **GitHub Actions** fait le reste automatiquement :
   - Build sur macOS (ARM + Intel), Windows, Linux
   - Signe tous les artefacts
   - Crée une **release draft** avec tous les fichiers

4. **Finaliser** :
   - Aller sur [Releases](https://github.com/raphplt/QoreDB/releases)
   - Éditer le draft, ajouter des notes de version
   - Publier la release

### Plateformes générées

| Plateforme | Fichiers |
|------------|----------|
| macOS ARM (M1/M2/M3) | `.dmg`, `.app.tar.gz`, `.app.tar.gz.sig` |
| macOS Intel | `.dmg`, `.app.tar.gz`, `.app.tar.gz.sig` |
| Windows | `.msi`, `.exe`, `.nsis.zip`, `.nsis.zip.sig` |
| Linux | `.deb`, `.AppImage`, `.AppImage.tar.gz`, `.AppImage.tar.gz.sig` |

---

## Méthode manuelle (build local)

### Build signé en local

```bash
export TAURI_SIGNING_PRIVATE_KEY=$(cat ~/.tauri/qoredb.key)
export TAURI_SIGNING_PRIVATE_KEY_PASSWORD="ton_password"
pnpm tauri build
```

Les artefacts sont générés dans `src-tauri/target/release/bundle/`.

### Publier manuellement

1. Créer une release sur GitHub
2. Uploader les artefacts + fichier `latest.json`

---

## Auto-updater

L'app vérifie les mises à jour via :
```
https://github.com/raphplt/QoreDB/releases/latest/download/latest.json
```

Le fichier `latest.json` est généré automatiquement par `tauri-action`.

### Fichiers clés

- `src-tauri/tauri.conf.json` : version, endpoint, pubkey
- `src-tauri/src/lib.rs` : init du plugin updater
- `src/App.tsx` : UI de mise à jour
- `.github/workflows/release.yml` : workflow de release
