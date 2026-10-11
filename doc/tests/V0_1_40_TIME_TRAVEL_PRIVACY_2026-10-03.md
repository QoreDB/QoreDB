# v0.1.40 — Masquage des captures Time Travel, 3 octobre 2026

Référence Git : `4e1859c9c7d7ad775d9b5a172979469726a6f841`, avec les incréments
locaux précédents conservés, dont le [lot Sandbox](V0_1_40_SANDBOX_2026-10-03.md).
Cet incrément traite le masquage resté ouvert dans le
[rapport Time Travel](V0_1_40_TIME_TRAVEL_2026-10-03.md). Il ne qualifie pas la release.

## Défauts reproduits

Quatre tests ont échoué avant correction :

- Une clé primaire sensible restait en clair dans le JSONL, contrairement aux
  valeurs des images avant/après. La suppression ne parcourait pas les objets
  et tableaux JSON imbriqués ; des variantes comme `apiKey` ou `postalCode`
  échappaient aussi à la liste de colonnes sensibles.
- Une règle ajoutée après la capture n'affectait ni sa lecture ni sa recherche.
  Le compteur pouvait donc révéler la présence d'une clé devenue sensible.
- Deux clés contenant le même marqueur de suppression étaient regroupées dans
  le diff comme s'il s'agissait de la même ligne.
- Le rollback acceptait un marqueur dans un objet JSON imbriqué ou le marqueur
  d'affichage masqué, et pouvait proposer de les réécrire dans la base.

Les données de test sont synthétiques. Les fichiers de journal sont créés dans
des répertoires temporaires ; aucune base ou capture utilisateur n'est modifiée.

## Comportement livré

La [politique de confidentialité du journal](../../src-tauri/src/time_travel/privacy.rs)
combine la liste de colonnes sensibles de Time Travel et les règles de masquage
de la connexion, résolues depuis la session active du backend. Le frontend ne
fournit pas cette politique. La correspondance de table/colonne réutilise
`ConnectionMasking::mode_for` ; les chemins JSON imbriqués et pointés sont couverts.
La liste Time Travel ignore casse et séparateurs, comme la détection standard.

Les captures directes INSERT/UPDATE/DELETE et les lots Sandbox transmettent les
règles courantes avant l'écriture dans le cache et le fichier. Les clés primaires,
les images avant/après, les objets et les tableaux sont parcourus. Un champ
protégé reçoit `[REDACTED]` quel que soit le mode d'affichage de la connexion
(masqué, partiel ou empreinte) : le journal ne doit pas fournir de valeur partielle
ou hachée comme donnée de restauration.

Chaque nouvelle lecture réapplique la configuration actuelle, y compris aux
captures chargées depuis un ancien fichier : timeline, compteur, historique,
état à une date, diff, export et entrée sélectionnée pour un rollback. Les filtres
sur la clé opèrent sur sa version protégée. Une recherche d'un ancien secret ne
renvoie donc ni l'entrée ni un compteur positif révélant cette valeur.

Une clé vide, masquée ou binaire n'est pas utilisée pour identifier une ligne.
La timeline conserve l'événement mais ne propose plus l'historique de ligne
pour cette clé. Les commandes d'historique/état refusent explicitement la
recherche ; le générateur ne produit pas de rollback utilisant cette clé.
Le diff exclut les lignes sans clé exploitable et positionne `incomplete`.
Pour une clé exploitable avec des champs protégés, les valeurs visibles restent
comparables, mais `incomplete` signale que le résultat n'est pas exhaustif.

Les marqueurs sont reconnus récursivement par le générateur de rollback. Les
arguments de clé et de recherche sont exclus des spans de tracing des commandes
Time Travel ; les erreurs de lecture d'image avant passent par la sanitisation
moteur avant journalisation.

Le compteur et la pagination filtrent les métadonnées sans recopier les images
des lignes. Le diff protège une entrée à la fois pour éviter de constituer une
seconde copie complète des images retenues en mémoire.

## Validation

| Critère | Preuve | Résultat |
| --- | --- | --- |
| Aucune valeur couverte par la politique dans les nouvelles captures | Lecture du JSONL avec clé sensible, variantes d'identifiants, objet et tableau imbriqués | Réussi |
| Règles de connexion prises en compte à l'écriture | Masque personnalisé sur clé, chemin imbriqué et chemin pointé ; relecture du fichier | Réussi |
| Capture Sandbox protégée | Appel du helper de capture après confirmation et contrôle du JSONL | Réussi |
| Anciennes captures protégées à la lecture | Réouverture du store puis règles masqué/partiel/empreinte ; historique, état, diff, export, sélection et rollback | Réussi |
| Recherche et compteur sans accès aux valeurs cachées | Règle globale ajoutée après capture et règle de connexion ajoutée après capture | Réussi |
| Clés masquées sans fusion ni recherche de ligne | Deux événements avec la même clé supprimée ; diff incomplet et historique/état indisponibles | Réussi |
| Rollback sans marqueur imbriqué ou d'affichage | Test échouant avant correction puis réussi | Réussi |
| Portée d'une règle | Une règle sur une autre table laisse visibles les champs non concernés | Réussi |
| IPC et UI natifs avec changement de règle pendant une session | Aucune application connectée au pont Tauri lors du contrôle | Non exécuté |

Commandes réussies :

```bash
cargo test --manifest-path src-tauri/Cargo.toml -p qoredb --lib
cargo test --manifest-path src-tauri/Cargo.toml -p qoredb --features pro --lib
rustfmt --check --edition 2024 src-tauri/src/time_travel/mod.rs src-tauri/src/commands/time_travel.rs src-tauri/src/commands/mutation.rs src-tauri/src/commands/sandbox.rs
pnpm docs:check
git diff --check
```

Résultats : 227 tests Core, 553 Pro, aucun test ignoré dans ces suites. Huit tests
sont ajoutés ; le module Time Travel compte 42 tests. La couverture porte sur
le store, le générateur et le helper de capture, avec compilation des commandes
Pro. Elle ne remplace pas un appel IPC réel ni une recette de licence.

Aucun fichier frontend ni contrat de réponse n'est ajouté par cet incrément.
TypeScript, bundle Vite et performances natives ne sont pas remesurés ici ;
la [mesure frontend du lot précédent](V0_1_40_SANDBOX_2026-10-03.md) reste une
preuve datée. Aucune dépendance ajoutée.

## Limites

Les captures déjà présentes sur disque ne sont pas réécrites ni purgées par
un changement de règle. Les nouvelles lectures via l'application sont protégées,
mais un ancien fichier ou une ancienne sauvegarde peut encore contenir les
valeurs historiques en clair. Le masquage n'est pas un chiffrement du journal
ni une autorisation de base de données.

Le retrait d'une règle peut rendre visible une ancienne capture qui contenait
encore sa valeur d'origine ; il ne permet pas de récupérer une valeur supprimée
à l'écriture. Des images anciennes issues d'un affichage partiellement masqué
ou haché peuvent manquer d'information sur leur origine. Les marqueurs connus
sont refusés, mais une chaîne arbitraire ne permet pas de déterminer si elle
est une vraie valeur ou une ancienne empreinte.

Certains INSERT utilisent encore l'ensemble des valeurs fournies comme identité
capturée. Si cet ensemble contient un champ protégé, son historique de ligne et
son rollback deviennent indisponibles ; la récupération systématique de la vraie
clé générée par le moteur reste à qualifier. Les triggers/coercitions serveur,
l'isolation indépendante par workspace, la couverture hors cache, les autres
moteurs et la recette native restent ouverts dans le plan v0.1.40.
