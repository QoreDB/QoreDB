# v0.1.40 — Qualification native Linux, 9–10 octobre 2026

## Référence et verdict

Candidate non prête à publier. La reprise du profil synthétique et les parcours
Linux ci-dessous sont établis ; ils ne qualifient pas les licences payantes,
les parcours Pro positifs ni les autres plateformes.

HEAD initial : `2af370cefc4c5b54ee13ba0cc362fca40c8d675c`, branche
`feat/v0-1-40`. Seul `HANDOFF.md` était non suivi ; son contenu initial est conservé, avec une
passation locale ajoutée à la demande de l’utilisateur.
Corrections et outillage de cette recette :
`bea2a26d87ebd69eda59703fe8ac89f4b3a7bf61`. Les binaires ont été compilés à partir
de ces sources, avant leur commit, puis les scénarios reproductibles rejoués.
Les versions restent `0.1.39` conformément à l'absence de bump : les noms des
artefacts ne suffisent donc pas à identifier cette candidate. Aucun tag, merge,
workflow de publication, déploiement ou signature de mise à jour n'a été lancé.
La PR #79 reste draft.

Les rapports [coffre](V0_1_40_VAULT_WORKSPACES_2026-10-09.md) et
[fichiers atomiques](V0_1_40_ATOMIC_FILES_2026-10-09.md) établissaient des tests
unitaires, un runtime IPC de test et des fixtures Chromium simulées. Ils
n'établissaient ni WebView native, ni trousseau système, ni installation.
Ce rapport complète également les preuves [notebooks](V0_1_40_NOTEBOOKS_2026-10-08.md),
[édition](V0_1_40_INLINE_EDIT_2026-10-04.md),
[profil/historique](V0_1_40_PROFILE_HISTORY_2026-10-09.md) et
[transferts](V0_1_40_FILE_OPERATIONS_2026-10-09.md), sans convertir leurs autres
scénarios en réussites natives.

## CI identifiée par SHA

Sur le HEAD initial `2af370c`, Frontend Lint a réussi. Core, Pro et Backend Tests
ont échoué au téléchargement Docker Hub, avant les tests : limite anonyme de
pulls, puis erreurs/timeouts d'authentification du registre lors de la seule
relance des jobs échoués. Aucune assertion n'a été désactivée.

