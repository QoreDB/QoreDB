## Reprise prioritaire — 10 octobre 2026, qualification native suspendue

Arrêt demandé par l'utilisateur pour la nuit : noter, commit/push, puis reprendre
plus tard. Ne pas considérer le point 3 comme terminé. Branche `feat/v0-1-40`,
PR https://github.com/QoreDB/QoreDB/pull/79 actualisée et vérifiée DRAFT.
CI du HEAD documentaire 8d163ee : quatre workflows démarrés/en cours
(Core37998046892, Pro37998046925, Backend37998046955, Lint37998046885). Aucun bump/tag/merge/
publication autorisé. Conserver ce fichier non suivi et ses sections historiques.

Code corrigé et poussé : `bea2a26d87ebd69eda59703fe8ac89f4b3a7bf61` (sur
`2af370c`, après isolation coffre `a57dafb`). Le commit documentaire `8d163ee` est poussé et
contient le rapport `doc/tests/V0_1_40_NATIVE_QUALIFICATION_2026-10-10.md`, index,
plan phase 6, guide TESTING et FEATURES. Relire git status/HEAD et la CI avant
reprise : d'autres changements peuvent intervenir.

Acquis réels (rapport public détaillé) :
- Reprise synthétique aux formats du tag v0.1.39 `252e0b2` ; profils isolés XDG,
  bus privé et vrai GNOME keyring, aucun accès aux données personnelles.
- 42 assertions Tauri réelles sur Core puis Pro compilé avec licence Core :
  connexions CRUD/credentials/coffre, workspace A/B/A, masques, SQLite exact,
  bibliothèques stockées, snapshot et erreurs ; redémarrage vérifié.
- UI native : notebook ouvrir/éditer/exécuter/sauver/réouvrir ; chmod500 fait
  échouer sauvegarde sans altérer l'ancien fichier, retry réussi après chmod700.
  Grille SQLite éditée réellement, annulation Escape vérifiée ; import/export
  projet via dialogues GTK après correctif. Préférences/historique conservés.
- PostgreSQL réel : 8 assertions dont annulation pg_sleep et bascule pendant
  requête. SQLite annulation NON supportée, ne pas la compter comme réussite.
- CLI/MCP debug réels : opt-in, SELECT masqué et entier exact, mutation refusée.
- Pas de licence payante disponible et aucun override utilisé. Bibliothèque UI
  avancée/Replay/Sandbox/TimeTravel/Data API/diff Pro positifs NON qualifiés.
- Corrections : import projet conservant métadonnées/drivers locaux/secrets
  supprimés ; AUR conservant qore/qore-mcp/ressources, checksum réel et dispatch
  version corrigé ; deux licences embarquées dans configuration sidecar.
- Vérifications : TS 397, typecheck, Biome ciblé, deux tests packaging réels de
  package() sur payload synthétique ; docs:check et diff check. Pas de nouvelle
  suite Rust unitaire complète (aucun changement Rust).

Build et prochain travail précis :
1. Cache `/home/raphael/.cache/qoredb-v0-1-40-target`. Build Cargo release générique
   réussi. Build distribution Tauri (x86-64-v3 selon src-tauri/.cargo) réussi,
   `.deb` créé puis AppImage ÉCHEC `failed to run linuxdeploy` (cause à trouver).
   Log persistant `.perf/qualification/evidence/bundle.log` (copie de
   `/tmp/qore-qualification-bundle.log`). Aucun build encore en cours à l'arrêt.
   Deb : cache/release/bundle/deb/QoreDB_0.1.39_amd64.deb, SHA-256
   ee98937b5c26072afeea31ef403fa7bb74e56da291e8ffc3d3e58a8501f030ba.
   Version volontairement inchangée. AppDir existe, aucun AppImage réussi.
2. Diagnostiquer linuxdeploy avec sortie détaillée ; inspecter le .deb et ses
   licences/ressources/3 binaires ; installer dans un root jetable (fakeroot,
   makepkg/pacman disponibles), jamais écraser l'installation personnelle.
   Puis démarrage réel et CLI/MCP RELEASE installés. Extraction != installation.
   Commande précédente (root repo, avec CARGO_TARGET_DIR ci-dessus) :
   pnpm tauri build --features pro --config src-tauri/tauri.sidecar.conf.json
   --config '{"build":{"beforeBuildCommand":""},"bundle":{"createUpdaterArtifacts":false}}'
   --bundles deb,appimage
   Sidecars release suffixés déjà copiés dans src-tauri/binaries (ignorés).
