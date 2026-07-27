# Fenêtre de chat agentique (Database Agent)

> Livrée en `v0.1.35`, avec trois écarts par rapport à la spec :
>
> - **Sélecteur de modèle** : déclaré hors périmètre (§ AI_REWORK 2.1/2.2), livré quand même —
>   le chat expose un sélecteur de modèle par provider, avec catalogue borné pour les providers
>   cloud connus et libre pour Ollama.
> - **Qore AI Local** : non prévu par la spec. Un septième provider embarque un runtime
>   `llama-server` (macOS, Windows, Linux), à builds reproductibles, installation reprenable et
>   vérification SHA-256 contre un manifest distant signé — l'agent tourne alors sans clé d'API.
> - **Tests de boucle** (Phase 3) : la logique de décision de l'orchestrateur (budget, batches
>   répétés, escalade de wrap-up, backoff de replay) est couverte par des tests unitaires sur
>   fonctions pures, pas par un harnais end-to-end avec provider HTTP mocké. Les tests de
>   round-trip par provider couvrent séparément la reconstruction des `tool_call` streamés.

Objectif : une fenêtre de chat dédiée, façon ChatGPT, où l'utilisateur formule sa demande en langage naturel ; l'agent explore la base lui-même (au besoin cross-environnement), exécute les requêtes, lit les résultats et répond. Écritures possibles sous confirmation (jamais en production), conversations persistantes. Tout le module reste Premium (BUSL-1.1, `#[cfg(feature = "pro")]`), BYOK uniquement.

## Relation avec `AI_REWORK.md`

État réel des acquis, vérifié dans le code (juillet 2026) :

- Phase 1 d'`AI_REWORK` (conversation multi-tour, contexte éditeur, extraction durcie) : livrée, réutilisée telle quelle (`AiRequest.history`, `EditorContext`, `AiMessageThread`, events `ai_stream:{id}`).
- Phase 2 d'`AI_REWORK` : non livrée. La partie erreurs typées + retries devient la Phase 1 de ce plan (prérequis d'une boucle multi-appels). Le listing dynamique des modèles et le sélecteur de modèle dans l'UI (2.1/2.2) restent hors périmètre.
- La Phase 3 d'`AI_REWORK` (« agent à outils » read-only, mono-connexion, `validate_query` en dry-run) est subsumée par ce document : ici l'agent exécute réellement les requêtes de lecture et lit les lignes, gère les écritures sous confirmation, et travaille sur plusieurs connexions.
- Les surfaces d'intégration de la Phase 4 d'`AI_REWORK` (Cmd+K, schema browser, etc.) restent indépendantes et hors périmètre ici.

Le serveur MCP (`crates/qore-mcp`) est la preuve d'exécution : il expose déjà à une IA des outils d'exploration en read-only forcé, via le pipeline de sécurité complet. Ce plan ré-héberge cette logique en interne, avec l'UI de l'app.

## Périmètre validé