- [Core initial](https://github.com/QoreDB/QoreDB/actions/runs/37990227163).
- [Pro initial](https://github.com/QoreDB/QoreDB/actions/runs/37990227149).
- [Backend initial](https://github.com/QoreDB/QoreDB/actions/runs/37990226999).
- [Lint initial](https://github.com/QoreDB/QoreDB/actions/runs/37990227068).

Sur `bea2a26`, le lint et Backend Tests ont réussi ; Core et Pro sont encore
en cours au point de contrôle du 10 octobre. Cela ne constitue pas une CI verte :
[Core](https://github.com/QoreDB/QoreDB/actions/runs/37997363031),
[Pro](https://github.com/QoreDB/QoreDB/actions/runs/37997363059),
[Backend](https://github.com/QoreDB/QoreDB/actions/runs/37997363057),
[lint](https://github.com/QoreDB/QoreDB/actions/runs/37997363047).
Les résultats des SHA antérieurs cités dans la PR restent historiques.

## Environnement et isolation

Arch Linux x86-64, noyau `7.2.8-arch1-2`, WebKitGTK `2.52.6`, Node `24.21.0`,
pnpm `11.3.0`. Fenêtres GTK/X11 sous la session graphique Linux disponible.
La WebView utilise `tauri://localhost` et le frontend de production embarqué.
Deux exécutables debug distincts sont construits : Core et `--features pro`.
Dans les deux cas, la licence effective est Core, sans clé activée. La commande
`dev_set_license_tier` n'a pas été utilisée.

Tous les fichiers sont sous `.perf/qualification/`. Chaque profil possède ses
propres XDG config/data/cache/runtime, son bus D-Bus privé et son trousseau GNOME
déverrouillé avec un mot de passe synthétique. Aucun profil QoreDB personnel,
secret personnel ou base existante n'est utilisé. Les mots de passe de fixture
ne sont pas des secrets de production. SQLite est un fichier jetable ; PostgreSQL
16 est le conteneur distinct `qoredb-qualification-v0140`, sur `127.0.0.1:15440`,
démarré depuis l'image locale avec `--pull=never`.

Le client [WebKit](../../scripts/native-webview-eval.mjs) évalue du JavaScript
dans cette WebView réelle et appelle son vrai `__TAURI_INTERNALS__.invoke`.
Il ne remplace aucun IPC, driver, licence, dialogue ou accès disque. Les parcours
UI utilisent les composants de l'application, les dialogues GTK et les écritures
Tauri. Les dialogues sont pilotés par événements clavier X11 sur une fenêtre dont
le PID est vérifié ; les captures sont limitées aux fenêtres de test.

Un premier lancement Cargo direct sur Vite affichait une page vide avec
`$RefreshReg$` absent. Il n'est pas compté comme démarrage réussi. Les recettes
suivantes embarquent `pnpm exec vite build` avec `tauri/custom-protocol`, sans
modifier la CSP ni ajouter un contournement au code produit.

## Profil v0.1.39 et comparaison

Le [générateur](../../scripts/prepare-native-qualification.py) refuse d'écraser
un répertoire existant. Les schémas sont vérifiés contre le tag `v0.1.39`, commit
`252e0b267b6d67585b6916bce2ce29f516685fd8` : connexions globales et individuelles,
identifiants FNV des workspaces, bibliothèque version 1, QNB version 1, snapshots,
ancien journal Time Travel sans `connection_id`/`workspace_id`, préférences et
historique localStorage. Des hashes SHA-256 précèdent les ouvertures.

Le paquet officiel `QoreDB_0.1.39_amd64.deb` a été téléchargé et extrait, puis son
exécutable démarré sur le premier profil isolé. Son SHA-256 est
`c7c48c1fda8f0fd1eee40d36f0976e176abd241e545d2e8c105262f53a20248c`, identique au
digest GitHub. Les fichiers synthétiques contrôlés restent byte-identiques après
ce démarrage. Ce lancement n'est pas une installation du paquet ni une recette
UI complète de l'ancienne version ; sa WebView release n'expose pas l'inspecteur.
Le second profil reproductible ajoute une capture persistée vérifiée séparément.

| Données | Résultat réellement observé |
| --- | --- |
| Connexions par défaut, alpha et beta | Identités, chemins, paramètres et masques restaurés ; secrets retrouvés dans les services keyring v0.1.39 ; aucune connexion d'alpha attribuée à beta |
| Workspaces | Manifestes conservés ; ouverture A/B/A et refus d'un chemin invalide ; timestamps/ordre des récents changent lors des ouvertures explicites |
| Bibliothèque | Dossiers, identifiants, variables, favoris et requêtes restaurés par IPC natif ; modification du titre dans alpha sauvegardée et retrouvée après bascule puis redémarrage ; beta inchangé |
| Notebooks | Ouverture par menu/dialogue GTK, édition SQL, exécution, sauvegarde puis réouverture après redémarrage ; variable `9007199254740993` conservée |
| Snapshot | Lecture native de la capture v0.1.39 avec entier exact, renommage, relecture identique des valeurs, puis relecture après passage du binaire Pro au binaire Core |
| Journal et réglages Time Travel | SHA-256 inchangés après ouvertures et redémarrages ; pas de nouvelle attribution des anciennes origines ; lecture fonctionnelle Pro refusée sous Core |
| Masquage | `email` reste `••••••` en aperçu natif, grille, CLI et MCP, même avec licence Core ; ajout de règles refusé sans modifier la politique conservée |
| Préférences/historique | Français, thème sombre, options de confidentialité, historique ancien et brouillon conservés dans la WebView après redémarrage |

Les différences attendues correspondent aux actions demandées par la recette :
label SQLite, titre de requête alpha, source/horodatage QNB, nom du snapshot,
connexions jetables et activation temporaire de l'exposition aux agents.
La réécriture de `connections.json` matérialise aussi les champs optionnels par
défaut ; les valeurs initialement définies sont conservées. Le journal ancien,
sa politique personnalisée sans rétention temporelle et les fichiers beta restent
inchangés. Ce profil synthétique n'établit pas la reprise de tous les profils
possibles, ni celle de données anciennes déjà corrompues.

## Parcours natifs exécutés

Le [script de qualification](../../scripts/qualify-native-profile.mjs) passe
42 assertions en Core et 42 dans le binaire compilé Pro avec licence Core.
Elles couvrent le stockage et le transport natifs ; elles ne signifient pas
42 parcours UI.

- Connexions : création, modification, duplication, suppression et conservation
  de l'original ; lecture des secrets ; verrouillage, mauvais mot de passe,
  déverrouillage et verrouillage rétabli au redémarrage.
- Workspaces : A/B/A, bibliothèque conservée, refus de liste/suppression/secret/
  sauvegarde avec l'identifiant d'un autre workspace ; erreur d'ouverture sans
  abandon du workspace courant.
- SQLite : modification via IPC, contrainte NOT NULL refusée, relecture exacte.
  Dans la vraie grille : édition d'une cellule, confirmation de succès et
  relecture directe SQLite ; Escape annule une autre édition sans mutation.
- PostgreSQL : huit assertions natives, dont `pg_sleep(20)` annulé effectivement,
  requête terminant sur sa session initiale pendant une bascule vers beta,
  absence de connexion réattribuée à beta, mutation et relecture d'un bigint exact.
- SQLite renvoie explicitement « does not support query cancellation » : aucune
  annulation réussie n'est revendiquée pour ce driver. Un essai long a dépassé le
  délai de l'outil ; le processus de test a ensuite été fermé.
- QNB : annulation du vrai dialogue sans ouverture ; ouverture/édition/exécution/
  sauvegarde/réouverture. Une interdiction réelle d'écriture du répertoire produit
  une erreur visible, conserve exactement l'ancien fichier et le brouillon ;
  le rétablissement des droits permet une sauvegarde réussie.
- Projet : export depuis les réglages, fichier JSON natif sans mots de passe,
  confirmation et import par dialogue GTK. Après correction, une connexion SQLite
  est ajoutée, avec lecture seule et `options` conservés, sans changer de workspace.
- Licence : Time Travel, enregistrement Replay et ajout de masquage refusés sous
  Core ; l'interface affiche l'offre Pro pour la bibliothèque avancée. Sa recette
  UI positive n'a donc pas été exécutée. Les écritures de bibliothèque ci-dessus
  concernent les commandes de stockage autorisées, pas une activation Pro.
- CLI et MCP : vrais processus, découverte après opt-in d'une connexion synthétique,
  requête SQLite masquée et exacte, refus d'UPDATE/DELETE. MCP est initialisé en
  JSON-RPC sur stdio ; ce n'est pas un serveur simulé.

Les changements de workspace pendant requête sont établis au niveau IPC.
La matrice complète des confirmations/UI pendant toutes les opérations longues
n'est pas rejouée nativement ; les fixtures Chromium historiques restent des
preuves distinctes.

## Défauts corrigés

L'import de projet utilisait une liste incomplète de drivers et imposait un port
et un utilisateur non vides même aux connexions locales. Il omettait les métadonnées
TLS, proxy, options et masquage. Deux régressions échouent avant correction.
L'import reconnaît désormais les identifiants du catalogue existant et conserve
les champs exportés, avec nouveaux ID/workspace et secrets supprimés. Les contrôles
Rust de forme, coffre et licence restent exécutés ; une licence refusant un masque
ne déclenche pas une seconde sauvegarde sans masque. L'atomicité globale d'un
import reste hors garantie : un import peut annoncer des connexions ignorées.

Le PKGBUILD AUR perdait `qore` et `qore-mcp` pourtant présents dans le `.deb`.
La régression exécute réellement `package()` sur un payload synthétique et échoue
sur `usr/bin/qore` avant correction. Les trois exécutables et les répertoires de
ressources sont conservés. Le bundle sidecar inclut les textes Apache-2.0/BUSL-1.1.
Le script AUR remplace `SKIP` par le hash du `.deb` et refuse une version invalide
ou un fichier manquant avant modification. La `.SRCINFO` correspond à
`makepkg --printsrcinfo`. Le dispatch sans version utilise la dernière release
publiée au lieu du nom de branche. Le workflow AUR n'a pas été déclenché.

## Commandes et contrôles

Les chemins de cache ne font pas partie du projet. Les commandes Cargo lancées
depuis la racine utilisent `CARGO_TARGET_DIR` et, pour le debug dynamique,
`LD_LIBRARY_PATH` vers son `debug/deps`. La compilation Tauri utilise aussi la
configuration `src-tauri/.cargo/config.toml`, notamment `x86-64-v3` ; son cache
n'est pas interchangeable avec le premier build Cargo générique.

```bash
pnpm exec vite build
cargo build --manifest-path src-tauri/Cargo.toml -p qoredb --features pro,tauri/custom-protocol
cargo build --manifest-path src-tauri/Cargo.toml -p qoredb --features tauri/custom-protocol
cargo build --manifest-path src-tauri/Cargo.toml -p qore-cli -p qore-mcp
cargo build --manifest-path src-tauri/Cargo.toml --release -p qoredb -p qore-cli -p qore-mcp --features pro,duckdb-bundled,tauri/custom-protocol -j 6
```

Les cinq compilations ci-dessus réussissent. Le premier frontend Vite contient
les sources initiales ; le build est rejoué après le correctif de transfert avant
les compilations embarquées finales. Aucun `pnpm build` avec pré-bump n'est requis.

```bash
pnpm test:ts
pnpm typecheck
pnpm exec biome check src/lib/share/projectTransfer.ts src/lib/share/projectTransfer.test.ts
node --test scripts/aur-package.test.mjs
```

Résultats : 397 tests TypeScript, 12 tests ciblés de transfert inclus ; typecheck
et Biome réussis ; deux tests packaging réussis. Aucun de ces résultats n'est une
recette native. Les suites Rust unitaires complètes n'ont pas été relancées dans
ce lot sans modification Rust ; les compilations et scénarios natifs ne sont pas
présentés comme un passage de ces suites.

## Artefacts et installation

Le build Tauri de distribution est lancé sans signature/update artifacts :

```bash
pnpm tauri build --features pro --config src-tauri/tauri.sidecar.conf.json \
  --config '{"build":{"beforeBuildCommand":""},"bundle":{"createUpdaterArtifacts":false}}' \
  --bundles deb,appimage
```

Les sidecars release sont copiés au préalable dans `src-tauri/binaries/` avec
leur suffixe `x86_64-unknown-linux-gnu`. La compilation release Tauri réussit
(10 min 53 s) et produit `release/bundle/deb/QoreDB_0.1.39_amd64.deb`
(95 Mio ; SHA-256 `ee98937b5c26072afeea31ef403fa7bb74e56da291e8ffc3d3e58a8501f030ba`).
Le bundling AppImage échoue ensuite : `failed to run linuxdeploy`, sortie 1.
Un répertoire `QoreDB.AppDir` existe, mais aucun AppImage réussi n'est établi.
La cause reste à diagnostiquer ; ce n'est pas une réussite de packaging globale.

À l'arrêt demandé de la session, ni inspection complète du payload candidat,
ni installation, ni démarrage des artefacts distribués ne sont acquis. Les essais
CLI/MCP décrits plus haut concernent les binaires debug, pas ces sidecars release.
Le conteneur PostgreSQL jetable a été arrêté après export de sa base synthétique ;
les profils, binaires debug, preuves et journal du bundling sont conservés localement.

## Blocages avant publication

1. Qualification positive avec une vraie licence Pro ; droits Team/Enterprise,
   expiration et reprise d'une licence existante. Sans clé légitime de test,
   Replay, Sandbox, Time Travel, Data API, diff avancé et bibliothèque avancée
   ne sont pas qualifiés positivement dans l'UI native.
2. Échec AppImage `linuxdeploy` à diagnostiquer ; qualification du payload `.deb`,
   installation et mise à jour
   par les mécanismes réels, dont signatures et auto-updater.
3. CI du HEAD final entièrement réussie ; aucun résultat historique n'y supplée.
4. Windows et macOS ARM/Intel, autres moteurs/transports concernés et budgets
   de performance de phase 6 non mesurés. Aucune machine correspondante disponible
   n'est revendiquée. La recette Linux n'est pas une validation multiplateforme.

La phase 6 reste ouverte. Les preuves locales générées (JSON, hashes, captures,
logs et profils) restent sous `.perf/qualification/` et `/tmp/qore-qualification-*` ;
les scripts suivis et le protocole du [guide de tests](../development/TESTING.md)
permettent de reproduire les assertions sans données personnelles.
