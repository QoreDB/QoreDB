# QoreDB — Fiche de route Open Core

**Date** : février 2026
**Auteur** : document d'implémentation interne
**Portée** : repo QoreDB (app desktop) + site vitrine (marketing, pricing, paiement)
**Source** : basé sur l'audit `OPEN_CORE_AUDIT.md`

---

## Contexte

QoreDB est un client de bases de données desktop local-first (Tauri 2 + React 19 + Rust), développé en solo depuis ~1,5 mois. Le produit a atteint le stade V1 publique (CRUD complet, 5 drivers, éditeur SQL/Mongo, DataGrid performant, Vault chiffré, protections production).

Les features power-user (Sandbox, Visual Diff, ER Diagram) existent et ont reçu du feedback positif. D'autres killer features sont en stock.

L'objectif de cette fiche de route est de poser le cadre complet de la transition Open Core : ce qui change dans le repo QoreDB, ce qui doit être créé sur le site vitrine, et dans quel ordre.

---

## Projet 1 — Repo QoreDB (App Desktop)

### Phase 1 : Fondations licensing (semaines 1-2)

L'objectif est de poser le mécanisme de licence sans modifier le comportement actuel du produit. À la fin de cette phase, QoreDB fonctionne exactement comme avant, mais le socle technique pour le split existe.

#### 1.1 — Refactoring App.tsx

**Pourquoi** : App.tsx fait 900+ lignes et orchestre session, tabs, modals, shortcuts, recovery. Tout ajout de gating passera par ce fichier. Il faut le décomposer avant d'y ajouter de la logique de licence.

**Travail** :

- Extraire `SessionProvider` (gestion du cycle de vie des connexions)
- Extraire `TabProvider` (gestion des onglets, état, persistence)
- Extraire `ModalManager` (orchestration des modals globaux)
- Extraire `ShortcutProvider` (keyboard shortcuts globaux)
- App.tsx ne conserve que la composition de ces providers

**Résultat attendu** : App.tsx < 200 lignes, chaque provider est testable isolément.

**Estimation** : 2-3 jours.

#### 1.2 — Module `license/` côté Rust

**Pourquoi** : c'est le socle de tout le mécanisme de vérification. Sans lui, rien ne peut être gaté.

**Fichiers à créer** :

```
src-tauri/src/license/
├── mod.rs              # Exports publics
├── key.rs              # LicenseKey struct + validation Ed25519
└── status.rs           # LicenseTier enum (Core, Pro, Team, Enterprise)
```

**Implémentation** :

La struct `LicenseKey` contient email, tier, dates d'émission et d'expiration, machine_id optionnel, et signature Ed25519. La fonction `verify_license()` décode le base64, vérifie la signature avec la clé publique embarquée dans le binaire, et vérifie l'expiration. Tout est offline.

Dépendance Rust à ajouter : `ed25519-dalek` pour la vérification de signature, `base64` pour le décodage.

**Commandes Tauri à exposer** :

- `activate_license(key: String) -> Result<LicenseStatus, String>` — valide et persiste la clé
- `get_license_status() -> LicenseStatus` — retourne le tier actuel
- `deactivate_license() -> Result<(), String>` — supprime la clé locale

**Stockage de la clé** : la clé activée est persistée dans le Vault existant (même mécanisme que les credentials de connexion), pas en clair dans un fichier de config.

**Estimation** : 3 jours.

#### 1.3 — Hook et composants React côté frontend

**Fichiers à créer** :

```
src/lib/license.ts              # Hook useLicense() + types
src/components/License/
├── LicenseGate.tsx             # Wrapper conditionnel
├── LicenseActivation.tsx       # UI d'activation de clé
├── LicenseBadge.tsx            # Badge "Pro" discret
└── UpgradePrompt.tsx           # Message d'upgrade contextuel
```

**Le hook `useLicense()`** appelle `get_license_status` au montage, cache le résultat en mémoire, et expose `tier` et `isFeatureEnabled(feature)`. Il ne fait pas de polling — le statut ne change que quand l'utilisateur active/désactive une clé.

**Le composant `LicenseGate`** prend un `feature` string, un `children`, et un `fallback` optionnel. Si la feature n'est pas déverrouillée, il affiche le fallback (par défaut `UpgradePrompt`). Sinon il render les children.

**UX des prompts d'upgrade** (aligné avec le Design DNA) :

- Pas de boutons grisés, pas de modals bloquants, pas d'animation flashy
- Label "Pro" discret avec accent color `#6B5CFF`
- Le prompt apparaît uniquement quand l'utilisateur cherche activement la feature
- Preview avant achat quand c'est possible (ex : 3 changements sandbox gratuits)