3. Consulter CI du SHA complet actuel (gh run list --commit ...). À l'arrêt
   code bea2a26 : Lint37997363047 et Backend37997363057 SUCCESS ;
   Core37997363031 et Pro37997363059 encore EN COURS. Initial2af échouait avant
   tests DockerHub (pull limit puis registry timeouts), ne pas confondre.
4. Compléter rapport/plan et RELEASE si nécessaire, reproduire chaque correctif,
   commit/push et actualiser PR draft. Verdict actuel : NON PRÊTE À PUBLIER.
   Pro légitime, plateformes Windows/macOS, matrice moteurs/transports,
   updater/signatures et budgets restent non établis.

Reproduction et preuves locales (ne pas effacer) :
- `.perf/qualification/legacy` profil manuel complet avec localStorage natif ;
  `.perf/qualification/repro` profil script incluant ancien snapshot.
- `qoredb-core-fixed`, `qoredb-pro-fixed` dans `.perf/qualification/`, frontend
  production embarqué ; debug nécessite LD_LIBRARY_PATH=cache/debug/deps.
- Scripts suivis prepare-native-qualification.py, launch-native-qualification.sh,
  native-webview-eval.mjs, qualify-native-profile.mjs ; guide TESTING explique.
  Ne pas régénérer dans un répertoire existant. Relancer via launcher suivi,
  port9333 (autre port via QOREDB_QUALIFICATION_INSPECTOR). Une seule connexion
  inspecteur à la fois. Release n'expose pas de target inspectable.
- `.perf/qualification/evidence/` contient JSON, hashes, captures des seules
  fenêtres synthétiques. UI helpers locaux dialog.py / xkeys.py vérifient PID.
  Ctrl+L puis Ctrl+A obligatoire dans dialogue GTK avant chemin ; Return parfois
  deux fois. Save notebook est un menu Radix à ouvrir via pointerdown, puis item.
- `.perf/qualification/headless.py` pour CLI/MCP ; dépend du bus privé courant
  de legacy/runtime/bus ; activer explicitement opt-in connexion synthétique.
- Aucun processus app/Vite de cette recette trouvé au dernier contrôle.
  Conteneur personnellement créé qoredb-qualification-v0140 arrêté/supprimé
  après pg_dump dans evidence/postgres-resume.sql. Pour recréer : postgres:16-alpine
  --pull=never, bind127.0.0.1:15440:5432, user/db qualification, mot de passe
  SYNTHÉTIQUE qualification-only ; restaurer dump si besoin. Autres conteneurs
  non touchés. Les comptes/mots de passe de fixture ne sont pas de production.

---

# Passation — v0.1.40 de QoreDB

> Mise à jour le 2026-10-09 dans `/home/raphael/dev/perso/QoreDB`.
> Note locale temporaire, à ne pas commiter. Brief destiné à l'agent qui reprend le travail.

## Complément de reprise — 9 octobre 2026, cloisonnement Replay

Arbitrage reçu : l'utilisateur a choisi le cloisonnement complet des enregistrements.
Il faut revenir dans le workspace d'origine pour voir ou gérer l'enregistrement,
qui continue en arrière-plan. La question ci-dessous est désormais résolue.

Deux lots : contrôles workspace/UUID sous verrou côté recorder et IPC, témoin/hook
protégés des réponses tardives ; nettoyage des captures avec erreurs propagées,
état conservé/cohérent pour retry (suppression multiple potentiellement partielle).
Aucun commit, staging, tag, bump, publication ni changement de droits.

Preuves : 90 tests Replay Pro (4 nouveaux), 268 Core ; 15 nouveaux scénarios
Chromium plus les 17 existants, transport/licence simulés. Typecheck, Biome ciblé,
rustfmt édition 2024, docs:check et diff passent. Pas de base réelle ni Tauri natif
réexécutés pendant ce lot. Rapport public :
`doc/tests/V0_1_40_REPLAY_WORKSPACES_2026-10-09.md` (indexé).

Budgets : huit passent face à `.perf/v0-1-40-2026-10-08-before.json`, environnement
identique. Comparaison immédiate de ce lot : +700 octets initiaux / +157 gzip ;
échec des deux budgets JS initiaux à zéro croissance, consigné sans le masquer.
Artefacts : `.perf/v0-1-40-2026-10-09-workspace-{before,after,release}.json`.
Les contrôles du témoin restent synchrones : extraction différée testée puis
écartée car elle augmentait la fragmentation gzip. Aucun nouveau composant livré.

