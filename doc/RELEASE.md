# Publication d'une nouvelle version (QoreDB)

## 1) Bump des versions

Mettre a jour ces champs (meme version partout):

- `src-tauri/tauri.conf.json` -> `version`
- `src-tauri/Cargo.toml` -> `version`
- `package.json` -> `version` (recommande pour coherence)

## 2) Build release (signe)

Pre-requis:

- Cle privee de signature Tauri disponible pour la build.
- Pubkey exposee a l'app via `TAURI_UPDATER_PUBKEY` (doit correspondre a la cle privee).

Commande:

```bash
pnpm tauri build
```

## 3) Publier sur GitHub Releases

- Creer une release `vX.Y.Z` sur GitHub.
- Uploader tous les artefacts generes par Tauri + `latest.json`.
- L'endpoint utilise par l'app est:

```
https://github.com/raphplt/QoreDB/releases/latest/download/latest.json
```

## 4) Verifications rapides

- Lancer l'app en prod.
- Un toast doit proposer l'installation si une version est disponible.
- L'utilisateur redemarre l'app apres installation (actuellement manuel).

## Fichiers cles

- `src-tauri/tauri.conf.json` (version + endpoint + pubkey)
- `src-tauri/src/lib.rs` (init du plugin updater)
- `src/App.tsx` (check/update UI)