- Cross-environnement inclus. L'agent démarre scopé sur la connexion active ; l'accès à une autre connexion ou à un autre environnement passe par une demande d'autorisation explicite.
- Écritures autorisées sous confirmation humaine en dev/staging ; bloquées d'office en production. L'utilisateur peut toujours reprendre la requête proposée et l'exécuter lui-même via le pipeline normal (qui a sa propre confirmation prod).
- Conversations persistantes multi-sessions (liste, reprise, renommage). Seuls les messages sont persistés, jamais les résultats de requêtes (option C, tranchée) : aucun résultat brut sur disque.
- Providers cloud garantis en mode agent : Anthropic, OpenAI, Gemini. Mistral et DeepSeek héritent du chemin OpenAI-compatible en best-effort. Ollama local en best-effort, avec dégradation gracieuse vers le mode texte si le modèle ne sait pas faire de tool-calling.
- Permissions : mémorisation « toujours autoriser » possible en dev/staging uniquement ; en production, confirmation redemandée à chaque action sensible, jamais mémorisée.
- Budget : plafonds par défaut (itérations d'agent, tokens par tour, timeout), tous ajustables dans les réglages IA.

## Architecture

```
UI Chat (onglet 'chat') ── events Tauri ──> Orchestrateur agent (src-tauri/src/ai/agent/)
                                                │
                            ┌───────────────────┼────────────────────┐
                            ▼                   ▼                    ▼
                    Providers tool-calling  Permission Gate      AgentTools
                    (Anthropic/OpenAI/       (scope, écriture,   (qore-service/src/agent_tools.rs,
                     Gemini/Ollama,           prod, cross-env)    partagé avec le MCP)
                     Mistral/DeepSeek                                  │
                     via OpenAI-compat)       ┌────────────────────────┤
                                              ▼                        ▼
                                     qore_service::query        SessionManager / Federation
                                     (preflight/execute,        (multi-connexion, environnement)
                                      safety, policy, audit)
```

## Modèle de permissions

Chaque appel d'outil de l'agent est classé avant exécution :

| Niveau | Cas | Comportement |
| --- | --- | --- |
| Auto | Lecture (`SELECT`) sur une connexion déjà dans le scope | Exécuté directement |
| Confirmation | Écriture (`is_mutation`) en dev/staging, accès à une nouvelle connexion, accès à un environnement staging/production, requête fédérée cross-env | Suspend la boucle, émet une `permission_request` → l'utilisateur autorise ou refuse |
| Bloqué | Toute mutation en production ; `is_dangerous` (DROP/TRUNCATE, `DELETE`/`UPDATE` sans `WHERE`) | Refusé ; l'agent reçoit un tool_result d'erreur explicite et doit s'adapter |

La classification s'appuie sur le pipeline existant : `qore_sql::safety::analyze_sql` fournit `is_mutation`/`is_dangerous` via `preflight`, `SafetyEngine` applique les règles, `SafetyPolicy` la configuration prod.

Mémorisation d'une autorisation : scoping par connexion, valable pour la session de l'app, en dev/staging seulement. En production, aucune mémorisation.

## Phase 0 — Fondations et outils partagés

- Extraire la logique des méthodes `do_list_tables`, `do_describe_table`, `do_run_query`, `do_list_namespaces` de `crates/qore-mcp/src/main.rs` vers un module partagé `qore-service/src/agent_tools.rs`, appelé par le MCP et par l'orchestrateur interne. Point de vérité unique. Le read-only n'y est plus implicite mais paramétré : le MCP continue de forcer read-only sur les sessions qu'il ouvre ; l'agent interne travaille sur les sessions déjà ouvertes de l'app.
- `list_connections` du MCP (lecture du vault, pour ouvrir des sessions) reste dans le MCP : l'outil homonyme de l'agent interne listera les sessions ouvertes (`SessionManager`) et relève de la Phase 4 — rien à factoriser ici.
- Types internes de tool-calling dans `src-tauri/src/ai/agent/types.rs` : `AgentTool { name, description, input_schema }`, `ToolCall { id, name, input }`, `ToolResult { id, content, is_error }`, `AgentMessage { role, content, tool_calls, tool_results }`.

Vérification : `cargo check` vert ; le MCP compile et se comporte à l'identique en passant par la couche factorisée.

## Phase 1 — Erreurs typées et retries (prérequis, reprend AI_REWORK 2.3)

- `ai/types.rs` : `AiError { InvalidKey, RateLimited { retry_after }, ContextTooLarge, Network, Provider(String) }`, sérialisé vers le front avec un `kind`.
- Providers : mapper les statuts HTTP (401/403 → InvalidKey, 429 → RateLimited, 5xx/transport → Network) au lieu d'aplatir en `String`.
- Retry autour de l'envoi initial (jamais d'un stream entamé) : 2 tentatives supplémentaires sur 429/5xx/réseau, backoff 1 s puis 3 s.
- Parser `usage` dans le flux → `tokens_used` renseigné (nécessaire au budget de tokens de la boucle agentique).

Vérification : tests wiremock (401/429/500/timeout, backoff) ; `cargo check`.

## Phase 2 — Tool-calling multi-provider