**Estimation** : 1-2 jours.

#### 1.4 — Feature flags Cargo

**Modification de Cargo.toml** :

```toml
[features]
default = []
pro = []
team = ["pro"]
enterprise = ["team"]
```

Les features premium ne sont pas dans le binaire Core. Pour les activer, il faut compiler avec `--features pro`. C'est la protection de base : le code premium n'est physiquement pas dans le binaire distribué publiquement.

**Convention pour le gating dans le code Rust** :

- Les feature gates se concentrent dans `src-tauri/src/commands/` (couche IPC), jamais dans la logique métier
- Chaque commande Tauri premium retourne une erreur explicite en mode Core, jamais un crash silencieux
- Le moteur (`engine/`) ne sait jamais qu'il existe des tiers de licence

**Estimation** : 1 jour.

---

### Phase 2 : Premier split Core/Pro (semaines 3-4)

L'objectif est de gater les premières features premium et de produire deux builds distincts.

#### 2.1 — Extraction du Sandbox en module Rust isolé

**Pourquoi** : la logique sandbox est actuellement répartie entre `commands/sandbox.rs` et le frontend (sandboxStore, 7 composants). Il faut consolider côté Rust.

**Travail** :

- Créer `src-tauri/src/sandbox/` comme module Rust propre
- Y déplacer la logique de `commands/sandbox.rs`
- Wrapper avec `#[cfg(feature = "pro")]`
- Côté frontend : wrapper les composants Sandbox avec `LicenseGate`
- Limiter le mode Core à 3 changements sandbox (preview de la valeur), puis prompt d'upgrade

**Estimation** : 2 jours.

#### 2.2 — Séparation Interceptor basique / avancé

**Pourquoi** : le safety engine (block DROP/TRUNCATE en prod) doit rester Core. L'audit illimité, le profiling, et les rules custom passent sous feature flag.

**Travail** :

- `interceptor/pipeline.rs` + `interceptor/safety.rs` restent Core (compilés toujours)
- `interceptor/audit.rs` : l'audit basique (50 dernières entrées, pas de filtrage) reste Core. L'audit avancé (illimité, filtres, export) passe sous `#[cfg(feature = "pro")]`
- `interceptor/profiling.rs` : intégralement `#[cfg(feature = "pro")]`
- Côté frontend : les composants Interceptor/ affichent la version basique par défaut, les panels avancés sont sous `LicenseGate`

**Estimation** : 1 jour.

#### 2.3 — Feature-flagging Visual Diff et ER Diagram

**Visual Diff** : module UI autonome, couplage faible. Le gating est simple — wrapper le point d'entrée avec `LicenseGate`. Pas de version "limitée", c'est tout ou rien.

**ER Diagram** : composant isolé (`Schema/ERDiagram.tsx`). Même approche — `LicenseGate` sur le point d'entrée. Le schema browsing textuel reste Core.

**Estimation** : 0,5 jour chacun.

#### 2.4 — Page License dans Settings

Ajouter un onglet "Licence" dans le panneau Settings existant (`settingsConfig.ts`).

**Contenu** :

- Affichage du tier actuel (Core / Pro / Team)
- Champ d'activation de clé (copier-coller)
- Liste des features déverrouillées / verrouillées
- Lien vers le site pour acheter / upgrader
- Bouton de désactivation

**Estimation** : 1 jour.

#### 2.5 — CI dual-build

**Pourquoi** : chaque feature flag double la surface de test. Le CI doit compiler et tester les deux variantes.

**Travail** :

- `.github/workflows/build-core.yml` : `cargo build` sans feature flag, `cargo test`
- `.github/workflows/build-pro.yml` : `cargo build --features pro`, `cargo test --features pro`
- Matrice de test : `{core, pro} × {postgres, mysql, mongodb, sqlite, redis}` = 10 configurations
- Les tests Rust existants doivent passer dans les deux modes

**Estimation** : 1 jour.

---

### Phase 3 : IA BYOK (semaines 5-8)

L'IA n'est pas encore implémentée. L'avantage est qu'elle peut être conçue dès le départ comme premium.

#### 3.1 — Module `ai/` côté Rust

```
src-tauri/src/ai/
├── mod.rs              # Exports, #[cfg(feature = "pro")]
├── provider.rs         # Trait AIProvider + impls (OpenAI, Anthropic, Ollama)
├── context.rs          # Context builder (schéma, historique, driver-aware)
└── safety.rs           # Filtrage des requêtes générées avant exécution
```