Prochaine action précise : recette Tauri de deux workspaces synthétiques :
enregistrer A, basculer B, refuser lecture/contrôles de A, revenir A, sauvegarder
et rouvrir ; inclure une réponse moteur terminant pendant la bascule. Vérifier
séparément le suivi d'un rejeu déjà lancé. Les autres familles restent ouvertes.

## Complément de reprise — 9 octobre 2026, références Replay et sauvegarde

Toujours `feat/v0-1-40`, HEAD `79a3664`. Aucun staging, commit, tag, bump ni
publication ; tous les lots précédents et fichiers non suivis sont conservés.

Trois lots supplémentaires réalisés :
1. Acceptation Replay : génération de captures immuable puis publication atomique
   du jeu. Sélection partielle/skipped conservés, absence de capture respectée,
   corruption/absence attendue et erreurs disque propagées. Anciennes références
   préservées pour les rapports historiques. Rétention de la génération active
   et des références citées, purge des générations inutiles. Capture/jeu liés au
   même snapshot de workspace ; acceptation refusée pendant un rejeu.
2. Enregistrement : état et captures conservés après échec de sauvegarde, retry
   possible sans altérer les requêtes originales. Baseline publiée avant le jeu.
   Démarrages concurrents sérialisés ; création atomique refusant un fichier déjà
   créé (helper partagé PendingOutput::commit_new, lien dur, sans écrasement).
3. Qualification PostgreSQL 16.15 réelle : douze scénarios passent, dont trois
   nouveaux cycles rejeu/acceptation/réouverture/rejeu avec capture complète,
   metadata-only et budget épuisé. Grands entiers exacts et historique vérifiés.

Preuves exécutées : 86 tests unitaires Replay Pro (15 nouveaux), 268 tests Core
(lot 1), 6 tests qore-service paths (lot 2), 12 tests PostgreSQL replay_e2e avec
QOREDB_TEST_POSTGRES_REQUIRED=true (lot 3), typecheck et Biome src/lib/replay.ts,
docs:check, git diff --check, rustfmt ciblé. Chromium et budgets frontend non
réexécutés : seules des interfaces TS ont changé, aucun code frontend émis.
Rapport : `doc/tests/V0_1_40_REPLAY_2026-10-09.md`, indexé.
Nouveau fichier BUSL : `src-tauri/src/replay/reference_tests.rs`.

Les nouveaux champs `ReplaySet.baseline_run_id` et RunMeta.reference_generation
sont facultatifs/defaultés pour lire les anciens fichiers. Aucun changement du
numéro de version. Une référence locale absente sur une machine qui reçoit le
jeu n'est pas remplacée par une ancienne capture ; les attentes restent utilisables.
Ne pas présenter cette compatibilité testée sur JSON synthétique comme une
migration native d'un profil v0.1.39. Les baselines déjà écrasées avant correction
ne sont pas récupérables automatiquement.

L'instance dédiée `qoredb-v0140-reference-20261009` (localhost:54321, tmpfs,
postgres:16-alpine / serveur 16.15) a été arrêtée et supprimée après qualification.
Aucun autre conteneur touché. Cache Cargo et LIBRARY_PATH identiques ci-dessous.

Arbitrage historique résolu et lot réalisé : voir le complément de cloisonnement ci-dessus.

## Complément de reprise — 9 octobre 2026, exports

Branche `feat/v0-1-40`, HEAD `79a3664`, inchangés. Tous les changements
antérieurs sont préservés. Aucun staging, commit, tag, bump ou publication.
La v0.1.40 reste une stabilisation des fonctions existantes, sans ajout de
fonction Pro ni arbitrage des droits ER Core/Pro.

Trois lots terminés pendant cette continuation :
1. Fidélité XLSX/Parquet : entiers XLSX de plus de 15 chiffres en texte ;
   décimaux/non-signés Parquet en UTF-8 ; types reconnus explicitement ;
   conversions destructrices refusées sans valeur de cellule dans l'erreur.
2. Publication des exports : temporaire voisin privé 0600 puis rename après
   réussite du driver et du writer ; fichier précédent préservé sur échec ou
   annulation ; Done ne masque plus l'erreur finale du driver ; limite zéro
   respectée ; taille finale réelle. Helper PendingOutput dans qore-service.
3. Qualification SQLite réelle : six formats, avec/sans masque, erreur SQL et
   résultats vides. Nouveau défaut reproduit/corrigé : flux vide sans colonnes
   produisant un Parquet illisible annoncé Completed. SQLite décrit désormais
   le schéma du flux vide ; Parquet refuse une finalisation sans métadonnées.