- Étendre `src-tauri/src/ai/provider.rs` : ajouter le champ `tools` aux corps de requête et le parsing des tool_calls en streaming (accumulation des arguments JSON partiels).
  - Anthropic : blocs `tool_use` / `tool_result`.
  - OpenAI : `tools` (functions), `tool_calls`, messages de rôle `tool` — Mistral et DeepSeek héritent via `stream_openai_compatible` (best-effort).
  - Gemini : `functionDeclarations`, `functionCall` / `functionResponse`.
  - Ollama : détection de capacité tool-calling ; fallback vers le mode texte one-shot si absent (non bloquant).
- Les événements de tool (`tool_call_started`, `tool_result`…) sont portés par le canal `agent_stream` de la Phase 3 ; le canal provider ne transporte que les deltas de texte, les tool calls remontent dans `AgentTurn`.

Vérification : tests de round-trip par provider avec HTTP mocké (traduction des outils, reconstruction d'un tool_call streamé) ; `cargo check`.

## Phase 3 — Orchestrateur agentique et sécurité

- `src-tauri/src/ai/agent/orchestrator.rs` : boucle `spawn` — appel provider → si `tool_calls`, passage par le Permission Gate → exécution via `agent_tools` → réinjection des `tool_result` → itération. Plafonds : itérations max, budget de tokens, timeout total, tous ajustables.
- `src-tauri/src/ai/agent/permissions.rs` : classification Auto/Confirmation/Bloqué (scope de connexions, écriture, environnement, fédéré), branchée sur les résultats de `preflight` et sur `SafetyEngine`/`SafetyPolicy`.
- Outils exposés à l'agent :

| Outil | Contrat | Permission | Garde-fous |
| --- | --- | --- | --- |
| `list_connections` | → sessions ouvertes + environnement | Auto | — |
| `list_namespaces` | connexion → bases/schémas | Auto (Confirmation hors scope) | — |
| `list_tables` | namespace → tables | Auto | borné par le driver |
| `describe_table` | table → colonnes/FK/index | Auto | redaction PII existante |
| `run_query` | `SELECT` → lignes réelles | Auto (lecture, connexion en scope) | read-only forcé, cap de lignes, timeout, redaction des résultats |
| `run_mutation` | `INSERT`/`UPDATE`/`DELETE` | Confirmation en dev/staging ; `EXPLAIN` préalable pour estimer l'impact ; bloqué en production | `analyze_sql`, mono-connexion |
| `run_federated_query` | SQL cross-connexion | Confirmation | Federation SELECT-only, cap 100k lignes/source |

- Sécurité transverse :
  - Nouveau champ `source` (`user` par défaut, `ai` pour l'agent) dans `QueryContext` et `AuditLogEntry`, propagé par `preflight`/`execute` — n'existe pas aujourd'hui, à créer dans `qore-service`. L'audit de chaque tool call passe par l'interceptor existant, avec cette source.
  - Écriture uniquement si confirmée, mutation en prod bloquée quel que soit le niveau de danger.
  - Redaction des résultats réinjectés : API de redaction de valeurs de lignes construite sur `crate::redaction` (détection de colonnes sensibles), appliquée par l'orchestrateur aux tool_results avant réinjection au modèle (`qore-service` ne peut pas dépendre du binaire : la redaction vit côté orchestrateur).
  - Prompt-injection depuis les données : délimiter les tool_results comme contenu non fiable, conserver le `SAFETY_FOOTER` ; toute action sensible passant par confirmation humaine, l'impact reste borné.
- Events Tauri `agent_stream:{id}` : `text_delta | tool_call_started | tool_result | permission_request | done | error`.
- Commandes Tauri (`src-tauri/src/commands/agent.rs`) : `agent_send_message`, `agent_respond_permission`, `agent_cancel`.

Vérification : tests de boucle avec provider mocké (multi-tours d'outils) ; matrice de gating (lecture auto, écriture confirmée, mutation prod bloquée, escalade de scope) ; `cargo check`. Critère d'acceptation : sur la base `docker-compose`, « combien de commandes par client le mois dernier ? » déclenche une exploration puis une requête exécutée qui répond du premier coup.

## Phase 4 — Cross-environnement et Federation

- Nouvel outil `list_connections` : sessions ouvertes avec leur environnement (`SessionManager::list_sessions`, `get_environment`, `connection_key`) — code nouveau, distinct de l'outil vault du MCP.
- `run_federated_query` câblé sur la Federation existante (SELECT-only, JOIN cross-serveur via DuckDB), gated en Confirmation. L'alias map est construite uniquement à partir des connexions déjà dans le scope de la conversation — le gating d'environnement se fait à l'entrée du scope, pas besoin d'enrichir `AliasEntry`.
- Tout accès à une connexion hors du scope initial déclenche une `permission_request`.
- Cohérence : les écritures restent mono-connexion (Federation est en lecture seule) ; l'agent cible une session précise pour muter, jamais en fédéré.

Vérification : test cross-env lecture (deux connexions mockées) ; refus d'accès hors scope tant que non autorisé ; `cargo check`.

## Phase 5 — Persistance des conversations

- Modèle `Conversation { id, title, created_at, updated_at, messages, scope }`, stocké dans le répertoire de données de l'app.
- Option C : seuls les messages sont persistés. Les étapes d'outil sont sauvegardées sous forme de résumé (outil, requête, nombre de lignes), jamais les lignes elles-mêmes ; à la reprise, les tableaux de résultats ne sont pas restaurés.
- Commandes Tauri : `chat_list_conversations`, `chat_load_conversation`, `chat_save_conversation`, `chat_rename_conversation`, `chat_delete_conversation`.
- Titre auto-généré (résumé du premier message par le modèle).

Vérification : CRUD des conversations en round-trip ; relecture après redémarrage ; aucun résultat de requête dans les fichiers persistés ; `cargo check`.

## Phase 6 — Frontend (onglet Chat)

- Nouveau `TabType 'chat'` dans `src/lib/tabs.ts` (+ factory `createChatTab`) + branche de rendu dans `AppContent` (`src/AppLayout.tsx`).
- `src/components/Chat/` : layout à deux colonnes (liste de conversations à gauche, fil à droite).
- Réutilisation : `AiMessageThread`, `AiResponseDisplay`, `AiPromptInput` promus/factorisés ; sélecteur de provider existant (`AiProviderSelector`). Pas de sélecteur de modèle (hors périmètre, cf. AI_REWORK 2.2).
- Nouveaux composants :
  - Cartes d'étape d'outil (« Décrit `users`… », « Exécute la requête… ») avec résultat repliable.
  - Tableaux de résultats inline (`DataGrid` en `readOnly` + `footerMode="none"` + hauteur bornée, comme dans le Notebook).
  - Cartes de permission (« L'agent veut accéder à `prod-pg` / exécuter une écriture — Autoriser / Refuser / Toujours autoriser »).
  - Indicateur de scope (connexions visibles par l'agent).
- Manques UI à combler : composant `<Markdown>` partagé (factoriser les deux usages directs de `react-markdown`), textarea auto-resize, coloration SQL des blocs (réutiliser `Editor/SQLEditor` en `readOnly`).
- Hook `useAgentChat` (dérivé de `useAiAssistant`) gérant le protocole d'events et les permissions.

Vérification : `pnpm lint:fix` ; parcours manuel — question NL → exploration → demande de permission → exécution → tableau inline → réponse finale ; reprise d'une conversation sauvegardée.

## Phase 7 — i18n, licence, documentation, release

- i18n : namespace dédié dans les 9 locales (`en, fr, es, de, pt-BR, zh-CN, ja, ko, ru`), français avec accents.
- Licence : header SPDX `BUSL-1.1` sur tout nouveau fichier ; mettre à jour la section « Current Premium scope » de `CLAUDE.md`.
- Documentation : entrée dans `doc/FEATURES.csv`, section README, changelog, bump de version ; archiver cette spec dans `doc/archive/` après release.
- Durcissement final : matrice de tests sécurité (mutation prod bloquée, écriture confirmée, escalade de scope, injection) ; revue via `/code-review` ou l'auditeur QA/sécurité.

## Transverse (toutes phases)

- SPDX `BUSL-1.1` sur tout nouveau fichier.
- `doc/FEATURES.csv` mis à jour à chaque phase livrée.
- Tests : chaque phase ajoute ses tests ; HTTP mocké (wiremock) pour les providers dès la Phase 1.
- Aucune télémétrie ni appel réseau hors du provider choisi (positionnement local-first inchangé).
