# v0.1.40 — Intégrité de la bibliothèque, 9 octobre 2026

## Référence et périmètre

Branche `feat/v0-1-40`, référence `beb33e3`. Ce lot complète la
[persistance par workspace](V0_1_40_QUERY_LIBRARY_2026-10-09.md) avec quatre
corrections : capacité, écriture interrompue, actions UI périmées et précision
des paramètres du chemin MCP. Les fixtures utilisent uniquement des données
synthétiques. Le format partagé reste en version 1.

## Capacité et imports

À 300 requêtes, l'ajout d'une requête supprimait silencieusement la dernière
requête enregistrée. Un import dépassant la capacité était partiel ; celui d'une
bibliothèque existante de plus de 300 entrées pouvait même la tronquer. Au-delà
de 100 dossiers, des requêtes importées perdaient leur rattachement.

Ces quatre cas sont reproduits par des tests qui échouaient avant correction.
Les limites existantes restent 300 requêtes et 100 dossiers. Un ajout ou import
qui les dépasse échoue désormais sans modifier le stockage. Le message traduit
dans les neuf langues explique la limite. Les bibliothèques historiques plus
grandes restent consultables, modifiables et exportables ; elles ne sont plus
tronquées lors d'un import refusé. Une suppression permet de libérer de la place.

L'enregistrement d'une requête et la création de son dossier utilisent une seule
écriture locale. Un refus ne laisse donc pas de dossier vide créé par le formulaire.
L'import à la limite exacte réussit et réutilise les dossiers de même nom. La
validation complète de fichiers arbitrairement malformés n'est pas qualifiée ici.

## Écriture du fichier partagé

Le chemin desktop utilisait une écriture directe de `queries/library.json`.
Un test injectant une erreur après les huit premiers octets a reproduit la
troncature de la version précédente. La persistance appartient maintenant à
`qore-service::workspace::query_library::write` et réutilise `PendingOutput` :
écriture dans un fichier voisin privé, fermeture, puis remplacement par renommage.
Le verrou et le contrôle d'identité du workspace restent dans la commande desktop.

Les tests couvrent la conservation octet pour octet du fichier précédent après
écriture partielle, l'absence de fichier JSON partiel lors d'une première
sauvegarde, l'échec de publication suivi d'une reprise, le remplacement réussi
et le nettoyage des fichiers temporaires après erreur traitée. Les champs JSON
additionnels et les valeurs SQL textuelles sont conservés. Les erreurs sont
propagées au mécanisme de reprise du frontend.

Il s'agit d'une injection déterministe d'erreur IO sur de vrais fichiers
temporaires, pas d'un disque physiquement saturé. Aucun test de coupure électrique,
de crash brutal ou de synchronisation simultanée par plusieurs instances n'est
revendiqué. La garantie concerne la publication d'un fichier complet ; ce lot
n'ajoute pas de protocole de durabilité avec `fsync`.

## Actions UI et changements de workspace

Un parcours Chromium a reproduit une suppression confirmée dans A qui supprimait
une requête de B portant le même identifiant. La bibliothèque garde maintenant
une instance par projet. Les confirmations et imports en attente sont invalidés
à la fermeture ou au changement de projet, y compris après un aller-retour A/B/A.
Le dialogue d'enregistrement se ferme lorsque son projet d'origine change.
Le dialogue de paramètres ne peut plus soumettre une ancienne requête après
bascule. Une bibliothèque visible suit les modifications locales et les nouveaux
chargements disque, sans nécessiter de fermeture/réouverture manuelle.

La recherche reste conservée lorsque le panneau est fermé puis rouvert dans le
même projet. Le chargement différé du panneau reste couvert par le parcours
notebooks existant. Les imports abandonnés et refus de capacité n'affichent pas
un succès ; le formulaire d'enregistrement reste ouvert après refus de capacité.

## Paramètres numériques du chemin MCP