Preuves de cette continuation (ne pas additionner les suites Core/Pro communes) :
- 24 tests `qoredb --features pro --lib export::` passent, dont trois tests avec
  une vraie base SQLite temporaire ; les autres utilisent writers/driver simulé.
- 12 tests `qoredb --lib export::` passent (lot 2, avant correction SQLite).
- 7 tests `qore-drivers --lib drivers::sqlite::tests` passent après correction.
- 5 tests `qore-service --lib paths::tests` passent, dont 3 nouveaux.
- docs:check, diff check et rustfmt ciblé passent.
- Aucun test TS/Chromium ni mesure frontend réexécuté : aucune source frontend,
  version, dépendance ou lockfile modifiés pendant cette continuation.

Rapport public : `doc/tests/V0_1_40_EXPORTS_2026-10-09.md` (indexé).
Nouveau fichier BUSL à conserver : `src-tauri/src/export/pipeline_pro_tests.rs`.
Les autres nouveaux fichiers et toutes les modifications des précédents lots
restent présents. Le cache Cargo et son workaround LIBRARY_PATH restent ceux
ci-dessous ; aucune base externe ni serveur Vite démarré pendant ces trois lots.

Limites : pas de recette Tauri/Excel/LibreOffice, pas d'exports des autres moteurs,
pas de disque plein réel ni remplacement Windows/macOS. Le masquage réel testé
part d'une SessionMasking synthétique ; coffre/licence/workspace/préflight restent
à qualifier via IPC. Le décimal SQLite est explicitement stocké en TEXT : ne pas
présenter ce test comme une validation de NUMERIC/DECIMAL SQLite. Dates non testées.
Un crash peut laisser un temporaire privé ; aucune garantie fsync ajoutée.

Prochaine action précise : poursuivre le cycle d'acceptation Replay déjà signalé
le 8 octobre. `CaptureStore::adopt_entry` retourne false si la nouvelle capture
est absente, en conservant l'ancienne ; `replay_accept_run` ignore les erreurs.
Aucun correctif de ce point n'a été ajouté aujourd'hui. Reproduire l'acceptation
metadata-only/budget puis traiter ensemble captures, attentes, erreurs disque et
cohérence multi-entrées, sans annoncer un succès partiel. Pour les exports,
prochaine qualification : portée sélection/résultat complet et frontières IPC.

## Complément de reprise — 8 octobre 2026, état après continuation

Branche `feat/v0-1-40`, HEAD `79a3664`, inchangés. Aucun commit, tag, bump,
publication ou staging. Tous les lots précédents sont préservés et non commités.
La reprise a ajouté les quatre lots notebooks/persistance/Data API/schema diff,
puis trois lots Replay et un lot de qualification PostgreSQL.

Derniers changements Replay :
- Annulation conservée dans un rapport persistant, requêtes restantes skipped,
  sans promesse d'interruption immédiate ni de rollback d'une requête déjà envoyée.
- Contrôle de session/workspace partagé dans SessionManager, réutilisé pour
  recording/run/A-B ; les deux cibles A/B sont vérifiées avant la première requête.
- Chronologie normale/A-B respectée à la réouverture ; corruption remontée.
  Rétention après écriture du rapport, conservation d'au moins un rejeu et des
  références nécessaires aux diffs ; purge refusée si référence illisible.
- Réponses tardives liées à sélection/connexion/workspace ; doublons de départ
  bloqués ; options Replay remontées sur changement de contexte dans AppLayout.

Preuves réellement exécutées pendant cette continuation :
- 9 tests PostgreSQL réels `replay_e2e`, dont 6 nouveaux, tous passent.
- 15 tests PostgreSQL réels `integration_databases postgres_`, dont 10 Time Travel,
  passent ; QOREDB_TEST_POSTGRES_REQUIRED=true dans les deux commandes.
- 71 tests unitaires Replay Pro, 3 tests SessionManager, 259 tests Core passent.
- 17 scénarios Chromium Replay (vrais hook/composant, IPC/notifications simulés).
- 336 tests TS / 39 fichiers ; typecheck ; Biome ciblé sans erreur, 2 warnings
  préexistants AppLayout ; rustfmt des fichiers Rust touchés ; docs:check ; diff check.
