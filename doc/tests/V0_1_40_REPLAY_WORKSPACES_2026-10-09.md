# v0.1.40 — Cloisonnement des enregistrements Replay, 9 octobre 2026

## Périmètre et comportement retenu

Branche `feat/v0-1-40`, HEAD `79a3664`, changements locaux antérieurs conservés.
Ce rapport complète les [références Replay](V0_1_40_REPLAY_2026-10-09.md).
L'utilisateur a choisi le cloisonnement complet : un enregistrement continue
dans son workspace d'origine ; il faut y revenir pour voir son nom, ses aperçus,
le sauvegarder ou l'abandonner. Un seul enregistrement peut rester actif à la
fois ; le refus d'un deuxième départ ne révèle ni nom ni requête de l'origine.

Critères observables : aucun statut ou aperçu étranger, aucune action d'un
workspace étranger ou d'un ancien enregistrement sur le courant, retour à
l'origine possible, absence de notification tardive dans un autre workspace.

## Lot 1 — Identité du workspace et de l'enregistrement

Deux tests de régression échouent avant les contrôles du recorder : accès depuis
un autre workspace et action conservant l'identifiant d'un ancien enregistrement.
Les contrôles portent désormais sur le couple workspace/UUID sous le même verrou
que la lecture ou l'action. Le statut étranger est absent et ses aperçus sont
vides ; sauvegarde, abandon et suppressions sont refusés sans appeler leur
callback de publication ou de nettoyage.

Les commandes IPC reçoivent le workspace attendu ; celles qui ciblent un
enregistrement reçoivent aussi son UUID. Elles vérifient le workspace actif et
le gardent verrouillé pendant l'opération. Le démarrage vérifie également la
connexion liée à ce workspace. Les chemins de capture restent ceux de l'origine.
Les chemins internes de capture des requêtes continuent de fonctionner après
un changement de workspace.

Le témoin est remonté par workspace et masque immédiatement les informations
précédentes. Témoin et hook ignorent leurs anciennes réponses et callbacks, y
compris un aller-retour vers le même workspace. Statut et aperçus du hook sont
publiés ensemble ; un ancien polling ne remplace pas une lecture plus récente.
Les boutons du témoin bloquent une deuxième action pendant la première.

Le contrôle Pro au démarrage, le masquage avant capture, les limites de capture
et la politique production sont conservés. Aucune nouvelle requête moteur n'est
exécutée par ces contrôles. Lecture seule, Sandbox et confirmation des mutations
restent dans les chemins d'exécution existants. Aucun changement de droits ER,
de version, de dépendance ou de licence ; sources et nouvelles fixtures BUSL.
Pas de nouveau texte d'interface nécessitant une clé de traduction.

## Lot 2 — Erreurs de suppression explicites

Le nettoyage des captures ne masque plus ses erreurs d'inspection ou suppression.
Un abandon échoué conserve l'enregistrement en mémoire. Une entrée est retirée
seulement après suppression réussie de sa capture. Pour un lot de mutations,
les suppressions déjà réalisées restent acquises ; l'erreur est remontée et les
entrées restantes gardent un ordre cohérent. L'interface recharge cet état après
échec pour permettre une nouvelle tentative, sans faux succès.

Deux tests supplémentaires couvrent un échec injecté d'abandon, une vraie erreur
fichier/répertoire pour une entrée et un échec après une première suppression
réussie, suivi du retry. Ce n'est pas un test de disque plein ni une garantie de
rollback du nettoyage : une suppression récursive échouée peut être partielle.

## Vérifications exécutées

- 90 tests unitaires Replay Pro réussis, aucun ignoré : quatre nouveaux tests
  dans ce lot, les autres requalifiés. Drivers simulés et fichiers temporaires
  réels ; ce ne sont pas des commandes IPC exécutées dans une fenêtre Tauri.
- 268 tests unitaires desktop Core réussis, aucun ignoré.
- 15 nouveaux scénarios Chromium : changement et retour de workspace, statut et
  aperçus tardifs, succès/erreur tardifs, ancien callback, payloads de démarrage,
  sauvegarde, abandon et suppressions, échec d'abandon puis retry. Composant et
  hook réels, transport IPC et licence simulés, données synthétiques uniquement.
- Les 17 scénarios Chromium Replay existants passent aussi (rapport, annulation,
  réponses tardives, sélection et contexte). Ne pas les compter comme des tests
  natifs ou des tests avec bases réelles.
