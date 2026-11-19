# Plan v0.1.42 — Paramètres SQL, préflight de migrations, CI/CD et finition workflows

Statut : livré sur `feat/v0-1-42` (prêt pour release `v0.1.42`).

## Périmètre et répartition Core / Pro

- **Core (`Apache-2.0`)** :
  - **Lot 1** : Paramètres SQL à la volée (`$1`, `:param`, `@param`, `{{param}}`) dans l'éditeur SQL principal (`QueryPanel`) + déblocage UI et re-licence `Apache-2.0` des cellules Chart et des labels de références inter-cellules dans les Notebooks (audit B7).
  - **Lot 2** : Migration Blast Radius — analyse d'impact avant migration/DDL croisant l'AST SQL (`sqlparser`), le journal d'audit local (`audit.jsonl`), les actifs `.qoredb/` et un preflight de données en lecture seule sur la session active.
  - **Lot 4 (CLI)** : `qore contract list|run` et `qore replay list|run` dans `qore-cli` (formats JSON et JUnit XML, codes retour CI/CD) exécutables sans licence Pro bloquante sur le runner, tout en conservant les garde-fous `AgentVault`.
  - **Lot 5** : Sûreté backend et alignement documentaire — consentement plugins WASM en opt-in strict (audit D10), jetons serveur de confirmation production sur les commandes de requêtes et mutations (audit C10), corrections de commentaires (audit D1) et alignement de la documentation (audits B7, B8, B10, B11, D7, D9).
- **Pro (`BUSL-1.1`)** :
  - **Lot 3** : Finition des modules Pro existants — export de patch SQL (`INSERT`/`UPDATE`/`DELETE`) dans Visual Data Diff (audit B8), respect des longueurs/largeurs/unicité/`NOT NULL` dans Data Generator v2 (audit B10), édition d'endpoint sans régénération de jeton, validation backend `page_size` et timeout d'exécution dans Instant Data API v2 (audit D3).
  - **Lot 4 (Service)** : Extraction du cœur d'exécution `contracts` et `replay` vers `qore-service` en conservant `BUSL-1.1`.

## Critères d'acceptation par lot

1. **Lot 1 — Paramètres SQL à la volée et Notebooks**
   - Toute requête SQL contenant `$1`/`$name`, `:name`, `@name` ou `{{name}}` (hors chaînes, commentaires, casts `::type` et variables système `@@var`) affiche un badge compact dans `QueryPanelToolbar` et ouvre la modale `QueryParametersDialog` à l'exécution si un paramètre manque, avec mode `auto` par défaut (`auto`, `text`, `number`, `date`, `null`) et aperçu SQL en direct.
   - L'exécution substitue les paramètres en une passe sans modifier le texte source dans l'éditeur.
   - Les Notebooks proposent l'ajout de cellules `chart`, l'édition de `cell.config.label` dans les cellules SQL/Mongo et un éditeur de configuration dans `ChartCell`.

2. **Lot 2 — Migration Blast Radius**
   - `qore-sql::ddl_impact` extrait par AST les `DropTable`, `DropColumn`, `AlterColumnType`, `AddNotNull`, `AddUnique` et `ShrinkVarchar`.
   - `analyze_migration_blast_radius` signale les requêtes récentes de `audit.jsonl`, les actifs `.qoredb/` impactés (Library, Notebooks, Contracts, Replay Sets, Instant API) et exécute en option le preflight `SELECT COUNT(*)` en lecture seule.

3. **Lot 3 — Finition Workflows Pro**
   - Visual Data Diff génère un script SQL `INSERT`/`UPDATE`/`DELETE` et refuse explicitement en présence de colonnes masquées ou de clés ambiguës/absentes.
   - Data Generator v2 respecte `varchar(N)`/`char(N)`, `tinyint`/`smallint`, refuse une FK `NOT NULL` sans ligne parente et garantit l'unicité des PK et index `UNIQUE` mono-colonne.
   - Instant Data API v2 valide `page_size` et `MAX_ENDPOINTS` côté backend, substitue `NULL` pour les paramètres optionnels absents sans défaut, applique un timeout de 30 s avec annulation driver, et permet la mise à jour d'un endpoint sans réémettre le jeton.

4. **Lot 4 — `qore-cli` CI/CD**
   - `qore contract list`, `qore contract run`, `qore replay list` et `qore replay run` fonctionnent depuis la CLI en respectant l'opt-in `expose_to_agents`, le mode lecture seule et le masquage des colonnes.
   - Sortie JSON ou JUnit XML et code retour `2` en cas d'échec de contrat ou d'écart replay.

5. **Lot 5 — Sûreté et vérité produit**
   - `ConsentDialog` ne pré-coche que `log` et `storage` à l'installation initiale.
   - Les commandes de requêtes et mutations en production exigent un jeton serveur délivré par `issue_production_confirmation_token` lorsque `prod_require_confirmation` est actif.
   - Les constats D1, B7, B8, B10, B11, D7, D9 et D10 sont résolus dans le code et documentés dans [l'audit Claims vs Code](../audits/CLAIMS_VS_CODE.md).