**Architecture BYOK** : l'utilisateur fournit sa propre clé API. Aucun backend QoreDB n'est nécessaire. Les appels LLM partent directement de la machine de l'utilisateur. Cohérent avec la philosophie local-first et privacy-first.

**Providers supportés initialement** : OpenAI (GPT-4), Anthropic (Claude), Ollama (modèles locaux). Chaque provider implémente le trait `AIProvider`.

**Context management** : le context builder analyse le schéma de la connexion active (tables, colonnes, types, relations, virtual relations) et construit un prompt adapté au driver (SQL pour Postgres/MySQL, MQL pour MongoDB). Le contexte est reconstruit à chaque requête, pas caché entre sessions.

**Safety net** : toute requête SQL/MQL générée par l'IA passe par le safety engine existant avant exécution. L'utilisateur voit et confirme la requête avant qu'elle ne soit envoyée.

#### 3.2 — Interface frontend IA

- Panel d'assistant intégré dans le query editor (pas un chatbot plein écran)
- Configuration des clés API dans Settings (stockées dans le Vault, comme les credentials DB)
- Actions : générer une requête, expliquer un résultat, résumer un schéma, corriger une erreur
- Tout est sous `LicenseGate` feature "ai"

**Estimation totale Phase 3** : 3-4 semaines (le context management par driver est le gros du travail).

---

### Phase 4 : Maintenance et features premium futures (continu)

#### Nouvelles features premium à implémenter progressivement

Chaque nouvelle killer feature suit le même pattern :

1. Implémenter dans un module Rust isolé
2. Wrapper avec `#[cfg(feature = "pro")]`
3. Exposer via commandes Tauri
4. Côté frontend, utiliser `LicenseGate`
5. Ajouter aux tests CI dual-build

**Features en stock (ordre de priorisation à ajuster selon feedback)** :

- Export avancé (XLSX, Parquet) — ajout de writers dans `export/writers/`
- Custom Safety Rules — le `SafetyEngine` supporte déjà les règles custom, il suffit d'autoriser l'ajout sous flag
- Query Library avancée (dossiers, tags, snippets paramétrés)
- Virtual Relations auto-suggest
- Toute future killer feature conçue directement avec le flag

#### Gestion des licences dans le code

**Headers de fichier** :

```
// Fichiers Core
// SPDX-License-Identifier: Apache-2.0

// Fichiers Premium
// SPDX-License-Identifier: BUSL-1.1
```

**Fichiers racine** :

```
LICENSE                     # Apache 2.0 (core)
LICENSE-BSL                 # BSL 1.1 (premium)
```

---

## Projet 2 — Site Vitrine (Marketing, Pricing, Paiement)

### Prérequis : état actuel du site

Le site vitrine existe déjà. Il doit être étendu avec trois nouvelles pages/sections : pricing, activation, et paiement. Le tout doit rester cohérent avec le positionnement LinkedIn (technique, honnête, pas de hype marketing).

### 2.1 — Page Pricing

**Objectif** : présenter les tiers clairement et honnêtement, sans dark patterns.

**Structure des tiers** :

**Core (gratuit, open source)**

- Tous les drivers (Postgres, MySQL, MongoDB, SQLite, Redis)
- CRUD complet, éditeur SQL/Mongo, DataGrid performant
- Protections production, Vault chiffré, SSH tunneling
- Export CSV/JSON, historique, transactions
- Thèmes, i18n, keyboard shortcuts
- Apache 2.0

**Pro (payant, licence individuelle)**

- Tout ce qui est dans Core
- Sandbox mode + migration generator
- Visual Data Diff
- ER Diagram interactif
- Audit log et profiling avancés
- IA BYOK (clé API perso)
- Export avancé (XLSX, Parquet)
- Custom safety rules
- Query library avancée
- BSL 1.1 (passe en Apache 2.0 après 3-4 ans)

**Team (futur — payant, licence équipe)**

- Tout ce qui est dans Pro
- Sync multi-device
- Bibliothèques partagées
- Permissions fines
- IA managée
- Requiert un compte

**Principes UX de la page pricing** :

- Comparaison claire des tiers côte à côte
- Le Core est mis en valeur, pas présenté comme "la version pauvre"
- Pas de "features grisées" — chaque tier montre ce qu'il inclut, pas ce qu'il n'inclut pas
- FAQ transparente : "Pourquoi open core ?", "Mes données sont-elles envoyées quelque part ?", "Que se passe-t-il si j'arrête de payer ?"
- Prix affiché clairement (pas de "contactez-nous" pour le Pro)

