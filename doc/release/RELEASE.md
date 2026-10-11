# Publication d'une nouvelle version (QoreDB)

## Méthode recommandée : GitHub Actions (automatique)

### Prérequis (une seule fois)

1. **Générer une paire de clés de signature** :

   ```bash
   pnpm tauri signer generate -w ~/.tauri/qoredb.key
   ```

2. **Ajouter les secrets GitHub** :
   - Aller sur [Settings → Secrets → Actions](https://github.com/QoreDB/QoreDB/settings/secrets/actions)
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

1. Préparer la version avec `pnpm bump`. `package.json` est la source de
   vérité ; `tauri.conf.json` la référence directement. Le script synchronise
   les manifests Cargo du desktop et des deux outils. Actualiser `Cargo.lock`
   avec Cargo et vérifier les trois versions avant de commiter.
2. Qualifier le candidat dans une pull request du dépôt. Le workflow
   [Signed release candidate](../../.github/workflows/release-candidate.yml)
   compile Linux, Windows et macOS ARM/Intel avec les secrets de signature.
   Il conserve les paquets dans les artefacts Actions pendant 14 jours,
   sans créer de release ni publier de mise à jour.
3. Le job final vérifie les signatures de tous les paquets updater contre la
   clé publique embarquée et exige les quatre plateformes. Il conserve un
   `candidate-latest.json` de contrôle. Ce manifeste utilise les futures URLs
   de release : ce n'est pas un endpoint de téléchargement utilisable.
4. Terminer la recette d'installation et de mise à jour ainsi que les mesures
   natives. Une compilation signée réussie ne démontre pas ces parcours.
5. Seulement lorsque la publication est décidée, pousser le tag `vX.Y.Z` du
   commit qualifié. Le [workflow Release](../../.github/workflows/release.yml)
   construit les distributions, les signatures updater, les SBOM et checksums,
   puis publie automatiquement la release après assemblage de `latest.json`.
   Le draft intermédiaire n'est pas une étape d'approbation manuelle.

Les signatures updater sont distinctes des signatures système. macOS utilise
les secrets Apple et sa signature est contrôlée dans le workflow candidat ;
le MSIX du workflow Release est signé si son certificat est configuré. Le
workflow candidat ne qualifie pas le Microsoft Store ni une installation réelle.

### Plateformes générées

| Plateforme           | Fichiers                                                        |
| -------------------- | --------------------------------------------------------------- |
| macOS ARM (M1/M2/M3) | `.dmg`, `.app.tar.gz`, `.app.tar.gz.sig`                        |
| macOS Intel          | `.dmg`, `.app.tar.gz`, `.app.tar.gz.sig`                        |
| Windows              | `.msi`, `.exe`, signatures updater associées                    |
| Linux                | `.deb`, `.rpm`, `.AppImage`, signatures updater associées |
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
Cette commande active automatiquement le linkage DuckDB statique utilisé pour
les artefacts distribués. Un build Tauri lancé directement sans le script pnpm
doit préciser `--features duckdb-bundled`.

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

Le fichier `latest.json` est assemblé une seule fois par
[scripts/updater-json.mjs](../../scripts/updater-json.mjs), après les quatre builds.
Le script refuse une plateforme manquante, une cible en double, une signature
absente ou un tag incompatible avec la version du checkout. Les signatures sont
intégrées au manifeste avant suppression des fichiers `.sig` de la release.

Pour contrôler localement des paquets signés sans lancer QoreDB :

```bash
node --test scripts/updater-manifest.test.mjs scripts/verify-updater-artifacts.test.mjs
node scripts/verify-updater-artifacts.mjs /chemin/paquets candidate-latest.json
```

Le vérificateur nécessite [Minisign](https://jedisct1.github.io/minisign/) dans le
PATH, ou `MINISIGN_BIN` pointant vers son exécutable. Les tests cryptographiques
sont explicitement ignorés si cet outil manque ; le workflow candidat l'installe.
Le vérificateur n'envoie aucun fichier et ne modifie pas le profil utilisateur.

### Fichiers clés

- `src-tauri/tauri.conf.json` : version, endpoint, pubkey
- `src-tauri/src/lib.rs` : init du plugin updater
- `src/App.tsx` : UI de mise à jour
- `.github/workflows/release.yml` : workflow de release

## Manifest Qore AI Local

Le runtime et le modèle local peuvent être mis à jour sans publier une nouvelle
version de QoreDB. La source est :

```text
src-tauri/resources/qore-ai-local-manifest-v1.json
```

Après avoir modifié les artefacts, leurs tailles, leurs SHA-256 et incrémenté la
version `YYYY-MM-DD.N`, lancer manuellement le workflow
`Publish Qore AI Local manifest`.

Le workflow :

1. vérifie que les six couples macOS/Windows/Linux et ARM64/x86-64 sont présents ;
2. signe le JSON byte-exact avec `TAURI_SIGNING_PRIVATE_KEY` ;
3. remplace le JSON et sa signature sur la prerelease `qore-ai-local`.

QoreDB vérifie la signature avec la clé publique de l'auto-updater avant tout
parsing. Une version égale ou inférieure, une signature invalide ou une matrice
incomplète est ignorée au profit du manifest embarqué.

### Fichiers clés

- `src-tauri/resources/qore-ai-local-manifest-v1.json` : catalogue publié
- `src-tauri/src/ai/local_manifest.rs` : vérification et validation
- `.github/workflows/publish-ai-manifest.yml` : signature et publication
