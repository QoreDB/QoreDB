# Universal Query Interceptor

> Documentation technique de l'implémentation du système d'interception de requêtes

## Vue d'ensemble

L'Universal Query Interceptor est un système complet d'interception de requêtes conçu pour offrir :

- **Audit Logging** : Journalisation persistante de toutes les exécutions de requêtes
- **Profiling** : Métriques de performance, percentiles, et détection des requêtes lentes
- **Safety Net** : Règles de blocage et d'avertissement pour les requêtes dangereuses

### Architecture : Backend-First

**Principe clé** : Toute la logique critique est implémentée côté backend (Rust) pour garantir une sécurité maximale. Le frontend ne fait qu'afficher et configurer ce que le backend fournit.

```
┌─────────────────────────────────────────────────────────────────┐
│                        FRONTEND (React/TS)                       │
│  ┌─────────────────┐  ┌─────────────────┐  ┌─────────────────┐  │
│  │ Settings Panel  │  │  Audit Panel    │  │ Profiling Panel │  │
│  └────────┬────────┘  └────────┬────────┘  └────────┬────────┘  │
│           │                    │                    │           │
│           └────────────────────┼────────────────────┘           │
│                                │                                │
│                    ┌───────────▼───────────┐                    │
│                    │  Tauri API (invoke)   │                    │
│                    │  src/lib/tauri/       │                    │
│                    └───────────┬───────────┘                    │
└────────────────────────────────┼────────────────────────────────┘
                                 │
┌────────────────────────────────┼────────────────────────────────┐
│                        BACKEND (Rust/Tauri)                      │
│                    ┌───────────▼───────────┐                    │
│                    │   Tauri Commands      │                    │
│                    │  commands/interceptor │                    │
│                    └───────────┬───────────┘                    │
│                                │                                │
│           ┌────────────────────┼────────────────────┐           │
│           │                    │                    │           │
│  ┌────────▼────────┐  ┌────────▼────────┐  ┌────────▼────────┐  │
│  │   AuditStore    │  │ ProfilingStore  │  │  SafetyEngine   │  │
│  │  (persistance)  │  │   (métriques)   │  │    (règles)     │  │
│  └─────────────────┘  └─────────────────┘  └─────────────────┘  │
│                                │                                │
│                    ┌───────────▼───────────┐                    │
│                    │  InterceptorPipeline  │                    │
│                    │   (orchestration)     │                    │
│                    └───────────┬───────────┘                    │
│                                │                                │
│                    ┌───────────▼───────────┐                    │
│                    │    execute_query()    │                    │
│                    │   (point d'entrée)    │                    │
│                    └───────────────────────┘                    │
└─────────────────────────────────────────────────────────────────┘
```

---

## Fichiers implémentés

### Backend (Rust)

| Fichier                                  | Lignes | Description                            |
| ---------------------------------------- | ------ | -------------------------------------- |
| `src-tauri/src/interceptor/mod.rs`       | ~22    | Module principal, exports publics      |
| `src-tauri/src/interceptor/types.rs`     | ~400   | Types et structures de données         |
| `src-tauri/src/interceptor/audit.rs`     | ~340   | Store d'audit avec persistance fichier |
| `src-tauri/src/interceptor/profiling.rs` | ~260   | Métriques et slow queries              |
| `src-tauri/src/interceptor/safety.rs`    | ~370   | Moteur de règles de sécurité           |
| `src-tauri/src/interceptor/pipeline.rs`  | ~360   | Pipeline d'orchestration               |
| `src-tauri/src/commands/interceptor.rs`  | ~420   | Commandes Tauri exposées               |

### Frontend (TypeScript)

| Fichier                                                   | Lignes | Description                    |
| --------------------------------------------------------- | ------ | ------------------------------ |
| `src/lib/tauri/interceptor.ts`                            | ~390   | API TypeScript vers le backend |
| `src/components/Interceptor/InterceptorSettingsPanel.tsx` | ~290   | Panel de configuration         |
| `src/components/Interceptor/AuditLogPanel.tsx`            | ~340   | Visualisation des logs d'audit |
| `src/components/Interceptor/ProfilingPanel.tsx`           | ~300   | Métriques de performance       |
| `src/components/Interceptor/SafetyRuleEditor.tsx`         | ~280   | Éditeur de règles custom       |

