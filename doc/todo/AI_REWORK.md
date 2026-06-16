# Refonte de l'assistant IA

Objectif : passer d'un générateur SQL one-shot à un assistant conversationnel qui voit ce que l'utilisateur fait, capable d'explorer la base lui-même, fiable au quotidien. Tout le module reste Premium (BUSL-1.1, `#[cfg(feature = "pro")]`), BYOK uniquement.

Constat de départ (analyse juin 2026) :

- Mono-tour strict : chaque requête envoie system + user, pas d'historique. Le panneau efface la réponse précédente (`useAiAssistant.ts`).
- Contexte aveugle : l'IA reçoit le schéma mais ni la requête en cours d'édition, ni les résultats, ni la table ouverte.
- Extraction de la requête générée par regex sur code-blocks (`provider.rs::extract_query_from_response`) — fragile.
- Listes de modèles hardcodées datant de mi-2025 (`types.rs::available_models`), IDs Gemini preview probablement morts. `AiConfig.model` et `base_url` jamais exposés dans l'UI.
- Aucun retry, erreurs aplaties en string, `tokens_used` toujours `None`.
- `ai_summarize_schema` implémenté côté backend, jamais branché côté UI.

Les phases sont indépendantes et livrables séparément, dans cet ordre de priorité.

---

## Phase 1 — Conversation multi-tour + contexte riche

Le cœur du ressenti « bancal ». Les deux changements touchent les mêmes structures (`AiRequest`, construction du prompt, panneau) : à faire ensemble.

### 1.1 Backend : messages[] dans le protocole

- `types.rs` : ajouter `AiMessage { role: Role, content: String }` (`Role = System | User | Assistant`) et `AiRequest.history: Vec<AiMessage>` (default vide pour compat).
- Changer la signature du trait `AIProvider::stream` : remplacer `system_prompt: &str, user_prompt: &str` par `messages: &[AiMessage]`. Adapter les 6 providers :
  - OpenAI / Mistral / DeepSeek / Ollama : mapping direct vers `messages[]`.
  - Anthropic : `system` à part + `messages[]` user/assistant alternés.
  - Gemini : `systemInstruction` + `contents[]` avec roles `user`/`model`.
- `commands/ai.rs::stream_ai_request` : construire `[system] + history + [user courant]`.
- Borner l'historique : garder les N derniers messages dans une enveloppe de caractères (ex. 24 000 chars, tronquer par paires les plus anciennes). Constante dans `context.rs` à côté de `MAX_SCHEMA_WORDS`.
- `validate_user_prompt` s'applique au prompt courant ; l'historique est borné mais pas re-validé (il vient de nous).

Vérification : tests unitaires sur le mapping messages par provider (corps JSON généré) et sur la troncature d'historique ; `cargo test` vert ; manuel : « génère X » puis « ajoute un filtre sur la date » produit une requête qui amende la précédente.

### 1.2 Backend : contexte d'éditeur

- `AiRequest` : ajouter `editor_context: Option<EditorContext>` avec `{ current_query, active_table, last_error, result_shape }` (tous optionnels).
- `build_user_prompt` (ou un bloc dans le system prompt) : injecter ces éléments quand présents, clairement délimités (« Current query in editor: … »).
- `result_shape` = colonnes + types + nombre de lignes, jamais de valeurs (les valeurs restent opt-in, voir 1.5).

Vérification : test unitaire sur le prompt construit avec/sans contexte ; manuel : « explique ce que fait ma requête » sans coller la requête fonctionne.

### 1.3 Frontend : fil de conversation

