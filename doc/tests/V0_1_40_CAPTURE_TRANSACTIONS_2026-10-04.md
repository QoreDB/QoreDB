# v0.1.40 — Captures et transactions PostgreSQL, 4 octobre 2026

Référence : `b151d2063c5bf116afff04f59f50b5a3f9b2ffaf`, avec les lots locaux
[clés générées](V0_1_40_GENERATED_KEYS_2026-10-04.md), fidélité des captures et
édition en place conservés. Ce complément traite la sûreté des lectures
facultatives d'historique ; il ne clôture pas la qualification de la release.

## Défauts reproduits

Deux tests PostgreSQL réels échouaient avant correction :

- Un rôle ayant INSERT et SELECT sur la clé, mais pas SELECT sur toute la ligne,
  pouvait insérer une ligne, échouer à lire son image et recevoir un succès au
  commit alors que la table restait vide. La lecture avait interrompu la
  transaction. Le même parcours concerne les lectures avant UPDATE/DELETE.
- Après une erreur SQL dans une transaction, le driver annonçait un commit
  réussi. SQLx ne fournit pas le tag de commande permettant ici de distinguer
  le COMMIT effectif du ROLLBACK renvoyé par PostgreSQL.

L'injection d'une perte de connexion pendant la nouvelle lecture protégée a
reproduit un troisième défaut avant sa correction : après abandon de la
connexion transactionnelle, l'insertion suivante utilisait le pool en autocommit.
Ce comportement peut produire des écritures partielles dans un lot transactionnel.

## Correction

Le contrat interne `supports_safe_row_capture` impose un opt-in explicite.
SQLite et PostgreSQL sont activés. Sans qualification, le service ne demande ni
schéma ni image pour Time Travel et conserve un événement incomplet. La lecture
ordinaire de tables ne suffit pas à promettre une capture sans effet sur les
écritures. Cette restriction concerne aussi les variantes de protocole non
qualifiées ; elle ne retire pas leurs mutations ordinaires.

