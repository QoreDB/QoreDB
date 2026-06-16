# Plan d'intégration — Driver Elasticsearch / OpenSearch

> Spec d'implémentation. Driver mutualisé (un seul moteur paramétré par « flavor »),
> couvrant Elasticsearch et OpenSearch. Licence : **Core / Apache-2.0** (les drivers
> sont Core par défaut).

## 1. Décisions d'architecture (verrouillées)

| Sujet | Décision | Raison |
| ----- | -------- | ------ |
| Mutualisation | **1 module partagé `search_compat.rs` + 2 wrappers minces** (`elasticsearch.rs`, `opensearch.rs`) | Même pattern que `pg_compat.rs` + cockroachdb/neon/supabase. ~95 % de code commun, divergences gérées par un enum `SearchFlavor`. |
| Transport | **HTTP/REST via `reqwest`** (pas de SQLx) | ES/OS n'ont pas de protocole wire SQL. Client HTTP + pool de connexions interne. |
| Interface requête (primaire) | **Console « Dev Tools »** : `MÉTHODE /chemin` + corps JSON optionnel | Standard de facto (Kibana / OpenSearch Dev Tools). Couvre **toutes** les APIs (search, aggs, index, bulk, `_cat`, `_cluster`…). Vision long terme, pas POC. |
| Interface requête (complément, phase 2) | **Mode SQL** via endpoint `_sql` (ES) / `_plugins/_sql` (OpenSearch) | Confort pour les profils SQL. Réutilise `SQLEditor`. Ajouté après le cœur natif. |
| Auth | None / Basic / API Key / Bearer + TLS (CA custom, skip-verify dev) + Cloud ID (Elastic Cloud) | Couvre auto-hébergé, Elastic Cloud, OpenSearch managé. Le secret transite par le champ `password` (déjà chiffré au vault). |
| Modèle de données | `dataModel: 'search'` (nouveau), `isDocumentBased`-like → éditeur dédié `SearchEditor` | Ni tabulaire ni Mongo : besoin d'un éditeur console propre. |

### Pourquoi la console plutôt qu'un éditeur JSON « à la Mongo »

La requête ES n'est pas qu'un corps JSON : elle cible un index + une méthode HTTP + un
endpoint. Le format Dev Tools (`GET /index/_search` + corps) est le seul qui exprime
**toute** la surface de l'API sans bricolage. C'est ce que connaissent les utilisateurs ES,
et ça évite d'inventer une convention maison.

## 2. Contrat de `execute()`

Entrée = texte de la console :

```
POST /my-index/_search
{
  "query": { "match": { "title": "rust" } },
  "aggs":  { "by_year": { "terms": { "field": "year" } } }
}
```

Parsing :
- **1re ligne** : `MÉTHODE CHEMIN` (`GET|POST|PUT|DELETE|HEAD` + path, `/` initial optionnel).
- **Reste** : corps JSON (ou NDJSON pour `_bulk`). Vide autorisé.
- Plusieurs blocs séparés par une ligne vide = plusieurs requêtes (comme Dev Tools) — **phase 2** ; v1 = une requête par exécution.

### Mapping réponse → `QueryResult` (détection par forme)