- 8 budgets frontend passent vs référence figée du début de la reprise :
  `.perf/v0-1-40-2026-10-08-before.json`. Candidate finale
  `.perf/v0-1-40-2026-10-08-replay-final.json`, Node 24.21.0/pnpm 11.3.0/Vite 8.1.0.
  JS initial −8 463 octets (−1 107 gzip), total +16 118 (+7 033 gzip), CSS inchangé.

Rapports publics : `doc/tests/V0_1_40_REPLAY_2026-10-08.md`, et les rapports
NOTEBOOKS, DATA_API et SCHEMA_DIFF du même jour. Les preuves de ces quatre lots
précédents (30 scénarios notebooks, 6 schema diff, 7 groupes Pro, 50 tests API)
ne sont pas présentées comme réexécutées ici.

Environnement : cache Cargo `/home/raphael/.cache/qoredb-v0-1-40-target` ; utiliser
`--target-dir /home/raphael/.cache/qoredb-v0-1-40-target -j 4` et
`LIBRARY_PATH=/home/raphael/.cache/qoredb-v0-1-40-target/debug/deps` (workaround DuckDB).
Playwright : `.perf/browser-tools/node_modules/playwright/index.mjs` ; Chromium :
`/home/raphael/.cache/ms-playwright/chromium-1243/chrome-linux64/chrome`.
Les manifests/lockfiles pnpm et versions ne sont pas modifiés.
Le PostgreSQL 16.15 de qualification est un conteneur dédié tmpfs nommé
`qoredb-v0140-replay-20261008`, port localhost 54321, créé par cette continuation.
Le conteneur a été arrêté et supprimé automatiquement (--rm) après les tests ;
son tmpfs a été libéré. Le serveur Vite de cette continuation est arrêté.

Prochaine action précise : reproduire l'adoption d'une référence Replay à partir
d'un rejeu sans capture (metadata-only/budget), puis vérifier que les anciennes
lignes de baseline ne restent pas proposées comme nouvelle référence. Examiner
ensuite les lectures/mutations de l'enregistreur global pendant un changement de
workspace, dont la sémantique actuelle permet d'arrêter/sauver dans son origine.
Autres familles peu couvertes : fédération, contrats/générateur, XLSX/Parquet, IA.
Restent ouverts : natif Tauri, autres moteurs, migration complète d'un profil
synthétique v0.1.39, performances natives, AUR et arbitrage commercial ER Core/Pro.
Aucune phase globale clôturée, aucun pourcentage de qualification recalculé.
Les anciens brouillons/historiques en clair ne sont pas assainis globalement.

## Objectif

Avancer autant que possible sur la v0.1.40, une tâche à la fois, avec des changements complets et vérifiés. Le périmètre est celui de `doc/todo/V0_1_40.md` : stabilisation des familles Pro existantes, précision et sûreté des données, édition sans perdre la position, clavier, légèreté, compatibilité et packaging. La reprise doit poursuivre l'implémentation et la qualification, pas seulement produire un audit ou un plan.

## État actuel

- Vérifié le 8 octobre : branche `feat/v0-1-40`, HEAD `79a3664`, également HEAD de `origin/feat/v0-1-40`. Ne pas présumer que cet état restera identique : commencer par `git status --short --branch` et le diff.
- Les commits précédents sont `b151d20` (mutations et Time Travel), `4e1859c` (plan), `ee33034` (paramètres Data API) et `28f4dc6` (paquet AUR).
- Les quatre lots du 6 octobre restent locaux, avec fichiers modifiés ET fichiers non suivis. Préserver tout cet état ; un checkout du seul HEAD ne contient pas ces lots.
- Estimation communiquée à l'utilisateur : environ 45 % du périmètre complet, marge de ±10 points. C'est une estimation qualitative incluant la qualification, pas un décompte mesuré. Aucune phase complète n'est déclarée validée dans le plan.
- Aucun nouveau test n'a été exécuté pour préparer cette passation. Les résultats ci-dessous sont ceux du 6 octobre ; les rapports datés ne prouvent pas que tout passe après des changements ultérieurs.

### Travail déjà réalisé

