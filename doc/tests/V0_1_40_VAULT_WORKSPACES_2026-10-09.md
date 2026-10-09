# v0.1.40 — Coffre et workspaces, 9 octobre 2026

## Référence et périmètre

Branche `feat/v0-1-40`, référence avant ce lot `22457b3`. Complément du
[lot transferts et fichiers](V0_1_40_FILE_OPERATIONS_2026-10-09.md) pour les actions
sur les connexions sauvegardées. Les fixtures utilisent des données synthétiques.

Critères retenus : une opération demandée pour A ne doit pas résoudre le stockage
de B ; les confirmations et retours asynchrones de A ne doivent pas agir dans B,
y compris après un aller-retour A → B → A ; les actions valides et leur reprise
après erreur doivent fonctionner dans les workspaces nommés.

## Corrections

Le résolveur desktop vérifie le projet demandé sous le verrou du gestionnaire de
workspaces, dans le même instantané que le chemin du stockage. La suppression,
la duplication, la lecture des identifiants, l'exposition aux agents et le masquage
l'utilisent désormais, ainsi que la résolution des connexions pour tester ou ouvrir
une connexion sauvegardée. La liste et la sauvegarde gardent ce contrôle via le
résolveur commun. Une opération déjà résolue reste liée à son stockage initial.
Une ancienne étiquette `default` dans les métadonnées d'un workspace nommé ne
remplace pas l'identité calculée par le backend. Les contrôles de verrouillage,
d'authentification récente, de licence et de propagation du masquage sont conservés.

Les actions des menus transmettent le projet actif au lieu de `default`. Elles
écartent les réponses périmées, notamment les mots de passe reçus après une bascule
ou la fermeture du composant. Les confirmations de suppression gardent l'action
initiale ; la confirmation de retrait d'un masque garde aussi sa table et sa
connexion. Une politique indisponible ne peut pas être remplacée par une règle
construite à partir d’une politique vide ; le chargement rétablit les actions.
L'exposition aux agents ne rafraîchit pas un autre workspace après
une réponse tardive. Un verrou immédiat empêche les doubles mutations concurrentes
dans ces contrôles.

Un hook commun protège les listes de connexions de la sidebar, de la recherche et
du contexte de session utilisé par les réglages. La liste précédente est cachée
dès que son activation devient invalide ; une réponse ancienne ne peut pas
remplacer une lecture plus récente. Une erreur donne une liste vide et permet un
nouvel essai. La réconciliation des favoris attend une lecture réussie.

Le binding partagé de connexion refuse une demande déjà périmée. Si une session
arrive après le changement de contexte, il demande sa déconnexion et ne la renvoie
pas aux appelants. Un échec de fermeture produit un avertissement et reste un
échec de nettoyage ; il n'est pas assimilé à une session fermée.

## Vérifications

- Deux nouvelles régressions Rust ont échoué avec le résolveur encore non protégé,
  puis réussi après correction : projet périmé et bascule pendant l'attente du verrou.
  Les quatre tests du contexte utilisent de vrais répertoires temporaires.
- Bibliothèque desktop Core : 278 tests réussis, aucun ignoré.
- Bibliothèque desktop Pro : 646 tests réussis, aucun ignoré.
- `pnpm test:ts` : 392 tests réussis dans 43 fichiers, dont huit tests du binding
  de connexion (contexte valide, refus avant IPC, retours tardifs, fermeture en erreur).
- `pnpm typecheck` : réussi.
- `node scripts/test-vault-workspaces-ui.mjs` : 26 scénarios Chromium réussis.
  Actions des menus, identifiants tardifs, confirmations réelles des deux menus,
  exposition aux agents, confirmation de masquage en production, listes concurrentes
  et reprise après erreur sont exercées, avec des identifiants identiques dans A et B.
- `node scripts/test-file-operations-ui.mjs` : 19 scénarios réussis, dont la
  déconnexion d'une session créée après invalidation du formulaire.
- `node scripts/test-query-library-ui.mjs` : 24 scénarios réussis, dont la recherche.
- Biome ciblé : aucune erreur ; quatre avertissements d'accessibilité préexistants
  dans GlobalSearch. Rustfmt ciblé, `pnpm docs:check` et `git diff --check` : réussis.

Les commandes Rust utilisent `cargo test --manifest-path src-tauri/Cargo.toml
-p qoredb --lib`, avec `--features pro` pour Pro, le cache Cargo local et son
chemin de bibliothèque DuckDB. Les scripts navigateur utilisent Vite sur le port
1430, Playwright via `QOREDB_PLAYWRIGHT_MODULE` et Chromium via
`QOREDB_CHROMIUM_EXECUTABLE`.

## Limites

Les tests Rust couvrent le résolveur commun, pas les commandes via le transport
Tauri ni le coffre système. Les tests navigateur montent les vrais hooks, menus,
réglages agents et confirmation de masquage, avec IPC, licence et contexte de session
simulés. Le hook de liste est monté directement ; la sidebar et SessionProvider
complets ne font pas partie de cette fixture. Les tests du binding ne créent pas
de vraie connexion réseau. Aucun audit exhaustif du cycle de vie des sessions,
du transport web/headless ou de la concurrence entre plusieurs processus n'est revendiqué.

Une mutation déjà partie peut terminer dans son workspace d'origine ; il n'y a
pas d'annulation ni de rollback global. L'atomicité des fichiers de connexion,
la migration complète d'un profil v0.1.39, le parcours natif et les distributions
multiplateformes restent à qualifier séparément. Ces résultats locaux ne qualifient
pas la CI du nouveau commit. La PR reste draft, sans bump ni publication.
