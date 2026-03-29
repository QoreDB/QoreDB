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

#### (Optionnel) Publication Microsoft Store (MSIX)

Si tu soumets un `.msix` dans Partner Center, le Store impose que le manifeste du package corresponde exactement à l'identité réservée dans le Store.

À récupérer dans **Partner Center → Identité du produit / Product identity** :

- **Package/Identity name** (ex: `QoreDB.QoreDB`) → à mettre dans le secret `MSIX_IDENTITY_NAME`
- **Publisher ID** (ex: `CN=B4F140BE-726E-4427-93B8-EAF78B5D2E9E`) → à mettre dans le secret `MSIX_PUBLISHER`

Ensuite, pour que la CI signe le package :

- `MSIX_CERT_BASE64` : le `.pfx` encodé en base64
- `MSIX_CERT_PASSWORD` : le mot de passe du `.pfx`

Important : `MSIX_PUBLISHER` doit être **exactement** la valeur Partner Center (zéro espace / zéro retour ligne). Même un caractère en plus change le `PackageFamilyName`.

Si tu génères le `.pfx` sur macOS via OpenSSL et que `signtool` échoue, essaie un export plus compatible :

```bash
openssl pkcs12 -export -legacy -out qoredb-store.pfx -inkey qoredb.key -in qoredb.crt -name "QoreDB Store Upload Signing"
```

Sur macOS, pour créer `MSIX_CERT_BASE64` à partir d'un `.pfx` (sans le commiter) :

```bash
base64 -i path/to/cert.pfx | pbcopy
```

Note : le **Package Family Name** est dérivé automatiquement de `Identity.Name` + `Publisher`.

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

| Plateforme           | Fichiers                                                        |
| -------------------- | --------------------------------------------------------------- |
| macOS ARM (M1/M2/M3) | `.dmg`, `.app.tar.gz`, `.app.tar.gz.sig`                        |
| macOS Intel          | `.dmg`, `.app.tar.gz`, `.app.tar.gz.sig`                        |
| Windows              | `.msi`, `.exe`, `.nsis.zip`, `.nsis.zip.sig`                    |
| Linux                | `.deb`, `.AppImage`, `.AppImage.tar.gz`, `.AppImage.tar.gz.sig` |
| Arch Linux (AUR)     | `qoredb-bin` (publié automatiquement à la release)              |

### Distribution AUR (Arch Linux)

Un package AUR `qoredb-bin` est publié automatiquement via `.github/workflows/aur-publish.yml` quand une release est publiée sur GitHub.

#### Prérequis (une seule fois)

1. **Créer le package sur AUR** :
   - Se connecter sur [aur.archlinux.org](https://aur.archlinux.org)
   - Créer le package `qoredb-bin`

2. **Générer une clé SSH pour le bot** :

   ```bash
   ssh-keygen -t ed25519 -C "qoredb-aur-bot" -f ~/.ssh/aur_qoredb
   ```

3. **Ajouter la clé publique** au compte AUR (SSH Keys dans les settings)

4. **Ajouter les secrets GitHub** :
   - `AUR_SSH_PRIVATE_KEY` : contenu de `~/.ssh/aur_qoredb`
   - `AUR_EMAIL` : email du compte AUR

#### Fonctionnement

Le workflow se déclenche quand une release est publiée (`release: published`). Il met à jour le `PKGBUILD` et `.SRCINFO` avec la nouvelle version, puis push sur le repo AUR.

Le package utilise le `.deb` de la release (et non l'AppImage) car le binaire du `.deb` est linkée dynamiquement contre les libs système (notamment webkit2gtk-4.1). Cela évite les crashes EGL/Wayland causés par les libs embarquées dans l'AppImage.

Les fichiers source sont dans `aur/` (PKGBUILD, .SRCINFO) et le script de mise à jour dans `scripts/update-aur.sh`.

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

## Rollback

En cas de problème après une release :

### Via l'auto-updater

1. Publier un hotfix (nouveau tag `vX.Y.Z+1`)
2. L'auto-updater Tauri distribue la correction automatiquement

### Rollback manuel

1. Supprimer la release problématique : `gh release delete vX.Y.Z --yes`
2. Supprimer le tag : `git push --delete origin vX.Y.Z && git tag -d vX.Y.Z`
3. Les utilisateurs qui n'ont pas encore mis à jour conservent l'ancienne version
4. Pour les utilisateurs déjà mis à jour : publier un patch pointant vers le code stable

### Rollback partiel (draft non publié)

Si la release est encore en draft, il suffit de supprimer le draft depuis GitHub.

---

## Auto-updater

L'app vérifie les mises à jour via :

```text
https://github.com/QoreDB/QoreDB/releases/latest/download/latest.json
```

Le fichier `latest.json` est généré automatiquement par `tauri-action`.

### Fichiers clés

- `src-tauri/tauri.conf.json` : version, endpoint, pubkey
- `src-tauri/src/lib.rs` : init du plugin updater
- `src/App.tsx` : UI de mise à jour
- `.github/workflows/release.yml` : workflow de release