- `useAiAssistant.ts` : remplacer `response: string` par `messages: AiMessage[]` (+ état streaming du dernier message). Chaque échange est conservé ; `reset()` vide le fil.
- `AiRequest` côté `lib/ai.ts` : ajouter `history` et `editor_context` ; QueryPanel passe la requête courante de l'éditeur, la table active, la dernière erreur.
- Nouveau composant `AiMessageThread.tsx` (le panneau dépassera 500 lignes sinon) : bulles user/assistant, requête générée + badges safety + actions par message assistant, bouton « nouvelle conversation ».
- Rendu markdown des réponses (réutiliser le rendu markdown existant s'il y en a un, sinon `react-markdown` déjà présent ou équivalent léger).
- Élargir le panneau (`w-80` → redimensionnable ou `w-96` minimum).

Vérification : `pnpm lint:fix` + manuel : itération sur 3 tours, insertion de la requête du 2e tour, reset, panneau fermé/rouvert conserve le fil tant que l'onglet vit.

### 1.4 Extraction de requête durcie

- Garder la convention code-block pour le streaming, mais valider l'extrait : pour les dialectes SQL, le candidat doit parser via la même chaîne que `sql_safety` (sqlparser) ; sinon ne pas le proposer comme « generated_query » (la réponse texte reste affichée).
- Si plusieurs blocs, préférer le dernier bloc valide (le modèle corrige souvent en fin de réponse).
- L'extraction par tool use arrive en Phase 3 ; ceci est le correctif intermédiaire.

Vérification : tests sur les cas connus de faux positifs (bloc d'explication, bloc JSON non-Mongo, multi-blocs).

### 1.5 Échantillon de données opt-in

- Toggle dans les settings IA (« Inclure un échantillon de lignes », défaut OFF) stocké en localStorage côté front, passé dans `AiRequest`.
- Backend : si activé, 3 lignes max par table mentionnée, colonnes sensibles (regex de redaction existante) remplacées par `<redacted>`, valeurs tronquées à 80 chars.
- Documenter dans la section transparence (`doc/todo/ENTERPRISE_READINESS.md` : case « Transparency » à compléter).

Vérification : test unitaire de redaction sur l'échantillon ; le toggle OFF n'envoie rien (assert sur le prompt construit).

---

## Phase 2 — Modèles, settings, robustesse

### 2.1 Modèles à jour + listing dynamique

- Rafraîchir les listes hardcodées de `types.rs` (vérifier chaque ID sur la doc provider au moment de l'implémentation ; côté Anthropic : `claude-fable-5`, `claude-opus-4-8`, `claude-sonnet-4-6`, `claude-haiku-4-5-20251001`). Supprimer les IDs preview datés.
- Nouvelle commande `ai_list_models(provider)` : interroge l'endpoint modèles du provider (`/v1/models` OpenAI-compat, `/api/tags` Ollama, équivalents Anthropic/Gemini/Mistral), cache en mémoire (TTL session), fallback silencieux sur la liste curée si réseau/clé KO. La liste curée devient le fallback, plus la source de vérité.

Vérification : test avec serveur mock (wiremock) pour le parsing par provider ; manuel : Ollama local liste les modèles réellement installés.

### 2.2 Exposer modèle et base_url dans l'UI

- `AiSection.tsx` : par provider, dropdown du modèle (alimenté par `ai_list_models`) + champ `base_url` (placeholder = défaut du provider). Persistance localStorage via `AiPreferencesProvider` (même mécanique que `qoredb_ai_provider`).
- Dans le panneau IA : afficher provider + modèle actifs dans le header (petit badge cliquable → settings).

Vérification : changer de modèle se reflète dans le corps de la requête (log debug) ; base_url custom pointe un Ollama distant.

### 2.3 Erreurs typées + retries

- `types.rs` : `AiError { InvalidKey, RateLimited { retry_after }, ContextTooLarge, Network, Provider(String) }`, sérialisé vers le front avec un `kind`.
- Providers : mapper les statuts HTTP (401/403 → InvalidKey, 429 → RateLimited, 5xx/transport → Network) au lieu de `format!("HTTP {}", …)`.
- Wrapper retry autour de l'envoi initial (pas du stream entamé) : 2 tentatives supplémentaires sur 429/5xx/réseau, backoff 1 s puis 3 s. Pas de retry une fois le stream commencé.
- Front : messages localisés par `kind` (« Clé invalide — vérifier les réglages » avec lien settings, « Limite atteinte, réessayez », …) + bouton « Réessayer » sur le dernier message en erreur.
- Parser `usage` dans le dernier chunk SSE (tous les providers l'exposent) → `tokens_used` enfin renseigné, affiché discrètement sous chaque réponse.

Vérification : tests wiremock 401/429/500/timeout ; manuel : clé invalide affiche le bon message et le lien settings.

### 2.4 i18n

Toutes les nouvelles strings dans les 9 locales (`en, fr, es, de, pt-BR, zh-CN, ja, ko, ru`), namespace `ai.*`.

---

## Phase 3 — Agent à outils (read-only)

Le différenciateur : le modèle explore la base lui-même et vérifie sa requête avant de la proposer. C'est ce qui justifie le tarif Pro face à « je colle mon schéma dans ChatGPT ».

### 3.1 Outils exposés au modèle

Quatre outils, tous read-only, exécutés via la session existante :

| Outil | Contrat | Garde-fous |
| --- | --- | --- |
| `list_tables` | namespace → noms | déjà borné par le driver |
| `describe_table` | table → colonnes/FK/index | redaction PII existante |
| `sample_rows` | table → 5 lignes max | redaction + troncature, refusé si toggle 1.5 OFF |
| `validate_query` | requête → parse + `EXPLAIN` (dry-run) | jamais d'exécution réelle ; réutilise `sql_safety` |

### 3.2 Boucle agentique backend

- Étendre le trait `AIProvider` avec le function calling natif : format OpenAI tools (couvre OpenAI/Mistral/DeepSeek/Ollama), tool use Anthropic, functionDeclarations Gemini.
- Boucle dans `commands/ai.rs` : stream → le modèle demande un outil → exécuter → renvoyer le résultat → continuer. Plafonds : 6 itérations max, budget global de tokens, timeout total inchangé (120 s).
- La requête finale passe par `validate_generated_query` comme aujourd'hui.
- Émettre des chunks d'activité (`AiStreamChunk.tool_activity`) pour l'UI.

### 3.3 UI

- Dans le fil : étapes d'outil repliées (« Inspection de `orders`… », « Vérification de la requête… ») avec détail au clic.
- Réutiliser le streaming existant ; pas de nouveau canal.

Vérification : tests d'intégration avec provider mock qui scénarise 2 appels d'outils ; manuel sur la base docker-compose : « combien de commandes par client le mois dernier ? » → l'agent inspecte les tables, propose une requête validée qui s'exécute du premier coup. C'est le critère d'acceptation de toute la phase.

Risque principal : divergence des formats tool-call entre providers. Mitigation : commencer par OpenAI-compat (4 providers d'un coup) + Anthropic, Gemini ensuite ; en attendant, les providers non migrés gardent le mode texte de la Phase 1.

---

## Phase 4 — Surfaces d'intégration

Chaque point est livrable indépendamment, dans l'ordre qu'on veut.

1. **Cmd+K dans l'éditeur** : sélection (ou requête entière) + instruction → réécriture proposée en diff (réutiliser les composants Diff existants, eux aussi Premium) → appliquer/annuler. Raccourci enregistré dans `useKeyboardShortcuts`.
2. **Schema browser** : « Expliquer cette table » (describe + LLM) dans le menu contextuel et brancher enfin `ai_summarize_schema` au niveau base (« Résumer ce schéma »). Sortie dans le panneau IA.
3. **Erreurs** : « Corriger avec l'IA » sur les toasts d'erreur de requête (aujourd'hui seulement dans QueryPanelResults).
4. **Résultats** : l'explication s'affiche dans le panneau IA (fil de conversation) au lieu de l'overlay détaché de DataGrid — supprime l'état `aiExplanation` local.

Vérification : manuel par surface ; lint + tailles de fichiers (aucun composant > 500 lignes, splitter au besoin).

---

## Phase 5 — Backlog (non planifié)

- Optimisation de requête : `EXPLAIN ANALYZE` → suggestions d'index (synergie avec `interceptor/profiling.rs`).
- Cellule IA dans le Notebook (cf. `doc/todo/v3.md`).
- Mémoire des habitudes (requêtes fréquentes, relations virtuelles apprises).
- Ghost-text completion dans l'éditeur (Ollama local pour la latence).

---

## Transverse (toutes phases)

- SPDX `BUSL-1.1` sur tout nouveau fichier IA.
- `doc/FEATURES.csv` : mettre à jour la ligne IA à chaque phase livrée.
- README (section AI assistant) à la fin de la Phase 3.
- Tests : chaque phase ajoute ses tests unitaires ; wiremock pour les providers à partir de la Phase 2.
- Aucune télémétrie ni appel réseau hors provider choisi (positionnement local-first inchangé).
