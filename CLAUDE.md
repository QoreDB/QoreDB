# QoreDB

Client desktop de bases de données moderne construit avec **Tauri 2 + React 19 + Rust**.
Alternative légère et rapide à DBeaver/pgAdmin pour développeurs.

## Stack technique

| Couche   | Technologies                                         |
| -------- | ---------------------------------------------------- |
| Frontend | React 19, TypeScript, Vite 7, Tailwind 4, CodeMirror |
| Backend  | Rust (edition 2021), Tauri 2, SQLx, tokio            |
| BDD      | PostgreSQL, MySQL, MongoDB, SQLite                   |

## Structure du projet

```
src/                    # Frontend React/TypeScript
├── components/         # Composants UI (Browser/, Query/, Results/, ui/)
├── hooks/              # Hooks React (useTabs, useTheme, useKeyboardShortcuts)
├── lib/                # Bindings Tauri, utilitaires, types
└── locales/            # Traductions i18n (en.json, fr.json)

src-tauri/              # Backend Rust
├── src/commands/       # Handlers Tauri (query, mutation, export, vault)
├── src/engine/         # Abstraction BDD (traits.rs, drivers/, session_manager)
└── src/vault/          # Gestion credentials chiffrés

doc/                    # Documentation détaillée
├── rules/              # Standards UI/design
├── security/           # Modèle de menaces, politiques
├── internals/          # Architecture interne
└── todo/               # Roadmap features
```

## Commandes essentielles

```bash
pnpm install            # Installer les dépendances
pnpm tauri dev          # Lancer l'app en dev (hot reload)
pnpm lint:fix           # Linter + fix automatique
pnpm format:write       # Formater le code
pnpm test               # Tests Rust (cargo test)
pnpm tauri build        # Build production
```

Docker pour les BDD de test : `docker-compose up -d`

## Architecture clé

**Frontend → Backend** : Les appels passent par `src/lib/tauri.ts` qui expose des bindings typés vers les commandes Rust.
**Drivers BDD** : Chaque driver implémente le trait `DataEngine` (`src-tauri/src/engine/traits.rs`). Le `DriverRegistry` gère l'instanciation.
**Sécurité** : Vault chiffré (Argon2), validation SQL avant exécution (`sql_safety.rs`), mode sandbox.

## Conventions

- Composants UI réutilisables dans `src/components/ui/` (basés sur shadcn/Radix)
- Hooks personnalisés préfixés `use*` dans `src/hooks/`
- Commandes Tauri dans `src-tauri/src/commands/`, exports dans `lib.rs`
- Erreurs Rust : types custom dans `engine/error.rs`, propagation avec `?`

## Documentation approfondie

Consulte ces fichiers selon le contexte de ta tâche :

| Sujet                        | Fichier                              |
| ---------------------------- | ------------------------------------ |
| Vision produit               | `doc/PROJECT.md`                     |
| Design system UI             | `doc/rules/DESIGN_SYSTEM.md`         |
| Fondations visuelles         | `doc/rules/VISUAL_FOUNDATION.md`     |
| Spécificités drivers BDD     | `doc/rules/DATABASES.md`             |
| Sécurité / menaces           | `doc/security/THREAT_MODEL.md`       |
| Tests SSH                    | `doc/tests/TESTING_SSH.md`           |
| Limitations drivers          | `doc/tests/DRIVER_LIMITATIONS.md`    |
| Intercepteur de requêtes     | `doc/internals/UNIVERSAL_QUERY_INTERCEPTOR.md` |
| URLs de connexion            | `doc/internals/connection-url-instructions.md` |
| Roadmap v2                   | `doc/todo/v2.md`                     |

## Règles générales

Applique l'internationalisation de manière systématique via `src/lib/i18n.ts`.
Pour les traductions, pense à toutes les langues, et écris dans un français clair et concis (avec les accents).
Utilise les composants UI de `src/components/ui/` autant que possible pour garantir la cohérence visuelle.