La substitution partagée utilisée par `run_saved_query` convertissait les nombres
en `f64`, puis les reconvertissait en texte. Le test reproduit la transformation
de `9007199254740993` en `9007199254740992`. La validation utilise désormais la
grammaire décimale/exponentielle du frontend et conserve le littéral d'origine,
après retrait des espaces périphériques.

Les tests couvrent grands entiers, décimaux précis, exposants hors de la plage
`f64`, signes, fractions et valeurs par défaut. NaN, infinis, hexadécimaux,
fragments SQL, commentaires et chiffres non ASCII sont refusés. Le moteur cible
reste responsable de sa plage numérique. La substitution reste en une passe ;
les contrôles MCP d'exposition, lecture seule et gouvernance ne changent pas.
Le test porte sur le service appelé par MCP, pas sur un échange MCP avec une base.

## Vérifications

- 354 tests TypeScript réussis, dont 18 sur la bibliothèque (six nouveaux).
- 221 tests de bibliothèque `qore-service` réussis, dont six nouveaux pour
  l'écriture et les paramètres numériques ; aucun ignoré.
- 275 tests de bibliothèque desktop Core et 643 Pro réussis ; aucun ignoré.
- `cargo check -p qore-mcp` réussit avec les features par défaut.
- Dix-huit scénarios Chromium bibliothèque/workspaces réussis, dont huit nouveaux.
- Trente scénarios Chromium notebooks/bibliothèque réussis ; chargement différé
  et conservation de la recherche compris.
- Typecheck, Biome ciblé (traductions comprises), rustfmt ciblé, documentation
  et contrôle des espaces réussis.

La première passe Pro a échoué sur
`replay::recorder::tests::budget_stops_capture_and_says_so` (642 réussis, un échec).
Sa capture de dimensionnement comportait un horodatage à six décimales ; une
capture suivante peut porter neuf décimales et dépasser la marge d'un octet du
test. Le test réserve maintenant la variation maximale de dix octets du format
RFC3339 et vérifie explicitement que deux captures ne tiennent toujours pas dans
le budget. Aucun contrôle de production ni test n'est désactivé. La suite Pro
complète a été rejouée après ce correctif et les 643 tests passent.

Commandes de base, en réutilisant le cache local Cargo :

```bash
pnpm typecheck
pnpm test:ts
LIBRARY_PATH=/home/raphael/.cache/qoredb-v0-1-40-target/debug/deps cargo test --manifest-path src-tauri/Cargo.toml --target-dir /home/raphael/.cache/qoredb-v0-1-40-target -j 4 -p qore-service --lib
LIBRARY_PATH=/home/raphael/.cache/qoredb-v0-1-40-target/debug/deps cargo test --manifest-path src-tauri/Cargo.toml --target-dir /home/raphael/.cache/qoredb-v0-1-40-target -j 4 -p qoredb --lib
LIBRARY_PATH=/home/raphael/.cache/qoredb-v0-1-40-target/debug/deps cargo test --manifest-path src-tauri/Cargo.toml --target-dir /home/raphael/.cache/qoredb-v0-1-40-target -j 4 -p qoredb --features pro --lib
LIBRARY_PATH=/home/raphael/.cache/qoredb-v0-1-40-target/debug/deps cargo check --manifest-path src-tauri/Cargo.toml --target-dir /home/raphael/.cache/qoredb-v0-1-40-target -j 4 -p qore-mcp
rustfmt --edition 2024 --check src-tauri/crates/qore-service/src/workspace/query_library.rs src-tauri/src/commands/workspace_queries.rs src-tauri/src/replay/recorder.rs
pnpm docs:check
git diff --check
```

Exécuter `scripts/test-query-library-ui.mjs` et `scripts/test-notebook-ui.mjs`
avec Node contre Vite sur le port 1430. Les variables
`QOREDB_PLAYWRIGHT_MODULE` et `QOREDB_CHROMIUM_EXECUTABLE` sélectionnent les
installations locales de Playwright et Chromium. Les vrais composants React
sont utilisés, avec IPC et licence simulés. Aucune recette native Tauri ou
qualification Windows/macOS n'est déduite de ces tests Linux/Chromium.