- Édition en masse : grands entiers et décimaux longs préservés dans le frontend ; génération SQL couverte, premiers cas de stockage SQLite réel.
- Sandbox : capture avant commit, résultats après rollback, contrôles de mutation et traitement des transactions interrompues corrigés.
- Time Travel : isolation connexion/workspace, valeurs relues depuis les drivers, clés composites et clés générées SQLite/PostgreSQL, captures incomplètes exclues des cibles de rollback, masquage, pagination au-delà du cache, compteurs, exports, suppressions et rétention applicative.
- Édition en place : mutation unique puis relecture canonique ; conservation des pages, du défilement, de la sélection et du focus dans les cas éligibles ; rechargement dans les cas où la cohérence ne peut pas être garantie.
- Variables/bibliothèque/notebooks : substitution depuis la source originale uniquement, chiffres numériques conservés, références intercellules combinées sans rescannage des valeurs.
- Visual Diff : doublons conservés, objets JSON comparés sans dépendre de l'ordre des propriétés, avertissements de comparaison incomplète dans l'UI et les exports, CSV protégé pour les retours chariot.
- Chargement Visual Diff : résultats tardifs invalidés, actualisation utilisant les nouveaux résultats, connexions partagées et libérées, invalidation sur changement de workspace, reprise explicite après erreur.
- Clavier : édition des cellules et dialogue de variables, Enter/Tab/Shift+Tab/Escape, focus restauré et actions de bibliothèque visibles au focus.
- Légèreté : mesure reproductible du bundle et chargement différé de plusieurs panneaux/dialogues. Traductions et documentation accompagnent les lots.

### Résultats documentés du 6 octobre

- `pnpm test:ts` : 295 tests réussis dans 37 fichiers, dont 37 nouveaux tests.
- `pnpm typecheck` : réussi.
- `node scripts/test-diff-sources-ui.mjs` : 13 scénarios Chromium, IPC simulé.
- `node scripts/test-pro-workflows-ui.mjs` : 7 groupes de contrôles Chromium, licence et stockage simulés.
- Biome ciblé : aucune erreur, 12 avertissements préexistants dans les composants Diff.
- `pnpm docs:check` et `git diff --check` : réussis.
- Huit budgets frontend respectés face au HEAD `79a3664` avant ces changements : JS initial -719 octets bruts/-16 gzip ; total +9 751 bruts/+3 772 gzip, sous le seuil de +2 %.
- Ce lot frontend n'a exécuté ni Rust, ni bases réelles, ni recette Tauri. Les preuves SQLite/PostgreSQL et Core/Pro des lots antérieurs sont dans leurs propres rapports.

### Travail encore ouvert

- Matrice Pro complète : licence/découverte, masquage sur tous les transports, Data API, règles/audit/profilage/alertes, Replay, schema diff, fédération, contrats/générateur, exports XLSX/Parquet, IA locale/distante, notebooks, index, relations virtuelles et ER.
- Notebooks : résultats intercellules périmés et cycle sauvegarde/import/export/réouverture encore à qualifier.
- Confidentialité Time Travel : les lectures appliquent les protections actuelles, mais l'assainissement persistant des anciens fichiers en clair et de leurs sauvegardes n'est pas terminé. L'origine des anciennes captures ambiguës n'est pas attribuée automatiquement.
- Moteurs hors des preuves SQLite/PostgreSQL, WebView/IPC natifs Tauri, plateformes et raccourcis associés.
- Migration depuis un profil synthétique v0.1.39, référence native de démarrage/RAM/CPU/latence, poids distribué/installé et cycles de fermeture/réouverture.
- AUR : cohérence PKGBUILD/.SRCINFO/checksums/licences/sidecars, installation/mise à jour/démarrage réels. Le paquet existant ne vaut pas qualification.
- Incohérence ER Core frontend/Pro backend : décision produit explicite requise avant de modifier les droits.
- Workflow de publication : la publication automatique et la procédure documentaire doivent être réconciliées avant toute release.

## Fichiers importants

Les chemins suivants sont relatifs à `/home/raphael/dev/perso/QoreDB`.

