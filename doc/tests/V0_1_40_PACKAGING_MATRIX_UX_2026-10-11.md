# v0.1.40 — Packaging Linux, fidélité de la fédération et clavier, 11 octobre 2026

## Périmètre

Départ : `8d163ee359dfe12ecd660c495101d576854f2fbe`, avec seulement
`HANDOFF.md` non suivi. Les premières sections décrivent le lot initial, encore
en version de travail 0.1.39. La section « Préparation du candidat 0.1.40 »
décrit ensuite le bump, les commits poussés et les recettes du paquet signé.
Aucun tag, publication, activation de licence ou changement du profil personnel.

Critères : produire et démarrer l'AppImage ; préserver les outils du paquet
AUR et leurs protections ; qualifier des moteurs supplémentaires ; reproduire
puis corriger les pertes de fidélité de la fédération et les conflits clavier ;
mesurer la grille native et conserver les budgets frontend.

## Packaging

L'échec de `linuxdeploy` est reproduit avec `tauri bundle --verbose` : son
`strip` embarqué rejette les sections ELF `.relr.dyn` des bibliothèques Arch.
`NO_STRIP=1` évite ce second stripping ; les exécutables restent dépouillés
par le profil Cargo `strip = "symbols"`. Le wrapper `pnpm tauri` applique ce
réglage aux commandes Linux `build` et `bundle`, et le workflow de release
l'applique aussi. Le test du wrapper échoue avant correction et passe après.
Le mécanisme est documenté par [linuxdeploy](https://github.com/linuxdeploy/linuxdeploy/issues/72).

Le premier AppImage est construit depuis les binaires du lot précédent,
avec les bibliothèques locales. Ses 42 assertions de profil natif passent.
Le paquet AUR est construit avec `makepkg` depuis le `.deb` candidat, avec
son vrai checksum, puis installé par `pacman` dans une racine jetable sous
namespace utilisateur Bubblewrap. Les 42 assertions passent aussi sur
l'exécutable ainsi installé. Les CLI/MCP de ces deux distributions effectuent
les lectures masquées et exactes et refusent UPDATE/DELETE.

Après les corrections Rust et clavier, la compilation release Pro avec DuckDB
embarqué réussit. Le wrapper corrigé produit de nouveau les deux paquets `.deb`
et AppImage, sans signatures updater. Les outils `qore` et `qore-mcp` extraits du
`.deb` final correspondent aux binaires release et exécutent `--help` sans GUI.
Le candidat final n'est pas redémarré graphiquement. Les fichiers gardent la
version de travail `0.1.39` : ce ne sont pas des releases v0.1.40 publiées.

Cette installation utilise les bibliothèques de l'hôte : la résolution de
dépendances et les scripts système sont désactivés dans la racine jetable
(`-Udd --noscriptlet --disable-sandbox`). Ce n'est pas une installation Arch
vierge ni une installation Debian. À ce premier point d'arrêt, l'installation
`.deb` sur Debian/Ubuntu, les signatures de production et l'auto-updater réel
restent non exécutés ; leur recette ultérieure figure à la fin du rapport.
Un build Arch ne démontre pas la compatibilité avec les distributions plus
anciennes ; le workflow de distribution conserve son environnement Ubuntu.

## Matrice et corrections

| Vérification | Résultat |
| --- | --- |
| MySQL 8, conteneur jetable | CRUD, pagination keyset et streaming : 3 tests passent |
| MongoDB 7, conteneur jetable | CRUD et streaming : 2 tests passent |
| Redis 7, conteneur jetable | Test réel : réussi |
| ClickHouse 24, conteneur jetable | Test réel : réussi ; le premier essai avec mot de passe sur HTTP est correctement refusé, puis la fixture sans mot de passe est utilisée |
| Elasticsearch 8.13.4, conteneur jetable | Pagination sans comptage : réussie |
| DuckDB embarqué | Pagination réelle : réussie ; 15 tests driver passent en série |
| Service partagé | 234 tests réussis, dont 19 de fédération |
| Desktop Pro / Core | 647 / 279 tests unitaires réussis |

Les services sont dédiés, publiés uniquement sur localhost et indépendants
des conteneurs personnels. Les variables `QOREDB_TEST_*_REQUIRED=true`
interdisent les skips silencieux. Les ports sont 13346 (MySQL), 27347 (MongoDB),
16347 (Redis), 18147 (ClickHouse) et 19247 (Elasticsearch).

La première exécution parallèle des tests DuckDB a échoué sur le délai
d'annulation, pendant la compilation release. Le test isolé puis les 15 tests
en série passent. Aucun correctif d'annulation n'est revendiqué ; le premier
échec reste une limite de cette preuve sous charge.

Trois régressions reproduisent des pertes en fédération avant correction :
`'00123'` devient `123`, `1.25` devient `1`, un décimal et un grand entier
sont altérés ; les dates deviennent des nombres de jours. Les noms contenant
un guillemet rendent la création des tables temporaires invalide. Le service
réutilise maintenant le codec typé du driver DuckDB, conserve les métadonnées
de résultat et échappe les identifiants. Les erreurs de lecture remontent.

Les sources NUMERIC/DECIMAL/MONEY utilisent une précision et une échelle
calculées sur toutes les lignes chargées, sans passage par DOUBLE. Le codec
Rust utilisé par DuckDB impose une limite conservatrice de 28 chiffres : les
sources dépassant cette limite ou contenant une valeur non représentable sont
refusées explicitement. Les colonnes masquées restent textuelles. Les tests
couvrent aussi échelles différentes, notation scientifique, agrégation,
refus de dépassement et absence d'accès fichier après restriction du moteur.
Il s'agit de tests du moteur de fédération réel en mémoire, pas d'une recette
UI Pro activée ni d'une qualification exhaustive des jointures hétérogènes.

## Clavier et mesures

Dans l'application installée, Échap annule une cellule puis ferme aussi son
onglet : le gestionnaire global ignorait `defaultPrevented`. Le défaut est
reproduit nativement et dans la fixture Chromium enrichie du vrai
`ShortcutProvider`. Celui-ci respecte désormais les événements consommés,
la composition IME et l'ouverture de l'aide clavier. Les tests vérifient
l'annulation sans fermeture, l'aide clavier, le maintien du raccourci Échap
hors éditeur et les parcours existants d'édition, relecture, erreurs et focus.

Le [script de grille native](../../scripts/qualify-native-grid.mjs) utilise
une table SQLite synthétique de 100 000 lignes, avec texte de 1 024 octets et
entier `9007199254740993`. Il charge au moins cinq pages, effectue 30 éditions,
vérifie position/sélection/focus et attend après Échap pour détecter une fermeture
tardive. Les événements DOM sont injectés dans la vraie WebView ; IPC, moteur,
stockage et relecture ne sont pas simulés. Cela ne remplace pas la recette des
touches physiques ni celles des autres plateformes.

Les huit budgets frontend passent face à la mesure précédant le correctif
clavier : JS initial −18 octets bruts / −2 gzip ; CSS inchangé. Ce bilan porte
sur deux builds Vite locaux comparables, pas sur la release publiée v0.1.39.

Le [script mémoire/CPU](../../scripts/measure-native-process.py) observe un PID
explicitement choisi sans lancer l'application. Il conserve les valeurs brutes
et annonce `comparison: not-requested`, sans déclarer de budget natif validé.
RSS compte les pages partagées par processus ; PSS est fourni lorsque disponible.
La capture locale suit la fermeture de l'onglet par Échap : elle ne démontre
donc pas le budget d'une grille ouverte.

La recette graphique est interrompue à la demande de l'utilisateur. Le défaut
Échap est reproduit dans le binaire précédent ; le correctif est vérifié dans
Chromium sans interface visible, mais sa nouvelle recette native reste ouverte.
Le script ajouté pour répéter les démarrages est retiré. Une série locale sur
le binaire précédent ne constitue pas une comparaison avec le candidat.

## Commandes et limites restantes

```bash
node --test scripts/tauri.test.mjs scripts/aur-package.test.mjs
cargo test --manifest-path src-tauri/Cargo.toml -p qore-service --lib
cargo test --manifest-path src-tauri/Cargo.toml -p qoredb --features pro --lib
cargo test --manifest-path src-tauri/Cargo.toml -p qoredb --lib
cargo test --manifest-path src-tauri/Cargo.toml -p qore-drivers --lib drivers::duckdb -- --test-threads=1
node scripts/test-inline-edit-ui.mjs
pnpm typecheck
pnpm exec biome check src/providers/ShortcutProvider.tsx scripts/fixtures/inline-edit/app.jsx
pnpm perf:bundle --baseline .perf/v0-1-40-2026-10-11-before.json --output .perf/v0-1-40-2026-10-11-final.json
```

Les tests natifs positifs Pro, Windows/macOS ARM/Intel, les services cloud,
la matrice fonctionnelle complète et le protocole comparatif complet de phase 6
restent ouverts. Les 30 lancements chauds ne couvrent pas les démarrages froids,
les cinq sessions de mémoire par scénario ou les cinq séries de 20 cycles
d'ouverture/fermeture de panneaux. Les logs, JSON et captures locales sont
conservés sous `.perf/qualification/evidence/`, sans données personnelles.

## Préparation du candidat 0.1.40

Le lot suivant passe `package.json`, les trois paquets Cargo distribués et leur
lockfile à `0.1.40`. Le changelog intégré décrit les corrections livrées. La
fenêtre des nouveautés est chargée à son ouverture pour conserver les huit
budgets frontend : JS initial −960 octets bruts / −32 gzip face à la même
référence locale ; CSS inchangé.

Les 397 tests TypeScript passent, ainsi que typecheck, contrôle documentaire et
Biome (avertissements existants). Les tests du manifeste updater refusent une
matrice incomplète, une cible dupliquée, une signature absente et un tag différent
de la version. Le vérificateur d'artefacts utilise Minisign avec la clé publique
embarquée. Ses essais cryptographiques passent, y compris les refus d'un contenu
altéré et d'une autre clé. Le paquet `.deb` officiel 0.1.39 téléchargé auparavant
est également vérifié contre la signature de son `latest.json` et cette clé.
Ce contrôle confirme la racine de confiance précédente, pas l'installation de
la nouvelle version.

Le workflow `Signed release candidate` prépare les quatre builds signés sur les
runners GitHub, contrôle la signature système macOS et vérifie ensuite la matrice
updater complète. Il conserve des artefacts Actions ; il ne publie ni release,
ni tag, ni endpoint updater. Son résultat doit être lu avant de déclarer le
candidat signé. Les recettes graphiques et mesures natives restent distinctes.

### Recette native isolée du candidat

La nouvelle recette utilise un seul lancement de l'AppImage local `0.1.40` dans
un conteneur Arch avec Xvfb et rendu logiciel. Aucun socket X11/Wayland de l'hôte
n'est monté ; seuls les scripts et l'AppImage en lecture seule, ainsi que le
profil synthétique en lecture-écriture, sont accessibles. L'inspecteur est
publié sur `127.0.0.1:9343`. La fenêtre virtuelle est fermée proprement après
les contrôles et le conteneur est supprimé automatiquement.

- Les 42 assertions de profil natif passent sur l'AppImage final local.
- La grille de 100 000 lignes conserve 500 lignes chargées, position, sélection
  et focus pendant 30 éditions. Échap conserve la valeur et l'onglet ouvert,
  y compris après le délai de propagation. La base SQLite relue contient
  `Native edit 29` et l'entier exact attendu.
- Latence mesurée : médiane 104 ms, p95 179 ms. Ce sont des événements DOM dans
  WebKit avec polling de 10 ms, sur écran virtuel ; aucune référence comparable
  n'est disponible pour déclarer un budget natif respecté.
- Une session de mémoire/CPU, après 30 s de repos puis 60 échantillons à une
  seconde : RSS médian/pic 853 516 288 octets, CPU moyen 0,0162 % d'un cœur.
  La grille reste ouverte. RSS inclut les pages partagées de chaque processus.
  Cela ne remplace pas les cinq sessions comparatives du protocole complet.
- Les sept contrôles CLI/MCP sur les exécutables de cette AppImage passent :
  découverte, lectures exactes/masquées et refus des mutations.
- L'updater natif confirme la version courante `0.1.40` et ne propose pas de
  rétrogradation vers `0.1.39` lors du contrôle normal. Un appel de qualification
  autorisant explicitement la recherche de l'ancienne version télécharge ses
  160 037 368 octets et valide sa signature via le vrai plugin ; aucune
  installation n'est demandée. Ce test couvre transport et vérification, pas
  le remplacement de `0.1.39` par le nouveau candidat signé.

L'image de test installe Xvfb 21.1.25 et glibc 2.44 ; l'AppImage utilise ses
bibliothèques embarquées. Les nouveautés sont aussi vérifiées dans Chromium
sans fenêtre visible : ouverture après migration de version, chargement différé,
fermeture par bouton/Échap et réouverture sans second téléchargement du module.
Les quatre contrôles habituels GitHub (Frontend, Backend, Core et Pro) du commit
`a3f4396` passent. La qualification signée est suivie séparément dans le
[run candidat](https://github.com/QoreDB/QoreDB/actions/runs/38152875613).

### Installation et updater sur Ubuntu 22.04

Le `.deb` officiel 0.1.39 s'installe avec ses dépendances dans un conteneur
Ubuntu 22.04 vierge. Les deux outils démarrent avec `--help` et aucune dépendance
partagée ne manque au desktop ou aux outils. La version installée est vérifiée
par `dpkg-query` avant la mise à niveau.

Une seconde instance de conteneur, avec Xvfb et rendu logiciel, ouvre l'AppImage
officielle 0.1.39 sur un profil synthétique. Elle utilise le vrai plugin updater
et un proxy local limité à un manifeste de qualification et son AppImage. Une
autorité TLS jetable est limitée à ce processus ; aucune confiance de l'hôte,
clé publique updater ou URL publiée n'est modifiée.

Le manifeste annonce 0.1.40, mais un premier essai associe volontairement
l'AppImage locale à une signature d'un autre contenu : le téléchargement natif
est refusé avec `The signature verification failed`. La version courante reste
0.1.39 et le SHA-256 de l'AppImage installée reste identique.

Le build Linux du commit `a3f4396` réussit. Les signatures de son AppImage et de
son `.deb` sont vérifiées avec Minisign contre la clé embarquée avant recette.
Le vrai updater télécharge les 161 245 688 octets de l'AppImage 0.1.40, vérifie
sa signature et remplace l'installation. Le fichier installé a exactement le
SHA-256 du candidat signé :
`58ebc186bc418ee5ff5ddcfb05329ba9372e76c386d5eacf6ae182ed2a28f1ce`.

Après fermeture propre et un seul redémarrage dans Xvfb, la version retournée
par Tauri est `0.1.40` et le même manifeste ne repropose aucune mise à jour.
Les 42 assertions natives passent avec le profil conservé, notamment le
trousseau, le coffre, les connexions, les instantanés, les masques et les droits
Core. La fenêtre est ensuite fermée et le conteneur supprimé.

Dans l'autre conteneur Ubuntu, `apt-get install` du `.deb` signé remplace
0.1.39 par 0.1.40. Les deux outils passent `--help`, les trois exécutables
n'ont aucune bibliothèque manquante selon `ldd` et les licences Apache/BSL sont
présentes dans `/usr/lib/QoreDB/licenses`. Le conteneur est arrêté après recette.
Ces contrôles couvrent l'AppImage et le `.deb` Linux x86_64 ; ils ne prouvent pas
une installation graphique Windows/macOS ou Microsoft Store.

Le PKGBUILD est enfin copié dans un répertoire jetable avec version 0.1.40 et
SHA-256 réel du `.deb` signé. `makepkg --nodeps --cleanbuild --force` réussit ;
les trois exécutables et les deux licences du paquet AUR sont identiques aux
fichiers du `.deb`. Ce dernier contrôle porte sur la construction et le contenu,
sans nouvelle installation graphique. Les fichiers AUR suivis restent sur la
version publiée : le workflow AUR les actualise depuis les assets de release.

### Résultat de la matrice signée

Le [run 38152875613](https://github.com/QoreDB/QoreDB/actions/runs/38152875613)
se termine avec succès sur le commit `a3f4396` : Linux x86_64, Windows x86_64,
macOS Intel et Apple Silicon. Sur les deux macOS, `codesign --verify --deep
--strict` passe et `spctl` accepte l'application avec `Notarized Developer ID`.
Le contrôle d'exécution des outils est passé sur les runners correspondant à
leur architecture ; il est explicitement ignoré pour la compilation croisée.

Le job final passe ses trois tests et vérifie les sept paquets updater contre
la clé publique embarquée : deux archives macOS, MSI, NSIS, AppImage, `.deb`
et RPM. Le manifeste 0.1.40 contient les onze clés de plateforme et de format
attendues. Les paquets et le manifeste sont conservés comme artefacts Actions
pendant 14 jours. Leurs URLs de future release ne sont pas encore publiées.

La préparation et la recette Linux sont terminées. Restent non qualifiés :
parcours positifs avec une vraie licence Pro, interfaces/installations
Windows/macOS, Microsoft Store, matrice cloud complète et protocole comparatif
complet de performance native. Aucun tag, merge ou publication n'est effectué.
