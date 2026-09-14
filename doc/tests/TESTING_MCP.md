# Smoke test du serveur MCP (qore-mcp)

Procédure manuelle à dérouler avant une release qui touche `crates/qore-mcp`,
`qore_service::agent_tools` ou `qore_service::agent_access`.

## 1) Préparer

- Compiler : `cd src-tauri && cargo build --release -p qore-mcp`.
  Le binaire est dans `src-tauri/target/release/qore-mcp` (sans `duckdb-bundled`,
  exporter `LD_LIBRARY_PATH` vers `target/duckdb-download/*/*/` sous Linux).
- Vérifier la version : `qore-mcp --version` affiche la version du `package.json`.
- Dans l'application, créer deux connexions vers la même base de test (par
  exemple `docker compose up -d postgres`), puis dans Settings > Agents IA
  activer l'interrupteur de `agents-on` et laisser `agents-off` désactivée.
- Le serveur lit le vault par défaut (`~/.config/com.rapha.qoredb`, variable
  `QOREDB_CONFIG_DIR` pour un autre dossier), sauf s'il est lancé depuis un
  dossier contenant un workspace `.qoredb`, ou avec `--workspace <dir>` ou
  `QOREDB_WORKSPACE` : il lit alors les connexions de ce workspace, comme l'app.
  Les `instructions` renvoyées à `initialize` indiquent le magasin utilisé.
- Pour les requêtes enregistrées : un workspace `.qoredb` dont la Query Library
  contient une requête `SELECT * FROM users WHERE city = {{city}}` (variable
  `city` de type texte) et une requête `DELETE FROM users`.
- Pour la fédération : une licence Pro active dans l'app (le serveur la lit dans
  le trousseau ; sous macOS, autoriser l'accès au premier appel) et une seconde
  connexion exposée vers un autre moteur.

## 2) Handshake JSON-RPC

```bash
printf '%s\n' \
  '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"smoke","version":"0"}}}' \
  '{"jsonrpc":"2.0","method":"notifications/initialized"}' \
  '{"jsonrpc":"2.0","id":2,"method":"tools/list"}' \
  '{"jsonrpc":"2.0","id":3,"method":"prompts/list"}' \
  '{"jsonrpc":"2.0","id":4,"method":"resources/templates/list"}' \
  '{"jsonrpc":"2.0","id":5,"method":"tools/call","params":{"name":"list_connections","arguments":{}}}' \
  | qore-mcp 2>/dev/null
```

Attendu :

- `initialize` renvoie les capacités `tools`, `resources` et `prompts` et des
  `instructions` listant les onze outils.
- `tools/list` : `list_connections`, `list_namespaces`, `list_tables`,
  `describe_table`, `preview_table`, `search_schema`, `run_query`, `explain_query`,
  `run_federated_query`, `list_saved_queries`, `run_saved_query`.
- `prompts/list` : `audit_table`, `explain_slow_query`, `document_schema`.
- `resources/templates/list` : `qore://{connection_id}/{database}/{table}`.
- `list_connections` ne contient que `agents-on`, avec `read_only: true` et son
  `alias` (`agents_on`).

## 3) Gouvernance

Avec l'inspecteur (`npx @modelcontextprotocol/inspector qore-mcp`) ou Claude Code
(`claude mcp add qoredb -- /chemin/qore-mcp`) :

1. `run_query` sur l'id de `agents-off` : erreur « not exposed to AI agents »,
   même si l'id est connu.
2. `run_query` avec `INSERT`/`UPDATE` sur `agents-on` : refus « read-only mode »,
   y compris si la connexion est enregistrée sans lecture seule.
3. `run_query` avec `database` renseigné cible bien ce namespace
   (`SELECT current_database()` sous PostgreSQL).
4. `preview_table` avec `limit: 500` renvoie au plus 100 lignes.
5. `explain_query` renvoie un plan sous PostgreSQL et MongoDB refuse avec la
   liste des capacités du driver.
6. `search_schema` avec `pattern: "mail"` renvoie les colonnes `email` et ne
   renvoie aucune donnée.
7. `resources/list` liste une ressource par connexion exposée, sans se
   connecter ; `resources/read` sur `qore://<id>` renvoie les namespaces et
   tables avec leurs URIs ; sur une URI de table, le `describe_table` en JSON ;
   une URI `file://` est refusée.
8. Les appels apparaissent dans l'audit (Settings > Sécurité) avec la source
   `mcp`.
9. Laisser le serveur ouvert plus de dix minutes puis rappeler un outil : la
   session est rouverte sans erreur (fermeture des sessions inactives).
10. `list_saved_queries` lancé hors workspace renvoie une erreur explicite ; dans
    le workspace, il liste les requêtes avec leurs variables.
11. `run_saved_query` avec `variables: {"city": "Paris"}` renvoie les lignes ;
    une variable inconnue ou une valeur invalide pour un `number` est refusée
    avant exécution ; la requête `DELETE` est refusée « read-only mode ».
12. `run_federated_query` sans licence Pro renvoie un refus explicite. Avec la
    licence, une jointure `agents_on.<db>.<table>` × seconde connexion renvoie
    les lignes et la liste des sources ; un id non exposé dans `connection_ids`
    est refusé.
13. `run_federated_query` avec `... FROM agents_on.<db>.<table>,
    read_csv('/etc/hosts')` échoue : DuckDB n'a pas accès aux fichiers.

## 4) Écran Settings

- Settings > Agents IA affiche le chemin et la version du binaire quand il est
  à côté de l'exécutable ou dans le `PATH`, sinon l'aide d'installation.
- Le snippet Claude Desktop copié contient le chemin absolu détecté.
- Chaque connexion enregistrée apparaît avec son interrupteur ; le basculer
  met à jour `list_connections` au prochain appel, sans redémarrer le serveur.
- Le formulaire de connexion affiche le même interrupteur « Exposer aux agents
  IA », reflète l'état choisi dans Settings et l'enregistre avec la connexion.