| Forme de réponse | Colonnes produites |
| ---------------- | ------------------ |
| `hits.hits[]` (search) | `_id` (text), `_index` (text), `_score` (float), `_source` (json) |
| `aggregations` présent | 1 ligne, colonne `aggregations` (json) — en plus des hits si les deux sont là |
| `_cat/*` (array d'objets) | colonnes dérivées des clés du 1er objet |
| `_bulk` / index / update / delete | `affected_rows` renseigné + colonne `result` (json) |
| Générique (cluster, mapping brut…) | 1 colonne `response` (json) |

`execution_time_ms` = mesuré côté client ; on expose aussi `took` ES dans la réponse json.

## 3. Mapping du trait `DataEngine`

| Méthode | Comportement ES/OS |
| ------- | ------------------ |
| `driver_id` | `"elasticsearch"` / `"opensearch"` |
| `test_connection` | `GET /` (version + cluster name) ; vérifie flavor cohérent (champ `version.distribution` = `opensearch` pour OS). |
| `connect` | Crée la `SearchSession` (client reqwest + base URL + auth) ; ping `GET /_cluster/health`. |
| `list_namespaces` | **1 namespace logique** (`database` = nom du cluster, `schema = None`). |
| `list_collections` | `GET /_cat/indices?format=json` → indices (filtre `.système` par défaut, togglable) + alias (`GET /_cat/aliases`). |
| `describe_table(index)` | `GET /index/_mapping` → aplatissement des champs en `TableColumn` (nested/object/multi-field gérés) ; `GET /index/_count` → `row_count_estimate`. |
| `preview_table(index, n)` | `POST /index/_search {"size": n, "query": {"match_all": {}}}`. |
| `execute` / `execute_in_namespace` | cf. §2. `namespace` ignoré (pas de notion de DB). |
| `execute_stream` | **phase 2** : `search_after` + PIT (point-in-time) pour la pagination large. v1 : non supporté. |
| `capabilities` | `supports_transactions=false`, `supports_mutations=true`, `supports_streaming=false` (v1), `cancel_support=BestEffort` (phase 2 via `X-Opaque-Id` + `_tasks/_cancel`). |
| routines/triggers/events/sequences | tous `false` / `NotSupported`. |

## 4. Auth & connexion

Champ ajouté à `ConnectionConfig` (cf. `clickhouse_cluster`, `mssql_auth` — même style flat) :

```rust
#[serde(default)]
pub search_auth_mode: Option<String>, // "none" | "basic" | "api_key" | "bearer"
```

- **basic** : `username` + `password` (vault) → header `Authorization: Basic …`.
- **api_key** : secret dans `password` (vault) → header `Authorization: ApiKey …`.
- **bearer** : secret dans `password` (vault) → header `Authorization: Bearer …`.
- **none** : aucun header.
- **TLS** : `ssl` → `https://`. `ssl_mode = "insecure"` → skip-verify (dev, avec warning UI). CA custom : réutiliser le mécanisme existant si présent, sinon phase 2.
- **Cloud ID** (Elastic Cloud) : si fourni (réutilise `host`), décodage base64 → endpoint réel. Détection DSN possible (`*.es.*.cloud.es.io`, `*.aws.elastic-cloud.com`).

Le secret passe **toujours** par `password` (déjà `skip_serializing` + chiffré au vault) — aucun nouveau chemin de secret à sécuriser.

## 5. Fichiers — Backend (Rust)

| Fichier | Action |
| ------- | ------ |
| `src-tauri/crates/qore-drivers/src/drivers/search_compat.rs` | **Créer** : `SearchFlavor` enum, `SearchSession` (client reqwest, base_url, headers auth), `SessionMap`, helpers (`request`, `list_indices`, `get_mapping`, `count`, `map_response`, `build_base_url`). |
| `src-tauri/crates/qore-drivers/src/drivers/elasticsearch.rs` | **Créer** : wrapper mince `impl DataEngine`, `driver_id="elasticsearch"`, flavor ES. |
| `src-tauri/crates/qore-drivers/src/drivers/opensearch.rs` | **Créer** : idem, `driver_id="opensearch"`, flavor OS. |
| `src-tauri/crates/qore-drivers/src/drivers/mod.rs` | Ajouter `pub mod search_compat; pub mod elasticsearch; pub mod opensearch;`. |
| `src-tauri/crates/qore-service/src/context.rs` | `registry.register(Arc::new(ElasticsearchDriver::new())); … OpenSearchDriver::new()`. |
| `src-tauri/crates/qore-core/src/types.rs` | Ajouter `search_auth_mode` à `ConnectionConfig`. |
| `src-tauri/crates/qore-drivers/Cargo.toml` | Ajouter `reqwest` (features `json`, `rustls-tls`) si absent. |

SPDX `Apache-2.0` en tête de chaque nouveau `.rs`.

## 6. Fichiers — Frontend (TypeScript/React)

| Fichier | Action |
| ------- | ------ |
| `src/lib/connection/drivers.ts` | `Driver.Elasticsearch`/`Driver.OpenSearch` dans l'enum + entrées `DRIVERS` complètes (label, icon, defaultPort 9200, terminologie « index »/« document », `dataModel:'search'`, `supportsSQL:false`, `createAction:'none'`). |
| `src/lib/connection/driverCapabilities.ts` | Type `DataModel` += `'search'` ; nouveau dialect `'search'` dans `getQueryDialect` ; caps schema = aucune routine. |
| `src/lib/ddl/driverCapabilities.ts` | Mapper ES/OS sur `NO_DDL`. |
| `src/components/Editor/SearchEditor.tsx` | **Créer** : éditeur console (highlight `MÉTHODE /path` + corps JSON via `lang-json`, lint JSON, complétion endpoints/index). Inspiré de `MongoEditor.tsx`. |
| `src/components/Editor/search-constants.ts` | **Créer** : templates (`GET _search`, agg terms, `_bulk`, `_cat/indices`, créer index…). |
| `src/components/Query/QueryPanelEditor.tsx` | 3e branche : `dialect search` → `SearchEditor`. |
| `src/components/Connection/connection-modal/BasicSection.tsx` | Section auth ES/OS (sélecteur mode + champs conditionnels). Pas de username pour `api_key`/`bearer`/`none`. |
| `src/components/Connection/connection-modal/{types.ts,mappers.ts}` | Champ `searchAuthMode` + mapping vers `search_auth_mode` + validation. |
| `src/lib/connection/dsnDetector.ts` | Détection Elastic Cloud (`*.cloud.es.io`) → `Driver.Elasticsearch`. |
| `public/databases/elasticsearch.png`, `opensearch.png` | **Ajouter** les logos. |
| `src/locales/*.json` (9 langues) | Labels connexion, terminologie index/document, libellés mode auth, messages d'erreur. |
| `doc/todo/DATABASES.md` | Cocher ES/OS une fois livrés ; compléter la matrice DDL (ES/OS = `❌` DDL visuel). |
| `doc/FEATURES.csv` | Ajouter les deux moteurs. |

## 7. Tests & vérification

- `docker-compose.yml` : services `elasticsearch` (single-node, security basic) + `opensearch` (single-node). → **vérif : `docker-compose up` expose 9200/9201.**
- Tests d'intégration Rust (gated comme les autres drivers) dans `qore-drivers` :
  - connect + `test_connection` (détection flavor correcte ES vs OS) ;
  - `list_collections` retourne les indices créés ;
  - `describe_table` aplatit un mapping nested ;
  - `execute` search → mapping `hits` correct ;
  - `execute` agg → colonne `aggregations` ;
  - `execute` index/delete → `affected_rows`.
- Front : `pnpm lint:fix` + `pnpm format:write` propres.

## 8. Découpage en jalons (critères de succès vérifiables)

1. **Backend cœur** → vérif : depuis un test Rust, connexion à ES local + `GET /_search` renvoie un `QueryResult` avec les bons hits.
2. **Mutualisation OpenSearch** → vérif : même suite de tests passe contre OpenSearch via le wrapper, sans dupliquer la logique.
3. **Câblage front (catalogue + connexion)** → vérif : ES/OS apparaissent dans le `DriverPicker`, le formulaire d'auth se remplit, la connexion s'établit dans l'app.
4. **`SearchEditor` + exécution** → vérif : écrire `GET /index/_search` dans l'app et voir les résultats tabulés.
5. **Schéma & arbre** → vérif : l'arbre liste les indices, `describe_table`/preview fonctionnent au clic.
6. **i18n + logos + docs** → vérif : 9 langues OK, logos affichés, `DATABASES.md`/`FEATURES.csv` à jour.

### Phase 2 (après le cœur)

Fait :

- **Mode SQL** ✅ — toggle Console ⇄ SQL dans `SearchConsole`. Une requête qui ne commence pas par une méthode HTTP est envoyée à `_sql?format=json` (ES) / `_plugins/_sql` (OS). Réponses `columns`/`rows` (ES) et `schema`/`datarows` (OS) mappées en `QueryResult`.
- **Data streams** ✅ — listés via `GET /_data_stream` dans `list_collections`, à côté des indices et alias.
- **CA cert custom** ✅ — champ `ssl_ca_cert` (chemin PEM) sur la connexion, chargé via `reqwest::Certificate::from_pem` quand TLS est actif. UI dans `BasicSection` (visible pour ES/OS avec SSL).

Reste à faire :

- Streaming (`search_after`/PIT) pour la pagination large.
- Annulation (`_tasks/_cancel` via `X-Opaque-Id`).
- Multi-requêtes console (plusieurs blocs séparés par une ligne vide).