| Fichier | Rôle |
| --- | --- |
| `AGENTS.md`, `src/AGENTS.md`, `src-tauri/AGENTS.md`, `doc/AGENTS.md` | Instructions générales et propres aux surfaces ; lire celles des répertoires touchés. |
| `doc/todo/V0_1_40.md` | Périmètre, phases, matrice Pro, budgets et critères de sortie. |
| `doc/tests/V0_1_40_PRO_WORKFLOWS_2026-10-06.md` | Preuves et limites des quatre lots locaux. |
| `doc/tests/V0_1_40_*.md` | Preuves antérieures, commandes et couverture réelle par lot. |
| `doc/development/TESTING.md` | Matrice des commandes et configuration des tests navigateur. |
| `doc/development/ARCHITECTURE.md`, `DRIVERS.md`, `COMMANDS.md`, `LICENSING.md`, `AGENT_WORKFLOW.md` | Guides à lire selon la frontière modifiée, sous `doc/development/`. |
| `src/lib/notebook/notebookVariables.ts` et `.test.ts` | Variables typées et précision ; changements locaux. |
| `src/lib/notebook/notebookInterCellRef.ts` et `.test.ts` | Références intercellules et résolution combinée ; module Premium. |
| `src/hooks/useNotebook.ts` | Exécution utilisant la résolution combinée. |
| `src/components/Query/QueryLibraryModal.tsx` | Dialogue différé et actions accessibles au clavier. |
| `src/components/Query/QueryVariablesPrompt.tsx` | Formulaire, nombres exacts et restauration du focus. |
| `src/lib/diffUtils.ts` et `.test.ts` | Appariement, fidélité, avertissements et exports Visual Diff ; Premium. |
| `src/components/Diff/hooks/useDiffSources.ts` | Générations, lectures, promesses de connexion et nettoyage. |
| `src/components/Diff/DataDiffViewer.tsx`, `DiffConfigPanel.tsx`, `DiffResultsGrid.tsx`, `DiffSourcePanel.tsx` | Métadonnées de troncature, avertissements, états vides et reprise. Les trois derniers noms sont dans le même répertoire que le premier. |
| `src/lib/tabs.ts` | Métadonnée de troncature de source. |
| `src/locales/{en,fr,de,es,pt-BR,ru,ja,ko,zh-CN}.json` | Traductions des nouveaux messages. |
| `scripts/fixtures/pro-workflows/{index.html,app.jsx,hook.html,hook.jsx}` | Fixtures des composants et du hook réels, avec transport simulé. |
| `scripts/test-pro-workflows-ui.mjs`, `scripts/test-diff-sources-ui.mjs` | Tests Chromium des quatre lots locaux. |
| `doc/FEATURES.csv`, `doc/README.md` | Inventaire fonctionnel et index documentaire, modifiés localement. |
| `src/lib/query/tableRowUpdate.ts`, `src/hooks/useInfiniteTableData.ts`, `src/components/Grid/hooks/useInlineEdit.ts` | Édition en place et relecture ; déjà dans HEAD. |
| `src-tauri/crates/qore-service/src/mutation/`, `src-tauri/crates/qore-drivers/src/` | Mutations et lectures partagées ; déjà dans HEAD. |
| `src-tauri/src/time_travel/`, `src-tauri/src/commands/time_travel.rs` | Captures, persistance, rollback, confidentialité et rétention ; déjà dans HEAD. |
| `src-tauri/src/api/handlers.rs`, `aur/qoredb-bin/` | Correctif Data API et paquet binaire présents depuis avant les lots principaux. |

## Approches qui ont échoué, maintenant corrigées

- Remplacements successifs des placeholders : les valeurs insérées étaient rescannées ; elles pouvaient devenir d'autres variables ou références. La résolution actuelle parcourt les tokens de la source originale.
- Conversion des nombres par `Number()` : perte de chiffres. Conserver le contrat de précision existant ; ne pas réintroduire une conversion en double sur les valeurs exactes.
- Une seule ligne par clé dans Visual Diff : perte de doublons. Le nouvel appariement conserve les occurrences ; son déterminisme ne prouve pas une identité de ligne, d'où l'avertissement.
- Rafraîchissement reposant sur un ancien état et connexions enregistrées après leur résolution : vieux résultats réutilisés et sessions dupliquées. Le hook actuel attend les réponses fraîches et partage la promesse dès sa création.
- Montage immédiat du dialogue de variables : budget de JS initial dépassé. Import différé et montage à l'ouverture nécessaires ; la fixture vérifie ce comportement.
- Autofix Biome vers `Object.hasOwn()` : la bibliothèque TypeScript cible ne l'acceptait pas. L'appel `Object.prototype.hasOwnProperty.call` possède une suppression Biome ciblée et justifiée ; préserver la compatibilité ou vérifier explicitement toute autre solution.
- Comparaison de poids entre environnements différents : résultat incomparable, notamment avec changement de noyau. Ne pas remplacer la référence ou relever un budget pour rendre le contrôle vert.

## Direction de reprise

Le meilleur gain est de vérifier les parcours encore ouverts dans la matrice, reproduire les défauts puis terminer de petits lots. Les tests généraux ne remplacent pas les preuves natives, les tests de transport ni les lectures des fichiers exportés. Les scénarios couverts par mocks doivent rester identifiés comme tels.

## Prochaines actions

