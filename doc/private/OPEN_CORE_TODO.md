# QoreDB — TODO Open Core

---

## Projet 1 — Repo QoreDB

### Phase 1 : Fondations licensing (semaines 1-2)

**1.1 — Refactoring App.tsx**

- [x] Extraire `SessionProvider`
- [x] Extraire `TabProvider`
- [x] Extraire `ModalManager`
- [x] Extraire `ShortcutProvider`
- [x] Réduire App.tsx à < 200 lignes (composition de providers uniquement)
- [x] Vérifier que tout fonctionne comme avant (pas de régression)

**1.2 — Module `license/` côté Rust**

- [x] Ajouter dépendances `ed25519-dalek` et `base64` dans Cargo.toml
- [x] Créer `src-tauri/src/license/mod.rs`
- [x] Créer `src-tauri/src/license/status.rs` (enum `LicenseTier`)
- [x] Créer `src-tauri/src/license/key.rs` (struct `LicenseKey` + `verify_license()`)
- [x] Générer la paire de clés Ed25519 (privée → serveur site, publique → embarquée dans le binaire)
- [x] Implémenter la commande Tauri `activate_license`
- [x] Implémenter la commande Tauri `get_license_status`
- [x] Implémenter la commande Tauri `deactivate_license`
- [x] Stocker la clé activée dans le Vault existant
- [x] Tests unitaires : clé valide, clé expirée, clé invalide, absence de clé

**1.3 — Hook et composants React**

- [x] Créer `src/lib/license.ts` — types + hook `useLicense()`
- [x] Créer `src/components/License/LicenseGate.tsx`
- [x] Créer `src/components/License/LicenseActivation.tsx`
- [x] Créer `src/components/License/LicenseBadge.tsx`
- [x] Créer `src/components/License/UpgradePrompt.tsx`
- [x] Respecter le Design DNA : pas de modal bloquant, label Pro discret `#6B5CFF`

**1.4 — Feature flags Cargo**

- [x] Ajouter `[features]` dans `Cargo.toml` (`pro`, `team`, `enterprise`)
- [x] Vérifier que `cargo build` (sans flag) compile normalement
- [x] Vérifier que `cargo build --features pro` compile normalement
- [x] Définir la convention de gating : gates dans `commands/` uniquement

---

### Phase 2 : Premier split Core/Pro (semaines 3-4)

**2.1 — Extraction du Sandbox**

- [ ] Créer `src-tauri/src/sandbox/` (nouveau module Rust)
- [ ] Déplacer la logique de `commands/sandbox.rs` vers le nouveau module
- [x] Wrapper avec `#[cfg(feature = "pro")]`
- [x] Côté frontend : wrapper les composants `Sandbox/` avec `LicenseGate`
- [ ] Implémenter la limite Core (3 changements gratuits + prompt upgrade)
- [ ] Tester le mode Core (limite respectée, prompt affiché)
- [ ] Tester le mode Pro (sandbox illimité)

**2.2 — Séparation Interceptor basique / avancé**

- [ ] Garder `interceptor/pipeline.rs` et `interceptor/safety.rs` en Core
- [ ] Séparer audit basique (50 entrées) vs avancé (illimité, filtres, export) dans `audit.rs`
- [ ] Passer `interceptor/profiling.rs` sous `#[cfg(feature = "pro")]`
- [x] Côté frontend : `LicenseGate` sur les panels avancés d'Interceptor
- [ ] Tester les deux modes

**2.3 — Feature-flagging Visual Diff**

- [x] Wrapper le point d'entrée Diff avec `LicenseGate`
- [ ] Commande Tauri associée retourne erreur explicite en mode Core
- [ ] Tester

**2.4 — Feature-flagging ER Diagram**

- [x] Wrapper le point d'entrée ERDiagram avec `LicenseGate`
- [x] Le schema browsing textuel reste accessible en Core
- [ ] Tester

**2.5 — Page License dans Settings**

- [x] Ajouter onglet "Licence" dans `settingsConfig.ts`
- [x] Affichage du tier actuel
- [x] Champ d'activation de clé (copier-coller)
- [ ] Liste features déverrouillées / verrouillées
- [ ] Lien vers le site (achat / upgrade)
- [x] Bouton de désactivation

**2.6 — CI dual-build**

- [x] Créer `.github/workflows/build-core.yml`
- [x] Créer `.github/workflows/build-pro.yml`
- [x] Matrice de test : `{core, pro} × {postgres, mysql, mongodb, sqlite, redis}`
- [ ] Vérifier que tous les tests passent dans les deux modes
- [ ] Les builds Core et Pro produisent des binaires distincts

**2.7 — Fichiers de licence**

- [ ] Modifier si besoin `LICENSE` (Apache 2.0) à la racine
- [ ] Créer `LICENSE-BSL` (BSL 1.1) à la racine
- [ ] Ajouter les headers SPDX dans les fichiers Core (`Apache-2.0`)
- [ ] Ajouter les headers SPDX dans les fichiers Premium (`BUSL-1.1`)

**✅ MILESTONE** : à ce stade, QoreDB Core se build et se distribue publiquement. QoreDB Pro se build avec clé. Les features premium sont gatées.

---

### Phase 3 : IA BYOK (semaines 5-8)

**3.1 — Module `ai/` côté Rust**

