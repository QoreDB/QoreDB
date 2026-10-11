# v0.1.40 — Lecture des données locales, 9 octobre 2026

## Référence et périmètre

Branche `feat/v0-1-40`, référence avant ce lot `6a01ff9`. Ce lot complète
l'[intégrité de la bibliothèque](V0_1_40_LIBRARY_INTEGRITY_2026-10-09.md)
et les [notebooks](V0_1_40_NOTEBOOKS_2026-10-08.md). Données synthétiques uniquement ;
les formats restent en version 1.

## Bibliothèques illisibles et imports invalides

Le cache local invalide était assimilé à une bibliothèque vide : un ajout pouvait
écraser son contenu. Un chargement disque malformé pouvait remplacer le cache
valide. L'import ignorait certaines requêtes invalides et pouvait annoncer un
succès partiel. Sept tests ont échoué avant les corrections correspondantes.

Une validation commune contrôle maintenant la structure, les identifiants,
les types des champs utilisés dans l'interface et les variables avant toute
mutation issue d'un import. Les identifiants de dossiers ambigus et les références
vers des dossiers absents sont refusés à l'import. Les champs d'affichage
optionnels historiques reçoivent des valeurs par défaut en mémoire, sans
réécriture à la simple consultation. Les nombres SQL restent des chaînes exactes.

Une lecture invalide laisse les octets locaux en place. La bibliothèque et le
dialogue d'enregistrement affichent une erreur traduite dans les neuf langues ;
les actions d'écriture sont bloquées. Une réponse disque invalide ou une erreur
de lecture bloque les modifications du projet jusqu'à une lecture valide.
Après restauration du fichier ou du cache, le bouton Actualiser permet de reprendre.
Il ne s'agit pas d'une réparation automatique : un cache lui-même invalide doit
être restauré avant synchronisation. Le dialogue d'enregistrement se recharge
à sa réouverture.

Les changements locaux en attente pendant une lecture échouée restent présents
et sont sauvegardés lors d'une reprise valide. Une erreur d'écriture conserve
la bibliothèque locale consultable et ses changements en attente. Quitter un
projet dont le fichier est illisible reste possible si son cache valide n'a rien
à sauvegarder ; aucune écriture de ce fichier n'est alors effectuée.

La recherche globale efface les anciens résultats de bibliothèque lorsqu'elle
ne peut pas lire celle du nouveau projet. Un parcours Chromium couvre le passage
de A à B avec un cache B invalide.

## Configurations de notebooks

Le parseur acceptait des configurations imbriquées incompatibles avec leur usage :
namespace non textuel, label objet ou colonnes de graphique sous forme de chaîne.
Ces trois cas ont été reproduits avant correction. Les configurations sont
maintenant validées avant ouverture, notamment les types de graphique, colonnes,
bornes de lignes et indicateurs booléens. Les fichiers valides gardent leurs
configurations lors d'un aller-retour.

Un parcours utilisant le vrai hook de notebook vérifie qu'un fichier avec une
configuration de graphique invalide produit une erreur et laisse le document
courant intact. Il ne teste pas le rendu Recharts. Les résultats d'exécution
chargés sont déjà supprimés par le hook ; ce lot ne prétend pas valider leur
schéma arbitraire ni modifier cette politique.

## Vérifications

- `pnpm test:ts` : 371 tests réussis dans 40 fichiers, dont 28 bibliothèque
  et 23 IO notebook. Dix-sept cas supplémentaires dans ces deux suites.
- `pnpm typecheck` : réussi.
- `pnpm exec biome check` sur les fichiers TS/TSX, la fixture JSX et les locales
  modifiés : aucune erreur ; quatre avertissements d'accessibilité déjà présents
  dans les conteneurs de recherche globale.
- `node scripts/test-query-library-ui.mjs` : 24 scénarios Chromium réussis.
- `node scripts/test-notebook-ui.mjs` : 31 scénarios Chromium réussis.
- `pnpm docs:check` et `git diff --check` : réussis.

Les scripts navigateur utilisent Vite sur le port 1430, Playwright via
`QOREDB_PLAYWRIGHT_MODULE` et Chromium via `QOREDB_CHROMIUM_EXECUTABLE`.
Ils montent les composants/hooks React réels avec IPC, dialogues et fichiers
natifs simulés. Les tests IO notebook utilisent des fichiers temporaires Node.
Aucune recette Tauri native ni suite Rust n'a été relancée pour ce lot frontend.
La CI de la nouvelle tête n'est pas qualifiée par ces contrôles locaux.

L'import global de projet n'est pas une transaction couvrant connexions et
bibliothèque. Ce lot ne qualifie ni sa reprise partielle, ni la migration complète
d'un ancien profil. Recette native, distribution et performances multiplateformes
restent ouvertes ; la PR reste draft, sans bump ni publication.
