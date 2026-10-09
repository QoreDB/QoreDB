# v0.1.40 — Transferts de projets et ouverture de notebooks, 9 octobre 2026

## Référence et périmètre

Branche `feat/v0-1-40`, référence avant ce lot `0025bfc`. Suite du
[lot validation des données locales](V0_1_40_LOCAL_DATA_VALIDATION_2026-10-09.md).
Les fixtures utilisent uniquement des données synthétiques.

## Transferts de projets

Quatre tests reproduisaient les défauts avant correction : export contenant
les connexions de A et la bibliothèque de B après une bascule ; connexions déjà
sauvegardées avant le refus d'une bibliothèque malformée ; bibliothèque importée
dans B après une sauvegarde de connexion commencée dans A ; erreur de lecture
des connexions existantes assimilée à une liste vide.

L'export prend sa copie de bibliothèque avant l'attente des connexions et vérifie
que le contexte reste valide. L'import valide le format et la capacité de la
bibliothèque avant toute sauvegarde de connexion. Il échoue si la lecture des
connexions existantes échoue, conserve le renommage des collisions et les règles
de sécurité existantes, puis revalide la bibliothèque au moment de sa fusion.
Des modifications locales arrivées pendant l'import sont conservées.

Les opérations sont liées à une activation de workspace. Le début d'une
transition les invalide, y compris si elle échoue ou si l'utilisateur revient à A
après B. Rafraîchir les workspaces récents ne les invalide pas. Dans le formulaire,
la confirmation, le sélecteur de fichier et sa lecture ne peuvent pas relancer
une action périmée. Un verrou immédiat bloque les transferts concurrents.

Le backend desktop vérifie aussi le projet demandé pour `save_connection` et
`list_saved_connections`, à partir du même instantané que le répertoire de
stockage. Une demande pour A résolue alors que B est actif est rejetée avant accès
aux connexions. Un stockage déjà résolu pour A reste lié à A après une bascule.
Les contrôles de verrouillage du coffre, de licence et de masquage sont conservés.
Aucun contrat IPC ni format de fichier ne change.

La vérification des appels a aussi révélé que le formulaire de connexion envoyait
systématiquement `default`, y compris dans un workspace nommé. Ses deux chemins
de sauvegarde et sa connexion transmettent maintenant le projet actif, devenu
obligatoire dans les fonctions de construction des paramètres. Le formulaire
se ferme au changement de projet ; ses retours périmés ne déclenchent pas de
callback dans le nouveau contexte. Une session créée tardivement fait l'objet
d'une demande de déconnexion, vérifiée par le parcours navigateur. Un échec de
cette déconnexion n'est pas assimilé à une fermeture réussie.

## Ouverture de notebooks depuis les entrées principales

Le menu et la palette utilisent maintenant le même hook d'ouverture, tout en
conservant le chargement différé du module IO. Une réponse tardive après changement
de session, de workspace ou fermeture du composant ne crée pas d'onglet et ne
remplit pas le cache des notebooks en attente. Les doubles déclenchements pendant
une lecture n'ouvrent qu'un seul dialogue et un seul onglet.

Une erreur de format ou de lecture affiche le message traduit existant. Une
annulation ou une erreur libère le verrou et permet une nouvelle ouverture.
Le parcours interne d'ouverture d'un notebook déjà affiché garde ses protections
et ses tests existants.

## Vérifications

- `pnpm test:ts` : 384 tests réussis dans 42 fichiers, dont neuf nouveaux tests
  de transfert et quatre du contexte de workspace.
- `pnpm typecheck` : réussi.
- `pnpm exec biome check` sur les TS/TSX, la nouvelle fixture JSX et les locales
  modifiés : aucune erreur ; quatre avertissements préexistants (deux dépendances
  de hooks dans AppLayout et deux labels dans ProjectTransferCard).
- `node scripts/test-file-operations-ui.mjs` : 19 scénarios Chromium réussis.
- `node scripts/test-query-library-ui.mjs` : 24 scénarios réussis.
- `node scripts/test-notebook-ui.mjs` : 31 scénarios réussis.
- Tests de bibliothèque desktop Core : 276 réussis, aucun ignoré.
- Tests de bibliothèque desktop Pro : 644 réussis, aucun ignoré.
- `rustfmt --edition 2024 --check src-tauri/src/commands/vault.rs`,
  `pnpm docs:check` et `git diff --check` : réussis.

Les commandes Rust utilisent `cargo test --manifest-path src-tauri/Cargo.toml
-p qoredb --lib`, avec `--features pro` pour Pro, le cache Cargo local et son
chemin de bibliothèque DuckDB. Le test ajouté au coffre utilise de vrais fichiers
temporaires et la résolution d'identité employée par les commandes ; il n'appelle
ni le coffre système ni le transport Tauri. Sa première fixture supposait à tort
qu'un JSON invalide faisait échouer l'énumération ; elle a été remplacée par une
connexion valide pour vérifier directement le stockage d'origine.

Les scripts navigateur utilisent Vite sur le port 1430, Playwright via
`QOREDB_PLAYWRIGHT_MODULE` et Chromium via `QOREDB_CHROMIUM_EXECUTABLE`. La nouvelle
fixture monte le hook partagé, la vraie carte de transfert et le formulaire de
connexion ; elle ne monte pas
AppLayout entier. Les dialogues, fichiers et IPC natifs sont simulés, ainsi que les contextes de
licence et de plugins nécessaires au formulaire. Les parcours de création et de
modification vérifient explicitement le projet transmis aux commandes. Deux
assertions de fixture ont été ajustées : exclusion de la préférence de langue
dans le contrôle du stockage et attente du rendu React avant vérification du bouton.

## Limites restantes

L'import de projet n'est pas une transaction entre toutes les connexions et la
bibliothèque. Une connexion déjà sauvegardée peut subsister après erreur IO,
changement de contexte ou conflit de capacité apparu pendant l'import. Aucun
rollback global ni reprise idempotente n'est revendiqué. Les exports JSON/QNB
via le plugin filesystem ne gagnent pas de garantie d'écriture atomique dans ce lot.

Les autres commandes du coffre ne sont pas qualifiées par le contrôle ajouté
à la liste et à la sauvegarde. Les parcours natifs, les transports web/headless,
la migration complète d'un profil v0.1.39 et les distributions multiplateformes
restent hors de cette vérification. La CI du nouveau commit n'est pas qualifiée
par ces résultats locaux. La PR reste draft, sans bump ni publication.
