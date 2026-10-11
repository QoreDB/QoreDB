# v0.1.40 — Origine workspace de l'historique, 4 octobre 2026

Référence : `b151d2063c5bf116afff04f59f50b5a3f9b2ffaf`, avec les lots locaux
captures, transactions et [historique hors cache](V0_1_40_HISTORY_RETENTION_2026-10-04.md)
conservés. Ce complément traite l'isolation de Time Travel desktop et la mise
à jour du masquage des sessions ouvertes.

## Défauts et comportement attendu

Deux tests échouaient avant correction : deux workspaces portant une connexion
avec le même identifiant partageaient leurs événements ; une ancienne capture
sans origine workspace pouvait aussi être attribuée à une nouvelle session
par le seul identifiant de connexion.

La revue du chemin de masquage a trouvé le même rapprochement par identifiant
seul. Retirer une règle dans un workspace pouvait retirer celle des sessions
ouvertes dans l'autre. Un test vérifie maintenant que seules les sessions du
workspace et de la connexion concernés changent.

Le critère retenu est l'origine backend de la session : une capture commencée
dans A conserve A si le workspace actif devient B. Une session de A ne permet
pas de consulter Time Travel depuis B. Revenir dans A et rouvrir sa connexion
retrouve les événements attribués à A. Les noms d'affichage et les identifiants
copiés dans les fichiers de connexion ne suffisent pas à établir cette origine.

## Mise en œuvre

Le [gestionnaire de sessions](../../src-tauri/crates/qore-drivers/src/session_manager.rs)
conserve une identité workspace liée une seule fois, avant que les commandes
desktop rendent la session à l'interface. Les connexions sauvegardées utilisent
le contexte du magasin effectivement lu. Les connexions directes de debug
utilisent le workspace backend au début de l'ouverture.

Le [contexte de coffre](../../src-tauri/src/commands/vault.rs) résout ensemble
l'identité et le magasin depuis le même état du gestionnaire de workspaces.
Un changement ultérieur de workspace ne modifie pas cet objet. Les appels
frontend historiques qui transmettent `project_id: "default"` pour un workspace
sur fichiers restent compatibles ; ce champ ne fournit pas l'origine de
l'historique. L'identité est celle déjà utilisée par le backend pour ce chemin
de workspace et son service de trousseau.

Les captures individuelles et Sandbox enregistrent `workspace_id` avec les
autres métadonnées du journal. Les [lectures et suppressions](../../src-tauri/src/commands/time_travel.rs)
résolvent leur périmètre depuis la session et le workspace actif. Timeline,
compteurs, historiques de ligne, états, diffs, rollback et exports passent par
le même filtre, en mémoire comme sur disque.

La suppression de toutes les entrées par IPC concerne désormais le workspace
actif. Les entrées d'un autre workspace et les anciennes origines inconnues
restent dans le journal. Les jetons de confirmation des suppressions Time Travel
sont liés à l'action et au workspace : un jeton obtenu dans A ne peut pas
supprimer l'historique de B après un changement de workspace. Leur durée de vie
et leur usage unique restent inchangés.

Les mises à jour de masquage depuis le coffre filtrent aussi par workspace et
connexion. Les sessions d'origine inconnue ne sont pas requalifiées. Les
contrôles existants de licence, lecture seule, production et masquage restent
sur leurs chemins respectifs. Les requêtes IPC ne reçoivent pas de nouvelle
identité workspace fournie par le frontend ; le type de réponse des captures
accepte le nouveau champ nullable.

## Scénarios couverts

Dix tests ont été ajoutés, dont huit dans le desktop et deux dans le gestionnaire
de sessions. Les assertions de capture SQLite et Sandbox existantes vérifient
aussi la persistance de l'origine.

| Scénario | Résultat attendu et observé |
| --- | --- |
| Même connexion, table et clé dans deux workspaces après redémarrage du store | Une seule origine visible sur tous les chemins de lecture ; suppression ciblée isolée |
| Capture ancienne sans `workspace_id` | Pas d'attribution à une nouvelle session ; lecture limitée à sa session originale |
| Historique au-delà du cache, y compris connexions directes | Filtre workspace appliqué sur disque ; origine inconnue refusée pour une entrée attribuée |
| Suppression du workspace dans un journal de plus de 5 000 entrées | Autres workspaces, anciennes captures et lignes non interprétables conservés |
| Session déjà liée ou supprimée | Réaffectation à un autre workspace et liaison d'une session absente refusées |
| Deux sessions de A et une de B avec le même identifiant de connexion | Modification du masquage limitée aux deux sessions de A |
| Magasin résolu avant changement du workspace actif | Magasin et identité continuent à désigner A ; B reçoit une autre identité |
| Consultation depuis un autre workspace ou sans origine de session | Refus backend |
| Confirmation obtenue dans A puis utilisée dans B | Refus et consommation du jeton ; action et origine toutes deux requises |

Les fixtures utilisent des répertoires temporaires, des métadonnées synthétiques
et les bases SQLite de test existantes. Le test de contexte ne lit ni n'écrit
de secret dans le trousseau système.

## Vérifications

```bash
cargo test --manifest-path src-tauri/Cargo.toml -p qoredb --features pro --lib
cargo test --manifest-path src-tauri/Cargo.toml -p qoredb --lib
cargo test --manifest-path src-tauri/Cargo.toml -p qore-drivers --no-default-features --features driver-sqlite --lib
cargo test --manifest-path src-tauri/Cargo.toml -p qore-service --no-default-features --features driver-sqlite --lib
cargo check --manifest-path src-tauri/Cargo.toml -p qore-cli --no-default-features --features driver-sqlite
pnpm typecheck
pnpm test:ts src/lib/tauri/time-travel.test.ts
pnpm exec biome check src/lib/tauri/time-travel.ts
pnpm docs:check
git diff --check
```

Les suites passent avec 576 tests Pro, 249 Core, 77 drivers avec SQLite et
195 service avec SQLite, sans test ignoré. La compilation de la CLI avec SQLite,
les quatre tests du transport Time Travel, le typecheck et Biome passent.
Le formatage Rust ciblé, `docs:check` et le contrôle du diff passent également.
Les logs locaux sont dans `.perf/v0140-workspace-*.log`.

## Limites

L'isolation décrite concerne les opérations Time Travel et la propagation des
règles de masquage. Elle n'interdit pas toute requête SQL sur une ancienne
session encore ouverte après un changement de workspace. Les captures de ces
écritures gardent leur origine de session. Les captures et réglages restent dans
le stockage local de l'application ; il ne s'agit pas d'une frontière entre
utilisateurs ayant accès au même système de fichiers.

Les anciennes captures sans origine fiable restent sur disque, sans attribution
automatique par nom, connexion ou workspace actif. Après redémarrage, leur session
originale n'existe plus et elles ne sont donc pas exposées par une reconnexion.
Déplacer un workspace sur disque ne migre pas son identité ni son historique.

La configuration de Time Travel et les budgets du journal restent globaux.
La purge par durée n'est pas encore planifiée et la limite de taille du fichier
reste à appliquer. La recette Tauri native, les échanges réels avec le trousseau
et les changements de workspace pendant une requête réseau restent à qualifier.
Les tests unitaires ne constituent pas une validation IPC de bout en bout.
Le seul changement frontend est un champ de type TypeScript, sans code exécuté
supplémentaire ; la mesure de bundle n'a pas été relancée.
