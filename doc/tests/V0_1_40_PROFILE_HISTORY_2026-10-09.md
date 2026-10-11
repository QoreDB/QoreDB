# v0.1.40 — Réouverture des profils Time Travel, 9 octobre 2026

## Référence et périmètre

Branche `feat/v0-1-40`, référence `63bf77f`. Ce lot qualifie la réouverture du
journal et des réglages Time Travel, avec des fichiers synthétiques au schéma
v0.1.39 vérifié au tag. Il ne qualifie pas la migration complète du coffre,
de la licence, des notebooks ou du profil natif. Aucun fichier personnel n'est lu.

## Défauts reproduits et corrections

Deux régressions échouaient avant correction lorsque `time-travel.json` était
illisible : une nouvelle capture pouvait être écrite avec les réglages par
défaut ; les neuf chemins de consultation testés acceptaient la politique de
confidentialité par défaut. La purge était déjà suspendue, mais les exclusions,
la désactivation et les colonnes sensibles personnalisées n'étaient plus fiables.

Une configuration invalide suspend désormais les captures et l'accès aux données
historiques, ainsi que la purge existante. La commande de lecture des paramètres
remonte une erreur ; le formulaire existant affiche son erreur traduite et refuse
de sauvegarder des valeurs par défaut. Aucun fichier n'est réinitialisé. Le
diagnostic de parsing ne recopie plus de valeur JSON dans les journaux.

Après restauration d'un fichier valide, le bouton de reprise des paramètres
relit la configuration sans redémarrage. La désactivation reste respectée si
elle figure dans le fichier restauré. Supprimer le fichier après une erreur
ne réactive pas les valeurs par défaut dans le store en cours. Une nouvelle
installation sans fichier conserve le comportement par défaut existant.

Une troisième régression a reproduit une lecture en attente utilisant les
anciens masques après récupération. Les consultations prennent maintenant le
verrou du journal avant de lire la politique ; celle-ci reste cohérente avec les
données consultées. Le défaut observé était une politique périmée, pas un deadlock.

Une quatrième régression a reproduit une capture préparée avant un changement de
réglages, puis enregistrée malgré une nouvelle exclusion de table ou le passage
à « production uniquement ». Le store revérifie ces restrictions sous le verrou
avant d'écrire. Les contrôles SQL et les résultats de la mutation ne changent pas.

## Compatibilité et récupération

La fixture v0.1.39 contient un événement sans identité stable de connexion ou
workspace, un entier `9007199254740993`, une colonne sensible personnalisée et une
configuration désactivée à rétention illimitée. Deux réouvertures et une passe de
rétention conservent les octets des deux fichiers. La lecture protège la colonne
sensible et conserve l'entier exact. Les identités manquantes restent absentes :
une nouvelle session ne récupère pas automatiquement cet ancien historique.

Une réactivation explicite puis une nouvelle ouverture conservent les exclusions,
les colonnes sensibles et la restriction à la production. Le journal n'est pas
réécrit par cette sauvegarde lorsque la politique de rétention ne l'exige pas.

Le fichier à restaurer est `time-travel/time-travel.json` sous le répertoire de
données locales de l'application, résolu par
[app_data_dir](../../src-tauri/crates/qore-service/src/paths.rs). Restaurer une
copie connue valide, puis réessayer depuis les paramètres ; ne pas supprimer le
journal pour réparer sa configuration. Une restauration ne chiffre ni ne purge
les valeurs historiques déjà présentes sur disque.

## Vérifications

Six nouveaux tests Rust couvrent les défauts et la compatibilité ci-dessus. Les
quatre régressions de défaut ont échoué avant leur correction respective. Le
test de concurrence utilise le store réel avec des lectures en attente pendant
la restauration, sans base distante ni transport simulant le store.

Commandes, avec le cache local et la bibliothèque DuckDB documentés pour les
lots précédents :

```bash
LIBRARY_PATH=/home/raphael/.cache/qoredb-v0-1-40-target/debug/deps cargo test --manifest-path src-tauri/Cargo.toml --target-dir /home/raphael/.cache/qoredb-v0-1-40-target -j 4 -p qoredb --features pro --lib
LIBRARY_PATH=/home/raphael/.cache/qoredb-v0-1-40-target/debug/deps cargo test --manifest-path src-tauri/Cargo.toml --target-dir /home/raphael/.cache/qoredb-v0-1-40-target -j 4 -p qoredb --lib
rustfmt --edition 2024 --check src-tauri/src/time_travel/store.rs src-tauri/src/time_travel/retention.rs src-tauri/src/commands/time_travel.rs
pnpm docs:check
git diff --check
```

Le parcours Chromium `scripts/test-time-travel-settings-ui.mjs` vérifie aussi
le refus des valeurs par défaut après erreur, la reprise avec des réglages
restaurés différents, puis la conservation des protections lors d'une sauvegarde.
Il utilise la vraie carte des paramètres avec IPC et licence simulés. Les
résultats sont : 642 tests Rust Pro et 274 Core réussis, aucun ignoré ; huit
scénarios Chromium réussis, sans erreur navigateur. Formatage Rust ciblé,
`pnpm docs:check` et `git diff --check` réussissent. Les suites Rust sont des
tests de bibliothèque ; elles ne certifient pas l'IPC natif ni tous les moteurs.

La qualification native Tauri, l'activation de licence et la migration des autres
composants du profil restent ouvertes. Aucun changement de bundle frontend ni
de dépendance : seules la logique Rust et la fixture navigateur sont modifiées.