- `pnpm typecheck`, Biome sur les trois sources frontend, rustfmt ciblé en
  édition 2024, `pnpm docs:check` et `git diff --check` passent.

```bash
LIBRARY_PATH=/home/raphael/.cache/qoredb-v0-1-40-target/debug/deps cargo test --manifest-path src-tauri/Cargo.toml --target-dir /home/raphael/.cache/qoredb-v0-1-40-target -j 4 -p qoredb --features pro --lib replay:: -- --test-threads=1
LIBRARY_PATH=/home/raphael/.cache/qoredb-v0-1-40-target/debug/deps cargo test --manifest-path src-tauri/Cargo.toml --target-dir /home/raphael/.cache/qoredb-v0-1-40-target -j 4 -p qoredb --lib
pnpm typecheck
pnpm exec biome check src/lib/replay.ts src/hooks/useReplay.ts src/components/Replay/ReplayIndicator.tsx
rustfmt --edition 2024 --check src-tauri/src/commands/replay.rs src-tauri/src/replay/recorder.rs src-tauri/src/replay/capture.rs
pnpm docs:check
git diff --check
```

Les scripts navigateur nécessitent Vite sur `http://127.0.0.1:1430` et
Playwright/Chromium disponibles ; les chemins locaux utilisés ici sont ignorés
par Git. Aucun ajout de dépendance au projet.

```bash
pnpm exec vite --host 127.0.0.1 --port 1430 --strictPort
QOREDB_PLAYWRIGHT_MODULE=../.perf/browser-tools/node_modules/playwright/index.mjs QOREDB_CHROMIUM_EXECUTABLE=/home/raphael/.cache/ms-playwright/chromium-1243/chrome-linux64/chrome node scripts/test-replay-workspaces-ui.mjs
QOREDB_PLAYWRIGHT_MODULE=../.perf/browser-tools/node_modules/playwright/index.mjs QOREDB_CHROMIUM_EXECUTABLE=/home/raphael/.cache/ms-playwright/chromium-1243/chrome-linux64/chrome node scripts/test-replay-ui.mjs
```

## Légèreté : deux comparaisons distinctes

Mesure avant ce lot : `.perf/v0-1-40-2026-10-09-workspace-before.json`.
Le lot ajoute 700 octets JS initiaux, soit 157 octets gzip (+0,022 %), et
1 217 octets JS totaux, soit 209 octets gzip. La comparaison stricte à zéro
croissance par rapport à cet état immédiat échoue donc sur les deux métriques
JS initiales (six autres contrôles passent). Ce résultat n'est pas un succès.

Les huit budgets de la mise à jour passent face à la référence comparable déjà
conservée du 8 octobre, sans changer les seuils. Métadonnées d'environnement,
configuration et lockfile identiques : JS initial 2 426 896 octets / 708 084 gzip,
soit −7 763 / −950 ; JS total +0,361 % brut / +0,506 % gzip, sous le budget 2 %.
CSS inchangé. Les artefacts finaux sont `workspace-release.json` (référence du
8 octobre) et `workspace-after.json` (même mesure, comparaison à l'avant-lot),
préfixés `.perf/v0-1-40-2026-10-09-`.

```bash
pnpm perf:bundle --output .perf/v0-1-40-2026-10-09-workspace-before.json
pnpm perf:bundle --output .perf/v0-1-40-2026-10-09-workspace-release.json --baseline .perf/v0-1-40-2026-10-08-before.json
```

## Limites et prochaine qualification

Aucune base réelle réexécutée pendant ce lot ; les douze scénarios PostgreSQL
précédents restent une preuve distincte. Transport et fenêtre Tauri non exécutés,
ainsi que Windows/macOS, autres moteurs, migration d'un profil v0.1.39,
performances natives et packaging AUR. Aucun pourcentage global réévalué ni
phase complète déclarée validée.

Prochaine action : scénario natif avec deux workspaces synthétiques, enregistrement
A en cours, bascule B, tentative de commande visant A depuis B, retour A puis
sauvegarde et réouverture. Vérifier aussi un résultat moteur qui termine pendant
la bascule. L'isolation du suivi d'un rejeu déjà lancé constitue un contrôle
séparé du cloisonnement de l'enregistrement traité ici.
