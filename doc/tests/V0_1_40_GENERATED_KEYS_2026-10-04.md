# v0.1.40 — Clés générées et chargement du formulaire, 4 octobre 2026

Référence : `b151d2063c5bf116afff04f59f50b5a3f9b2ffaf`, arbre local comprenant
les lots [édition en place](V0_1_40_INLINE_EDIT_2026-10-04.md) et
[fidélité des captures](V0_1_40_TIME_TRAVEL_CAPTURE_2026-10-04.md).
Ce complément traite les clés générées SQLite/PostgreSQL et le dépassement du
budget frontend du lot précédent. La recette de release reste ouverte.

Le [complément transactionnel suivant](V0_1_40_CAPTURE_TRANSACTIONS_2026-10-04.md)
qualifie les erreurs/annulations de relecture PostgreSQL et corrige les faux
succès de commit. Les chiffres ci-dessous restent ceux du lot clés générées.

## Changement

Le test d'insertion avec clé générée échouait avant correction : la clé restait
absente du journal alors que SQLite avait créé la ligne. Le contrat interne
`DataEngine::insert_row_returning` permet désormais de demander uniquement les
colonnes de clé primaire. SQLite et PostgreSQL les récupèrent dans l'INSERT,
sur la connexion transactionnelle lorsqu'elle existe. Les autres drivers gardent
leur insertion ordinaire sans inventer de valeur retournée. Aucun échec ne
provoque un second INSERT destiné à récupérer la clé.

