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

## Principes de collaboration

Ces principes prennent le pas sur la vitesse : pour une tâche triviale, utilise ton jugement.

### 1. Réfléchir avant de coder

**Ne pas supposer. Ne pas masquer la confusion. Exposer les compromis.**

Avant d'implémenter :

- Énonce explicitement tes hypothèses. En cas de doute, demande.
- Si plusieurs interprétations sont possibles, présente-les — ne choisis pas en silence.
- Si une approche plus simple existe, dis-le. Pousse-la quand c'est justifié.
- Si quelque chose n'est pas clair, arrête-toi. Nomme ce qui est confus. Demande.

### 2. La simplicité d'abord

**Le minimum de code qui résout le problème. Rien de spéculatif.**

- Pas de fonctionnalités au-delà de ce qui a été demandé.
- Pas d'abstractions pour du code à usage unique.
- Pas de « flexibilité » ou de « configurabilité » non demandée.
- Pas de gestion d'erreur pour des scénarios impossibles.
- Si tu écris 200 lignes et que 50 suffiraient, réécris.

Pose-toi la question : « Un ingénieur senior dirait-il que c'est sur-compliqué ? » Si oui, simplifie.

### 3. Modifications chirurgicales

**Ne touche qu'à ce qui est nécessaire. Ne nettoie que ton propre désordre.**

Lors d'édition de code existant :

- Ne « améliore » pas le code, les commentaires ou le formatage adjacents.
- Ne refactorise pas ce qui n'est pas cassé.
- Respecte le style existant, même si tu ferais autrement.
- Si tu remarques du code mort non lié, signale-le — ne le supprime pas.

Quand tes changements créent des orphelins :

- Supprime les imports/variables/fonctions que TES changements ont rendu inutilisés.
- Ne supprime pas le code mort préexistant sauf demande explicite.

Le test : chaque ligne modifiée doit pouvoir se rattacher directement à la demande de l'utilisateur.

### 4. Exécution guidée par l'objectif

**Définir des critères de succès. Itérer jusqu'à vérification.**

Transforme les tâches en objectifs vérifiables :

- « Ajouter une validation » → « Écrire des tests pour les entrées invalides, puis les faire passer »
- « Corriger le bug » → « Écrire un test qui le reproduit, puis le faire passer »
- « Refactoriser X » → « S'assurer que les tests passent avant et après »

Pour les tâches en plusieurs étapes, énonce un plan bref :

```text
1. [Étape] → vérification : [contrôle]
2. [Étape] → vérification : [contrôle]
3. [Étape] → vérification : [contrôle]
```

Des critères de succès solides permettent d'itérer en autonomie. Des critères faibles (« faire que ça marche ») exigent une clarification permanente.

## Règles générales

Applique l'internationalisation de manière systématique via `src/lib/i18n.ts`.
Pour les traductions, pense à toutes les langues, et écris dans un français clair et concis (avec les accents).
Utilise les composants UI de `src/components/ui/` autant que possible pour garantir la cohérence visuelle.
Quand tu ajoutes une nouvelle fonctionnalité, pense à la documentation associée (README, doc/rules/FEATURES.md) et à la licence (header SPDX).
Commente le code avec parcimonie, les commentaires doivent seulement etre utiles à la compréhension du code, pas des réflexions internes.