1. Lire les instructions applicables, vérifier Git et rapprocher ce brief du plan et du rapport du 6 octobre. Examiner les changements locaux et leurs tests sans réécrire les lots déjà terminés.
2. Choisir un seul lot borné encore ouvert. Priorité aux défauts P0/P1 reproduits de confidentialité, perte de données, mauvaise cible ou faux succès ; ensuite aux autres familles Pro peu qualifiées. Un bon point d'entrée est le cycle des références intercellules périmées, ou la matrice Data API si ses tests et environnement sont disponibles.
3. Définir les comportements attendus, ajouter une régression avant correction lorsque pratique, implémenter le plus petit changement complet et vérifier les surfaces réellement affectées.
4. Consigner les commandes, résultats, environnement et limites dans un rapport daté ; mettre à jour le plan, la documentation et `doc/FEATURES.csv` selon le résultat livré. Passer ensuite au lot suivant.
5. Préparer progressivement la qualification Tauri, moteurs, profils anciens et AUR. Si un environnement manque, le déclarer non exécuté et continuer sur un lot indépendant ; ne pas déclarer la phase terminée.

## Commandes et environnement

Vérifications à choisir selon les fichiers modifiés, pas à lancer aveuglément après chaque petite correction :

```bash
pnpm typecheck
pnpm test:ts src/lib/notebook/notebookVariables.test.ts src/lib/notebook/notebookInterCellRef.test.ts src/lib/diffUtils.test.ts
pnpm exec biome check <fichiers-modifiés>
pnpm docs:check
git diff --check
```

- Tests UI : démarrer `pnpm exec vite --host 127.0.0.1 --port 1430`, puis utiliser les scripts du rapport. Lire `TESTING.md` pour `QOREDB_UI_BASE_URL`, `QOREDB_PLAYWRIGHT_MODULE` et `QOREDB_CHROMIUM_EXECUTABLE`. Ne pas lancer le build de production pendant ces tests : les rechargements Vite réinitialisent la fixture.
- Chemins qui existaient le 6 octobre, à vérifier avant usage : module Playwright `/home/raphael/.cache/uv/archive-v0/q4tkOav9jtUBpQ85zi2NP/playwright/driver/package/index.mjs` ; Chromium `/home/raphael/.cache/ms-playwright/chromium-1243/chrome-linux64/chrome`. Aucune nouvelle dépendance n'a été ajoutée pour ces fixtures.
- Poids : référence locale `.perf/v0-1-40-2026-10-06-before.json`, candidate `.perf/v0-1-40-2026-10-06-final.json`. Vérifier leur présence et comparabilité. Ce n'est pas une référence de la release publiée v0.1.39.
- Pour un nouveau lot frontend, prendre sa mesure avant les modifications puis comparer après avec `pnpm perf:bundle --baseline <référence.json> --output <candidate.json>`.
- `pnpm typecheck` n'a pas d'effet de version ; `pnpm build` déclenche une synchronisation de versions via `prebuild`. Utiliser la procédure de mesure documentée.

## Contraintes

- Une tâche à la fois, qualité maximale, progrès autonome. Continuer tant que du travail pertinent est réalisable ; les seuls vrais arbitrages produit/sûreté/licence doivent être posés à l'utilisateur.
- Préserver les modifications existantes. Pas de reset destructif, de nettoyage global ou de remplacement de l'arbre par le seul HEAD.
- Utiliser pnpm, lire les AGENTS de chaque surface, conserver SPDX et séparation Apache/BUSL, traduire les chaînes dans toutes les locales et réutiliser les composants UI.
- Préserver les protections backend : lecture seule, sandbox, confirmation production, masquage et secrets. Ne pas utiliser de données de production pour les tests.
- Garder la logique métier partagée dans les crates workspace ; le dossier desktop `engine/` est une façade.
- Ne pas ajouter de nouvelle offre, de nouveau driver ou de refonte générale. L'ambiguïté ER nécessite un arbitrage explicite.
- La publication, les tags et le bump de version ne sont pas autorisés par cette demande de progression ; préparer les résultats reviewables avant une décision de release.
- `RTK.md` et l'outil `rtk` n'étaient pas disponibles lors du chantier. Le 8 octobre, aucun RTK.md trouvé à la racine du dépôt ni dans les parents testés jusqu'à `/home/raphael` ; ne pas inventer ses instructions.

## Mémoire

Les recherches claude-mem du 8 octobre sur `"v0.1.40"` et `"Time Travel"` n'ont pas fourni de référence récente utile à cette reprise. Utiliser Git et les rapports présents dans le dépôt ; aucune injection automatique de mémoire n'est présumée dans la session Astra.