**Note** : le price point exact sera déterminé séparément (brainstorm pricing à venir). La page doit être conçue pour accueillir facilement des ajustements de prix.

### 2.2 — Intégration Stripe

**Pourquoi Stripe** : standard du marché pour les devtools, excellente DX, support des licences perpétuelles et abonnements, webhooks fiables.

**Architecture** :

```
Utilisateur → Site vitrine → Stripe Checkout → Webhook → Génération de clé → Email
```

**Flux d'achat** :

1. L'utilisateur clique "Acheter Pro" sur la page pricing
2. Redirection vers Stripe Checkout (hosted page, pas d'intégration custom — moins de code, plus de confiance)
3. Stripe traite le paiement
4. Webhook Stripe notifie le backend du site
5. Le backend génère une clé de licence signée Ed25519
6. La clé est envoyée par email à l'utilisateur (via Stripe receipts ou un service email comme Resend/Postmark)
7. L'utilisateur colle la clé dans QoreDB → Settings → Licence

**Ce qu'il faut implémenter côté site** :

- Endpoint webhook Stripe (`/api/webhooks/stripe`)
- Module de génération de clés Ed25519 (la clé privée est sur le serveur, la clé publique est dans le binaire QoreDB)
- Template d'email de livraison de clé
- Page de confirmation post-achat avec la clé affichée + instructions

**Modèle de paiement** (à affiner lors du brainstorm pricing) :

- Pro : paiement unique (licence perpétuelle) ou abonnement annuel — à décider
- Team : abonnement mensuel/annuel par siège — futur
- Stripe gère les deux modèles nativement

**Dashboard Stripe** : permet de suivre les ventes, les remboursements, les abonnements actifs. Pas besoin de construire un back-office custom au départ.

### 2.3 — Page de gestion de licence (minimale)

Pour le tier Pro, une page minimale suffit :

- Vérifier le statut de sa licence (active, expirée)
- Récupérer sa clé si perdue (authentification par email)
- Pas de compte obligatoire : l'email d'achat + le payment ID Stripe suffisent pour identifier l'acheteur

Pour les tiers Team/Enterprise (futur) :

- Compte obligatoire avec OAuth2 (GitHub/Google)
- Dashboard de gestion des sièges
- Billing management via Stripe Customer Portal

### 2.4 — Contenu marketing

Le site doit refléter le positionnement LinkedIn : technique, honnête, centré sur l'expérience développeur.

**Pages à créer ou enrichir** :

- **Homepage** : proposition de valeur, screenshot/vidéo, CTA téléchargement Core + CTA upgrade Pro
- **Pricing** : comparaison des tiers (cf. 2.1)
- **Changelog** : liste des releases, ce qui est nouveau (renforce la confiance — "ce projet avance vite")
- **Docs** : documentation utilisateur minimale (installation, premiers pas, features)

**Ce qu'il ne faut pas faire** :

- Pas de landing page type SaaS avec des buzzwords vides
- Pas de témoignages inventés
- Pas de compteur "X développeurs font confiance à QoreDB" tant que le chiffre n'est pas significatif
- Pas de dark patterns (urgence artificielle, prix barré fictif)

---

## Récapitulatif des livrables et estimations

### Repo QoreDB

| Phase | Livrable                   | Estimation   |
| ----- | -------------------------- | ------------ |
| 1.1   | Refactoring App.tsx        | 2-3 jours    |
| 1.2   | Module license/ Rust       | 3 jours      |
| 1.3   | Hook + composants React    | 1-2 jours    |
| 1.4   | Feature flags Cargo        | 1 jour       |
| 2.1   | Extraction Sandbox         | 2 jours      |
| 2.2   | Séparation Interceptor     | 1 jour       |
| 2.3   | Feature-flagging Diff + ER | 1 jour       |
| 2.4   | Page License Settings      | 1 jour       |
| 2.5   | CI dual-build              | 1 jour       |
| 3     | IA BYOK complète           | 3-4 semaines |

**Total Phases 1-2** : ~13-15 jours de travail effectif.
**Total Phase 3** : ~3-4 semaines additionnelles.

### Site vitrine

| Livrable                      | Estimation |
| ----------------------------- | ---------- |
| Page Pricing                  | 2-3 jours  |
| Intégration Stripe + webhooks | 2-3 jours  |
| Génération de clés Ed25519    | 1 jour     |
| Email de livraison            | 1 jour     |
| Page gestion de licence       | 1 jour     |
| Changelog                     | 1 jour     |
| Docs minimales                | 2-3 jours  |

**Total site** : ~10-14 jours de travail effectif.

---

## Ordre d'exécution recommandé

L'idée est de paralléliser intelligemment les deux projets et de livrer incrémentalement.

**Sprint 1 (semaines 1-2)** — Fondations

- Repo : Phase 1 complète (refactoring, module license, hook React, feature flags)
- Site : page Pricing (le contenu, pas encore le paiement)

**Sprint 2 (semaines 3-4)** — Premier split + paiement

- Repo : Phase 2 complète (Sandbox, Interceptor, Diff, ER, CI dual-build)
- Site : intégration Stripe + génération de clés + email de livraison

**Milestone** : à la fin du Sprint 2, un utilisateur peut télécharger QoreDB Core gratuitement, acheter une licence Pro sur le site, recevoir sa clé par email, et débloquer les features premium. C'est le MVP Open Core.

**Sprint 3-6 (semaines 5-8)** — IA + polish

- Repo : Phase 3 (IA BYOK)
- Site : docs, changelog, page de gestion de licence

**Après** — Itération continue

- Nouvelles features premium au fil de l'eau
- Brainstorm et ajustement du pricing basé sur les données réelles
- Préparation du tier Team quand la demande se manifeste

---

## Risques et mitigations

**Risque 1 — L'estimation est optimiste pour un dev solo**

Les 13-15 jours des Phases 1-2 sont du temps de dev pur, sans compter le context switching, les bugs inattendus, et la charge cognitive de travailler sur l'infrastructure commerciale en parallèle du produit. Prévoir un buffer de 50% est prudent : compter 3-4 semaines réelles plutôt que 2.

**Mitigation** : ne pas bloquer le développement de features produit pendant la transition. Alterner des jours "open core infra" et des jours "features".

**Risque 2 — Le refactoring d'App.tsx révèle des couplages cachés**

L'audit identifie App.tsx comme un God Component, mais le couplage réel entre session, tabs, et modals pourrait être plus profond que ce qui est visible statiquement.

**Mitigation** : commencer par le refactoring (c'est en 1.1 pour cette raison). Si c'est plus complexe que prévu, ajuster le reste du planning. Ne pas faire le module license/ et le refactoring en parallèle.

**Risque 3 — Le paiement Stripe prend plus de temps que prévu**

La génération de clés Ed25519, les webhooks, la gestion des erreurs (paiement échoué, remboursement, double achat) — c'est un système financier, même simple, avec des edge cases.

**Mitigation** : utiliser Stripe Checkout (hosted) plutôt qu'une intégration custom. Ça délègue toute la gestion du paiement à Stripe. Le backend du site ne fait que recevoir un webhook et générer une clé.

**Risque 4 — La communauté perçoit mal le split**

C'est le risque stratégique principal identifié dans l'audit.

**Mitigation** : ne jamais retirer une feature déjà gratuite. Introduire le split avant la V1 stable publique si possible. Communiquer ouvertement sur LinkedIn (cohérent avec la posture éditoriale existante) : "voici pourquoi QoreDB passe en Open Core, voici ce qui reste gratuit pour toujours".

**Risque 5 — Se disperser entre produit et infrastructure commerciale**

Le temps passé sur le licensing, le site, Stripe, est du temps non investi sur les features. Pour un dev solo, c'est un coût d'opportunité réel.

**Mitigation** : implémenter le minimum viable à chaque étape. Pas de dashboard admin sophistiqué, pas de système de compte avant le tier Team, pas de DRM complexe. La meilleure protection, c'est un produit qui avance plus vite que les forks.

---

## Ce qui n'est PAS couvert par cette fiche de route

- **Pricing exact** : à déterminer lors d'un brainstorm dédié. La fiche de route est agnostique du price point.
- **Tier Team/Enterprise** : hors scope pour les 2-3 prochains mois. L'infrastructure (comptes, OAuth, sync) viendra quand la demande sera validée.
- **IA managée** : nécessite un backend complet (serveur, billing, rate limiting). Prématuré tant que le BYOK n'est pas validé.
- **Plugin system runtime** : l'audit conclut que les feature flags Cargo suffisent. Le dynamic loading serait un investissement disproportionné.
- **Marketing au-delà du site** : Product Hunt, campagnes, partenariats — hors scope technique.

---

_Ce document est la fiche de route d'implémentation technique. Il sera mis à jour au fur et à mesure de l'avancement._