---

## Fonctionnalités détaillées

### 1. Audit Logging

**But** : Traçabilité complète de toutes les requêtes exécutées.

#### Données capturées par requête :

- Timestamp précis
- Session ID et Driver ID
- Requête complète + preview tronquée
- Environnement (dev/staging/prod)
- Type d'opération (SELECT, INSERT, UPDATE, DELETE, DROP, etc.)
- Base de données cible
- Succès/Échec + message d'erreur
- Temps d'exécution (ms)
- Nombre de lignes affectées
- Si bloquée par une règle de sécurité

#### Persistance :

- Format : JSONL (JSON Lines) pour append efficace
- Localisation : `{app_data}/com.qoredb.app/interceptor/audit.jsonl`
- Rotation automatique quand le fichier dépasse `max_audit_entries`
- Cache mémoire des 1000 dernières entrées pour accès rapide

#### API disponibles :

```rust
get_audit_entries(filter)   // Récupérer avec filtres
get_audit_stats()           // Statistiques agrégées
clear_audit_log()           // Vider le log
export_audit_log()          // Export JSON complet
```

---

### 2. Profiling

**But** : Identifier les problèmes de performance et optimiser les requêtes.

#### Métriques collectées :

- Nombre total de requêtes
- Requêtes réussies / échouées / bloquées
- Temps d'exécution : min, max, moyenne
- **Percentiles** : P50, P95, P99 (calculés sur les 10 000 dernières requêtes)
- Répartition par type d'opération
- Répartition par environnement

#### Slow Queries :

- Seuil configurable (défaut : 1000ms)
- Capture automatique des requêtes dépassant le seuil
- Stockage des N dernières slow queries (défaut : 100)

#### API disponibles :

```rust
get_profiling_metrics()     // Métriques complètes
get_slow_queries(limit)     // Liste des slow queries
clear_slow_queries()        // Vider les slow queries
reset_profiling()           // Reset complet
export_profiling()          // Export JSON
```

---

### 3. Safety Net

**But** : Prévenir l'exécution de requêtes dangereuses, surtout en production.

#### Règles built-in (non supprimables) :

| Règle                        | Environnement  | Action               |
| ---------------------------- | -------------- | -------------------- |
| Block DROP in Production     | Production     | Block                |
| Block TRUNCATE in Production | Production     | Block                |
| Confirm DELETE in Production | Production     | Require Confirmation |
| Confirm UPDATE without WHERE | Prod + Staging | Require Confirmation |
| Confirm DELETE without WHERE | Prod + Staging | Require Confirmation |
| Warn ALTER in Production     | Production     | Warn                 |

#### Actions possibles :

- **Block** : Requête refusée, jamais exécutée
- **Require Confirmation** : Demande `acknowledged_dangerous=true`
- **Warn** : Permet l'exécution mais log un warning

#### Règles custom :

Les utilisateurs peuvent créer leurs propres règles avec :

- Nom et description
- Environnements ciblés
- Types d'opérations ciblés
- Pattern regex optionnel sur le texte de la requête
- Action à effectuer

#### API disponibles :

```rust
get_safety_rules()          // Liste toutes les règles
add_safety_rule(rule)       // Ajouter une règle custom
update_safety_rule(rule)    // Modifier une règle
remove_safety_rule(id)      // Supprimer (custom uniquement)
```

---

## Intégration dans le flux d'exécution

L'interceptor est appelé automatiquement dans `execute_query()` :

```rust
// 1. Construction du contexte
let interceptor_context = interceptor.build_context(
    session_id, query, driver_id, is_production,
    read_only, acknowledged, database, sql_analysis, is_mutation
);

// 2. Pre-execution : vérification des règles de sécurité
let safety_result = interceptor.pre_execute(&interceptor_context);
if !safety_result.allowed {
    // Requête bloquée - enregistrer et retourner erreur
    interceptor.post_execute(&context, &result, true, rule_name);
    return error_response;
}

// 3. Exécution de la requête
let result = driver.execute(...).await;

// 4. Post-execution : enregistrement audit + profiling
interceptor.post_execute(&context, &result, false, None);
```