- [ ] Créer `src-tauri/src/ai/mod.rs` sous `#[cfg(feature = "pro")]`
- [ ] Créer `src-tauri/src/ai/provider.rs` — trait `AIProvider`
- [ ] Implémenter provider OpenAI (GPT-4)
- [ ] Implémenter provider Anthropic (Claude)
- [ ] Implémenter provider Ollama (modèles locaux)
- [ ] Créer `src-tauri/src/ai/context.rs` — context builder
- [ ] Context builder : extraction du schéma de la connexion active
- [ ] Context builder : prompts adaptés par driver (SQL vs MQL)
- [ ] Créer `src-tauri/src/ai/safety.rs` — filtrage des requêtes générées
- [ ] Les requêtes IA passent par le safety engine existant avant exécution
- [ ] Commandes Tauri : `ai_generate_query`, `ai_explain_result`, `ai_summarize_schema`

**3.2 — Interface frontend IA**

- [ ] Panel assistant intégré dans le query editor
- [ ] Configuration des clés API dans Settings (stockage Vault)
- [ ] Sélection du provider (OpenAI / Anthropic / Ollama)
- [ ] Action : générer une requête à partir d'un prompt naturel
- [ ] Action : expliquer un résultat de requête
- [ ] Action : résumer un schéma
- [ ] Action : corriger une erreur SQL/MQL
- [ ] L'utilisateur voit et confirme la requête avant exécution
- [ ] Tout est sous `LicenseGate` feature "ai"
- [ ] Tests avec les trois providers

---

### Phase 4 : Features premium futures (continu)

- [ ] Export avancé : writer XLSX dans `export/writers/`
- [ ] Export avancé : writer Parquet dans `export/writers/`
- [ ] Custom Safety Rules : autoriser l'ajout de règles custom sous flag Pro
- [ ] Query Library avancée : dossiers, tags, snippets paramétrés
- [ ] Virtual Relations auto-suggest
- [ ] Chaque nouvelle feature suit le pattern : module isolé → flag → commande Tauri → LicenseGate → CI

---

## Projet 2 — Site Vitrine

### 2.1 — Page Pricing

- [ ] Maquette de la page (3 colonnes : Core / Pro / Team)
- [ ] Rédaction du contenu de chaque tier
- [ ] Core mis en valeur (pas présenté comme "version pauvre")
- [ ] FAQ : "Pourquoi open core ?", "Mes données sont-elles envoyées ?", "Si j'arrête de payer ?"
- [ ] Prix affiché clairement (après brainstorm pricing)
- [ ] CTA : télécharger Core / acheter Pro
- [ ] Intégration dans le site existant
- [ ] Responsive mobile

### 2.2 — Intégration Stripe

- [ ] Créer un compte Stripe (ou configurer l'existant)
- [ ] Créer le produit "QoreDB Pro" dans Stripe Dashboard
- [ ] Configurer Stripe Checkout (hosted page)
- [ ] Implémenter l'endpoint webhook `/api/webhooks/stripe`
- [ ] Gérer l'événement `checkout.session.completed`
- [ ] Gérer les edge cases : paiement échoué, remboursement, double achat
- [ ] Tester en mode Stripe Test avant passage en production

### 2.3 — Génération de clés Ed25519

- [ ] Module serveur de génération de clés (clé privée côté serveur)
- [ ] Génération automatique à la réception du webhook Stripe
- [ ] Format de la clé : base64 string contenant JSON payload + signature
- [ ] Tests : générer une clé → la vérifier avec la clé publique embarquée dans QoreDB

### 2.4 — Email de livraison

- [ ] Choisir un service d'email transactionnel (Resend, Postmark, ou Stripe receipts)
- [ ] Template d'email : clé de licence + instructions d'activation
- [ ] Envoi automatique après génération de la clé
- [ ] Page de confirmation post-achat avec la clé affichée + instructions

### 2.5 — Page gestion de licence (minimale)

- [ ] Vérifier le statut de sa licence (active / expirée)
- [ ] Récupérer sa clé par email (authentification email + payment ID)
- [ ] Pas de compte obligatoire pour le tier Pro

### 2.6 — Contenu additionnel

- [ ] Page Changelog (liste des releases)
- [ ] Documentation minimale : installation, premiers pas, features Core/Pro
- [ ] Mise à jour de la homepage : CTA téléchargement Core + CTA upgrade Pro
- [ ] Vérifier la cohérence avec le positionnement LinkedIn (technique, honnête, pas de hype)

**✅ MILESTONE** : un utilisateur peut télécharger Core, acheter Pro sur le site, recevoir sa clé par email, et débloquer les features premium. MVP Open Core complet.

---

## Communication

- [ ] Préparer un post LinkedIn annonçant le passage Open Core (ton : transparent, technique, honnête)
- [ ] Expliquer ce qui reste gratuit pour toujours
- [ ] Expliquer pourquoi ce choix (pérennité du projet solo)
- [ ] Ne jamais retirer une feature déjà perçue comme gratuite

---

## Hors scope (à traiter plus tard)

- [ ] Brainstorm pricing (price point exact)
- [ ] Tier Team/Enterprise (comptes, OAuth2, sync, permissions)
- [ ] IA managée (backend, billing, rate limiting)
- [ ] Plugin system runtime
- [ ] Product Hunt / campagnes marketing
