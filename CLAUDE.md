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
├── audits/             # Audits sécurité & conformité
├── internals/          # Architecture interne
├── private/            # Notes open-core (interne)
├── release/            # Process release & événements
├── rules/              # Standards UI/design & features
├── security/           # Modèle de menaces, politiques
├── tests/              # Contraintes de tests
└── todo/               # Roadmap & specs à venir
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

## Licensing Open Core (important)

- Le repo utilise un modèle **Open Core**.
- **Core** : licence Apache 2.0 (`LICENSE`)
- **Premium** : licence Business Source License 1.1 (`LICENSE-BSL`)
- Référence SPDX à utiliser pour Premium : `BUSL-1.1` (et non `BSL-1.1`)

### Règle obligatoire sur les fichiers code

Chaque fichier code `*.ts`, `*.tsx`, `*.rs` doit commencer par un header SPDX :

```ts
// SPDX-License-Identifier: Apache-2.0
```

ou, pour les fichiers Premium :

```ts
// SPDX-License-Identifier: BUSL-1.1
```

### Périmètre Premium actuel

Les fichiers suivants sont actuellement marqués Premium (`BUSL-1.1`) :

- `src/components/Diff/*`
- `src/components/Schema/ERDiagram.tsx`
- `src/lib/diffUtils.ts`
- `src-tauri/src/interceptor/profiling.rs`

Tout le reste est Core par défaut (`Apache-2.0`), sauf décision explicite contraire.

### Quand tu crées/déplaces un fichier

- Nouveau fichier : ajoute le header SPDX dès la création.
- Si un fichier passe de Core à Premium (ou inversement), mets à jour son header SPDX dans le même commit.
- Garde la cohérence entre le code et les licences racine (`LICENSE`, `LICENSE-BSL`).

## Documentation approfondie

Consulte ces fichiers selon le contexte de ta tâche :

| Sujet                    | Fichier                                        |
| ------------------------ | ---------------------------------------------- |
| Index docs               | `doc/README.md`                                |
| Vision produit           | `doc/PROJECT.md`                               |
| Features (liste)         | `doc/FEATURES.csv`                             |
| Design system UI         | `doc/rules/DESIGN_SYSTEM.md`                   |
| Fondations visuelles     | `doc/rules/VISUAL_FOUNDATION.md`               |
| Features (spécs)         | `doc/rules/FEATURES.md`                        |
| Spécificités drivers BDD | `doc/rules/DATABASES.md`                       |
| Sécurité / menaces       | `doc/security/THREAT_MODEL.md`                 |
| Sécurité / prod          | `doc/security/PRODUCTION_SAFETY.md`            |
| Audits sécurité          | `doc/audits/SECURITY_AUDIT.md`                 |
| Audits GDPR              | `doc/audits/GDPR_AUDIT.md`                     |
| Tests SSH                | `doc/tests/TESTING_SSH.md`                     |
| Limitations drivers      | `doc/tests/DRIVER_LIMITATIONS.md`              |
| Intercepteur de requêtes | `doc/internals/UNIVERSAL_QUERY_INTERCEPTOR.md` |
| URLs de connexion        | `doc/internals/connection-url-instructions.md` |
| Release process          | `doc/release/RELEASE.md`                       |
| Release events           | `doc/release/EVENTS.md`                        |
| Roadmap v2               | `doc/todo/v2.md`                               |
| Open-core roadmap (priv) | `doc/private/OPEN_CORE_ROADMAP_1.md`           |
| Open-core TODO (priv)    | `doc/private/OPEN_CORE_TODO.md`                |

## Règles générales

Applique l'internationalisation de manière systématique via `src/lib/i18n.ts`.
Pour les traductions, pense à toutes les langues, et écris dans un français clair et concis (avec les accents).
Utilise les composants UI de `src/components/ui/` autant que possible pour garantir la cohérence visuelle.