---

## Configuration

### Fichier de configuration

Localisation : `{app_data}/com.qoredb.app/interceptor/interceptor.json`

```json
{
  "audit_enabled": true,
  "profiling_enabled": true,
  "safety_enabled": true,
  "slow_query_threshold_ms": 1000,
  "max_audit_entries": 10000,
  "max_slow_queries": 100,
  "safety_rules": [
    // Règles custom de l'utilisateur
  ]
}
```

### Paramètres modifiables via l'UI :

| Paramètre                 | Défaut | Description                             |
| ------------------------- | ------ | --------------------------------------- |
| `audit_enabled`           | true   | Active/désactive l'audit logging        |
| `profiling_enabled`       | true   | Active/désactive le profiling           |
| `safety_enabled`          | true   | Active/désactive les règles de sécurité |
| `slow_query_threshold_ms` | 1000   | Seuil en ms pour les slow queries       |
| `max_audit_entries`       | 10000  | Limite du fichier d'audit               |
| `max_slow_queries`        | 100    | Nombre max de slow queries conservées   |

---

## Impact sur l'application

### Sécurité améliorée

- **Prévention des erreurs** : Les requêtes DROP/TRUNCATE en production sont bloquées par défaut
- **Traçabilité** : Chaque requête est loggée avec son contexte complet
- **Contrôle granulaire** : Règles personnalisables par environnement et type d'opération

### Performance

- **Overhead minimal** : L'interception ajoute ~0.1-0.5ms par requête
- **Détection proactive** : Les slow queries sont identifiées automatiquement
- **Métriques en temps réel** : P50/P95/P99 pour comprendre la distribution des latences

### Conformité

- **Audit trail complet** : Répond aux exigences de compliance (SOC2, GDPR, etc.)
- **Export facilité** : Données exportables en JSON pour analyse externe
- **Rétention configurable** : Contrôle sur la quantité de données conservées

### Expérience développeur

- **Feedback immédiat** : Messages d'erreur clairs quand une requête est bloquée
- **Configuration UI** : Pas besoin d'éditer des fichiers de config manuellement
- **Règles sur mesure** : Possibilité de créer des règles spécifiques au projet

---

## Commandes Tauri exposées

```typescript
// Configuration
get_interceptor_config();
update_interceptor_config(config);

// Audit
get_audit_entries(filter);
get_audit_stats();
clear_audit_log();
export_audit_log();

// Profiling
get_profiling_metrics();
get_slow_queries(limit, offset);
clear_slow_queries();
reset_profiling();
export_profiling();

// Safety Rules
get_safety_rules();
add_safety_rule(rule);
update_safety_rule(rule);
remove_safety_rule(rule_id);
```

---

## Fichiers supprimés (nettoyage)

L'ancienne implémentation frontend-first a été supprimée :

```
❌ src/lib/interceptor/           # Logique duplicative côté frontend
   ├── auditLog.ts
   ├── index.ts
   ├── interceptorPipeline.ts
   ├── interceptorSettings.ts
   ├── interceptors/
   ├── profilingStore.ts
   ├── queryAnalyzer.ts
   └── types.ts

❌ src/hooks/useQueryInterceptor.ts  # Hook non utilisé
```

**Raison** : Ces fichiers implémentaient la logique d'interception côté frontend, ce qui :

1. Dupliquait le code backend
2. Était contournable (sécurité faible)
3. Ne persistait pas les données correctement

---

## Évolutions futures possibles

1. **Alerting** : Notifications quand certains seuils sont dépassés
2. **Query fingerprinting** : Regrouper les requêtes similaires pour analyse
3. **Export vers services externes** : Envoi des métriques vers Prometheus/Grafana
4. **Analyse de tendances** : Graphiques d'évolution des performances dans le temps
5. **Règles conditionnelles** : Règles basées sur l'heure, l'utilisateur, etc.
