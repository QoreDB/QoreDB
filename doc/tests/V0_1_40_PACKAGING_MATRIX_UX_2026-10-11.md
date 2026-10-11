# v0.1.40 — Packaging Linux, fidélité de la fédération et clavier, 11 octobre 2026

## Périmètre

Départ : `8d163ee359dfe12ecd660c495101d576854f2fbe`, avec seulement
`HANDOFF.md` non suivi. Les changements décrits sont locaux. Aucun bump, tag,
publication, activation de licence ou changement du profil personnel.

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
vierge ni une installation Debian. L'installation `.deb` sur Debian/Ubuntu,
les signatures de production et l'auto-updater réel restent non exécutés.
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