`RowInsertResult` sépare ces valeurs internes de `QueryResult` et n'est pas
sérialisable. Les réponses IPC gardent leurs colonnes et lignes vides. Le service
vérifie la clé, puis relit l'image finale. Cette relecture reste nécessaire :
[SQLite RETURNING](https://www.sqlite.org/lang_returning.html) ne reflète pas les
modifications des triggers AFTER. Les chemins desktop et Sandbox utilisent le
même contrat ; le masquage avant stockage reste inchangé.

[PostgreSQL INSERT](https://www.postgresql.org/docs/16/sql-insert.html) impose des
droits SELECT supplémentaires avec RETURNING. Le driver renonce à la restitution
facultative si la lecture de toutes les colonnes n'est pas autorisée ou si les
politiques de sécurité des lignes s'appliquent au rôle. Les insertions générées
correspondantes restent confirmées, avec un historique incomplet. Les variantes
de protocole PostgreSQL n'activent pas automatiquement ce nouveau chemin.

Le formulaire `RowModal` est chargé à sa première ouverture, puis reste monté
pour conserver son cycle de traitement existant. Il ne fait plus partie du JS
initial. Les critères observables sont : absence de chargement avant ouverture,
insertion et modification utilisables, formulaire réinitialisé à la réouverture,
et absence de deuxième import ou de deuxième écriture involontaire.

## Vérification

Linux x86_64 ; Rust 1.94.0 ; PostgreSQL 16.15 dans un conteneur jetable lié
à `127.0.0.1:54321`, sans volume persistant ; SQLite réel via le driver SQLx.
Les tests PostgreSQL utilisent des noms de schéma/rôle uniques. Le test de
permissions requiert CREATE ROLE, disponible dans la fixture Docker.

| Scénario | Preuve | Résultat |
| --- | --- | --- |
| Clé générée et image après trigger | Régression SQLite échouant avant le changement | Réussi |
| DEFAULT VALUES, clé composite, nom contenant un guillemet, entier 9007199254740993 | SQLite et PostgreSQL réels | Réussi |
| Trigger qui déplace la clé ou ignore l'insertion ; violation de contrainte | SQLite ; aucune clé devinée, aucune écriture rejouée | Réussi |
| Driver sans valeurs retournées | Insertion ordinaire SQLite, identité générée laissée incomplète | Réussi |
| Lots avec clé générée, commit et rollback | SQLite et PostgreSQL ; images conservées uniquement pour les succès confirmés | Réussi |
| Rôle INSERT seul, SELECT limité à la clé, puis RLS sans politique SELECT | PostgreSQL ; insertion, capture incomplète et commit réussis | Réussi |
| Valeurs de retour internes | Résultat unitaire vide et captures exclues de la sérialisation du lot | Réussi |
| Formulaire différé, annulation, réouverture, INSERT et UPDATE | Composant TableBrowser réel dans Chromium, IPC simulé | Réussi |
| Édition en place et dialogues déjà différés | Script navigateur existant, dix scénarios | Réussi |
| IPC/WebView Tauri et autres moteurs | Hors des vérifications de ce lot | Non exécuté |

```bash
cargo test --manifest-path src-tauri/Cargo.toml -p qore-core --lib
cargo test --manifest-path src-tauri/Cargo.toml -p qore-drivers --no-default-features --features driver-sqlite --lib
cargo test --manifest-path src-tauri/Cargo.toml -p qore-service --no-default-features --features driver-sqlite --lib
cargo test --manifest-path src-tauri/Cargo.toml -p qoredb --lib
cargo test --manifest-path src-tauri/Cargo.toml -p qoredb --features pro --lib
QOREDB_TEST_POSTGRES_REQUIRED=true cargo test --manifest-path src-tauri/Cargo.toml -p qoredb --features pro --test integration_databases time_travel_capture -- --nocapture
pnpm test:ts src/components/Browser/rowModalUtils.test.ts src/lib/query/tableRowUpdate.test.ts
pnpm typecheck
pnpm exec biome check src/components/Browser/TableBrowser.tsx
pnpm docs:check
git diff --check
```

Les suites passent : 51 tests qore-core, 75 driver SQLite, 195 service SQLite,
229 desktop Core, 555 desktop Pro, 3 intégrations PostgreSQL et 33 TypeScript.
Le drapeau REQUIRED interdit le saut silencieux des tests PostgreSQL. Les
variables QOREDB_TEST_PG_HOST/PORT/USER/PASSWORD/DB ont été fixées explicitement
sur la fixture jetable lors de la vérification finale. Biome conserve un
avertissement préexistant de dépendance superflue dans l'effet de suivi des vues.
Le formatage Rust est contrôlé sur les fichiers modifiés.

Le contrôle navigateur utilise Playwright 1.62.1 et Chromium 153.0.8010.12 avec
les mêmes chemins de runtime que le lot précédent. Le script versionné
`node scripts/test-inline-edit-ui.mjs` passe. Le contrôle ponctuel du formulaire
monte TableBrowser, attend les lignes, ouvre « Insert », remplit puis annule,
rouvre et vérifie le reset, insère une ligne, puis ouvre « Open row » et modifie
un champ. Les requêtes de module et appels de mutation sont comptés ; aucune
erreur JavaScript n'est observée. La fixture et capture locales
`.perf/check-row-modal-ui.mjs` et `.perf/v0140-row-modal.png` sont ignorées et ne
constituent pas une nouvelle suite navigateur versionnée.

## Poids frontend

```bash
pnpm perf:bundle --output .perf/v0140-generated-before.json
pnpm perf:bundle --baseline .perf/v0140-generated-before.json --output .perf/v0140-generated-after.json
```

| Mesure en octets | Avant ce complément | Après | Écart |
| --- | ---: | ---: | ---: |
| JS initial brut | 2 450 781 | 2 439 897 | −10 884 |
| JS initial gzip | 709 751 | 708 359 | −1 392 |
| JS total brut | 4 787 170 | 4 790 860 | +3 690 |
| JS total gzip | 1 415 889 | 1 419 350 | +3 461 |
| CSS initial/total brut | 111 943 | 111 943 | 0 |
| CSS initial/total gzip | 17 491 | 17 491 | 0 |

Les huit budgets passent avec les métadonnées comparables : zéro croissance
initiale et moins de 2 % de croissance totale. Une seconde comparaison de la
candidate avec la référence conservée avant le lot de fidélité
(`.perf/v0140-capture-before.json`) passe aussi : −10 768 octets initiaux bruts
et −1 352 gzip. Le dépassement précédent est résorbé sans déplacer sa référence.
Ces mesures n'établissent ni le démarrage natif, ni le RSS, ni la taille installée.

## Limites restantes

Les clés non retournées par les autres drivers, les types de clé non pris en
charge et les identités déplacées après l'INSERT restent incomplets. Les lectures
avant/après ne forment pas un snapshot isolé ; les triggers différés, mutations
concurrentes et échecs de lecture dans une transaction restent à qualifier.
Les anciennes captures ne sont pas réparées et les anciens fichiers en clair
ne sont pas purgés. Isolation par workspace, historique au-delà des 5 000 entrées
du cache, matrice des autres moteurs, recette Tauri et performances natives
restent ouverts dans le [plan v0.1.40](../todo/V0_1_40.md).