La [lecture PostgreSQL](../../src-tauri/crates/qore-drivers/src/drivers/pg_compat/capture_read.rs)
reste sur la connexion transactionnelle et utilise un savepoint privé. Elle
revient à ce savepoint puis le libère, y compris après succès, pour restaurer les
paramètres locaux et annuler les effets transactionnels éventuels de la lecture.
Hors transaction, elle utilise une transaction de lecture temporaire. PostgreSQL
documente la restauration par [ROLLBACK TO SAVEPOINT](https://www.postgresql.org/docs/16/sql-rollback-to.html).

Une tâche possède le verrou jusqu'à la fin du nettoyage, même si l'appelant
cesse d'attendre. La requête reçoit un délai maximal de 1 500 ms sans relever un
délai utilisateur plus strict. Le budget de lecture/nettoyage est de quatre
secondes ; la lecture du service reste facultative avec un délai de deux secondes.
Une écriture suivante peut donc attendre la fin du nettoyage. Les acquisitions
de verrou et connexion sont elles-mêmes bornées. Les tables soumises aux
politiques RLS du rôle restent sans image certifiée.

Si le nettoyage ne peut aboutir, la connexion est retirée et l'état de transaction
perdue bloque les commandes suivantes, y compris après attente du verrou.
Commit ou rollback renvoient alors un échec et terminent cet état ; une nouvelle
transaction explicite redevient possible. Aucun repli vers une écriture en
autocommit n'est effectué pour continuer le lot.

Le commit vérifie aussi que la transaction accepte encore une commande avant de
la confirmer. Une transaction interrompue est nettoyée et signalée en échec. Les
contrôles de lecture seule, confirmation de production, masquage et licence
restent dans les chemins existants. Aucun champ IPC n'est ajouté.

## Scénarios exécutés

PostgreSQL 16.15 réel, Linux x86_64, conteneur jetable sans volume persistant,
écoutant uniquement sur `127.0.0.1:54321`. Les rôles et schémas de test ont des
noms uniques ; aucun service ou jeu de données utilisateur n'est utilisé.

| Scénario | Attendu et observé | Statut |
| --- | --- | --- |
| INSERT explicite, UPDATE et DELETE avec lecture partielle autorisée | Historique incomplet ; écritures persistées après commit | Réussi |
| Erreur SQL ordinaire avant commit | Commit en échec ; nouvelle transaction utilisable | Réussi |
| Colonne absente pendant une lecture facultative | Échec de lecture isolé ; écriture précédente conservée | Réussi |
| Abandon de l'appel pendant `pg_sleep` | Nettoyage achevé avant la prochaine écriture ; deux lignes persistées | Réussi |
| Timeout utilisateur de 100 ms et savepoint existant | Délai respecté et restauré ; savepoint utilisateur encore utilisable | Réussi |
| Lots Sandbox avec compte INSERT et SELECT limité à la clé | Succès confirmés malgré les images indisponibles ; lot suivant en conflit annulé | Réussi |
| Connexion transactionnelle terminée pendant la capture | Écriture suivante refusée ; aucun autocommit ; reprise dans une nouvelle transaction | Réussi |
| Clés générées, triggers, grands entiers, commit/rollback et RLS du lot précédent | Régressions conservées | Réussi |
| Cycle PostgreSQL ordinaire existant | Test `postgres_e2e` | Réussi |
| IPC/WebView Tauri, autres moteurs et pannes réseau silencieuses | Non exécutés dans ce complément | Non exécuté |

Le test de coupure attend que la requête de sa propre connexion soit dans
`PgSleep`, puis termine uniquement ce backend via une connexion de fixture.
Le test d'annulation distingue l'abandon de l'appelant du nettoyage interne.
Les tests de rôles et de terminaison requièrent les droits administratifs de la
fixture Docker ; ils ne conviennent pas à une base partagée.

## Commandes et résultats

```bash
QOREDB_TEST_POSTGRES_REQUIRED=true cargo test --manifest-path src-tauri/Cargo.toml -p qoredb --features pro --test integration_databases time_travel_capture -- --nocapture
QOREDB_TEST_POSTGRES_REQUIRED=true cargo test --manifest-path src-tauri/Cargo.toml -p qoredb --features pro --test integration_databases postgres_e2e -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml -p qore-core --lib
cargo test --manifest-path src-tauri/Cargo.toml -p qore-drivers --no-default-features --features driver-sqlite --lib
cargo test --manifest-path src-tauri/Cargo.toml -p qore-drivers --no-default-features --features driver-postgres --lib
cargo test --manifest-path src-tauri/Cargo.toml -p qore-service --no-default-features --features driver-sqlite --lib
cargo test --manifest-path src-tauri/Cargo.toml -p qoredb --lib
cargo test --manifest-path src-tauri/Cargo.toml -p qoredb --features pro --lib
pnpm docs:check
git diff --check
```

Les variables `QOREDB_TEST_PG_HOST/PORT/USER/PASSWORD/DB` sont fixées sur la
fixture jetable lors de l'exécution. REQUIRED transforme une connexion impossible
en échec, sans saut silencieux. La suite de captures contient dix tests, dont
sept ajoutés dans ce complément. Le cycle PostgreSQL existant contient un test.
Les suites passent également : 51 tests qore-core, 75 driver SQLite, 77 driver
PostgreSQL, 195 service SQLite, 229 desktop Core et 555 desktop Pro. Aucun test
n'est ignoré dans ces sélections. Formatage Rust ciblé, documentation et contrôle
du diff passent.

Le compilateur a signalé des artefacts incrémentaux corrompus puis les a ignorés.
Aucun nettoyage global du répertoire Cargo n'a été effectué. Une exécution
intermédiaire a été interrompue pendant l'indisponibilité de la fixture ; elle
n'est pas comptée comme réussie.

## Limites restantes

L'isolation protège les erreurs et annulations des lectures d'historique. Elle
ne fournit pas un snapshot atomique entre lecture, écriture et relecture, ni un
ordonnancement général des commandes concurrentes. Les effets non
transactionnels de fonctions SQL et les triggers différés restent à qualifier.
Une perte réseau pendant le commit reste un résultat incertain, jamais un succès
reconstitué. La coupure de backend testée ne remplace pas un test de réseau muet.

Les autres drivers doivent qualifier la récupération après erreur/annulation
avant d'activer leurs images Time Travel. Les anciennes captures et fichiers en
clair ne sont pas réparés ou purgés. Isolation par workspace, historique au-delà
du cache de 5 000 entrées, recette Tauri et performances natives restent ouverts.

Ce complément ajoute des commandes SQL de protection et un contrôle avant commit.
Leur coût sur une base distante reste à mesurer. Les sources frontend ne changent
pas dans ce lot ; la mesure de bundle précédente n'a pas été relancée.
