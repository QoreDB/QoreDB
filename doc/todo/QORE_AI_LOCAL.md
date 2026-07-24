# Qore AI Local

## Objectif

Fournir le Database Agent et l'assistant de requêtes avec un modèle open-weight exécuté sur la machine de l'utilisateur. QoreDB gère le runtime, le modèle et leur cycle de vie ; aucune installation d'Ollama n'est nécessaire.

Le runtime est `llama-server`. Les poids ne sont pas inclus dans l'installeur QoreDB : ils sont téléchargés séparément après consentement, vérifiés, puis stockés dans le répertoire de données de l'application.

## État d'implémentation

- [x] Provider `QoreLocal` partagé par l'assistant et le Database Agent.
- [x] Streaming et tool calling via l'API OpenAI-compatible de `llama-server`.
- [x] Détection de cible macOS, Windows et Linux sur ARM64 et x86-64.
- [x] Cycle de vie backend : détection, démarrage à la demande, health check, arrêt et nettoyage.
- [x] Port local dynamique et écoute sur `127.0.0.1` uniquement.
- [x] États frontend `unsupported`, `not_installed`, `ready`, `running` et `error`.
- [x] Provider visible dans les paramètres sans pouvoir devenir actif avant installation.
- [x] Manifest embarqué épinglant URL immuable, taille et SHA-256 pour chaque artefact.
- [x] Téléchargement reprenable avec progression, annulation et vérification SHA-256.
- [x] Extraction protégée et remplacement atomique des artefacts vérifiés.
- [x] Manifest distant signé pour publier des mises à jour sans nouvelle version de QoreDB.
- [x] Builds reproductibles de `llama-server` pour toutes les cibles.
- [ ] Sélection automatique du modèle selon la mémoire disponible.
- [ ] Benchmark agentique multi-driver et seuils de publication.

## Matrice de distribution

| OS | Architecture | Runtime initial | Accélération visée |
| --- | --- | --- | --- |
| macOS | arm64 | `llama-server` | Metal |
| macOS | x86-64 | `llama-server` | CPU |
| Windows | x86-64 | `llama-server.exe` | Vulkan, CPU fallback |
| Windows | arm64 | `llama-server.exe` | CPU initialement |
| Linux | x86-64 | `llama-server` | Vulkan, CPU fallback |
| Linux | arm64 | `llama-server` | CPU initialement |

Le runtime est stocké sous `ai-local/runtime/<os>-<architecture>/`. Les modèles sont partagés sous `ai-local/models/` afin qu'une mise à jour du runtime ne retélécharge pas les poids.

## Manifest d'artefacts

Le téléchargement n'accepte aucune URL fournie par le frontend. Le backend lit un manifest curé et embarqué contenant, pour chaque artefact :

- identifiant et version ;
- OS et architecture ;
- URL HTTPS autorisée ;
- taille attendue ;
- SHA-256 ;
- licence et fichier de notices ;
- compatibilité minimale avec le format GGUF et le template de tool calling.

Le manifest livré avec l'application constitue la source de confiance initiale. La première version épingle `llama.cpp` b10087 pour les six cibles et Qwen 3 8B Q4_K_M à la révision `212c964b8f97cb5edc203d411b767aaae707e653`.

Les mises à jour sont publiées sur la prerelease GitHub dédiée `qore-ai-local` sous la forme du JSON byte-exact et de sa signature `.sig`. Le backend vérifie la signature Minisign avec la même racine de confiance que l'auto-updater avant de parser le JSON. Le catalogue distant doit :

- avoir une version strictement supérieure au catalogue embarqué ;
- contenir exactement les six cibles supportées ;
- conserver les chemins relatifs attendus du runtime ;
- fournir uniquement des URL HTTPS, tailles bornées et SHA-256 minuscules ;
- garder une version immuable pour un couple de hashes donné.

Une signature invalide, une réponse trop grande, une cible manquante ou une panne réseau provoque un repli sur le catalogue embarqué. Le registre d'installation bloque ensuite tout downgrade si une version distante plus récente est déjà installée.

Le téléchargement utilise HTTP Range lorsqu'un fichier partiel existe. Un serveur qui ne confirme pas la plage demandée provoque un redémarrage propre du fichier au lieu d'une concaténation ambiguë. Un artefact n'est extrait ou déplacé vers son chemin final qu'après vérification de sa taille et de son SHA-256.

## Builds du runtime

La définition de build `packaging/qore-ai/runtime-build-v1.json` est la source de vérité pour les six cibles. Elle épingle le commit complet de `llama.cpp`, le commit BoringSSL, la date de source, les runners et les options CMake. L'interface web embarquée de `llama-server` est désactivée : QoreDB ne distribue que le serveur, ses bibliothèques d'exécution et les licences.

Le workflow `build-ai-runtime.yml` compile chaque cible deux fois dans des répertoires indépendants, sans cache. Les chemins absolus sont remappés, les métadonnées d'archive sont normalisées et les deux archives doivent avoir le même SHA-256. Un écart arrête la cible et empêche l'assemblage global. La provenance adjacente enregistre la source, la cible, les options et les versions de toolchain réellement observées.

La publication est atomique à l'échelle de la matrice :

1. les six artefacts et leurs provenances doivent être présents ;
2. chaque archive doit contenir `llama-server` au chemin déclaré ;
3. un manifeste candidat est généré avec les URL, tailles et hashes calculés ;
4. l'option manuelle `publish` crée la release immuable `qore-ai-runtime-b10087` et refuse de l'écraser ;
5. le manifeste candidat validé remplace explicitement le catalogue embarqué, puis le workflow de publication le signe et le publie.

Le manifeste embarqué continue de référencer les artefacts upstream tant qu'un candidat complet n'a pas été publié et signé. Les labels de runners hébergés peuvent évoluer ; le double build constitue le verrou bit-à-bit pour la toolchain publiée, tandis que la provenance rend toute dérive de toolchain observable.

## Modèle initial

Le premier profil ciblé est Qwen 3 8B, quantification Q4_K_M, avec l'alias stable `qore-qwen3-8b`. L'artefact GGUF doit être produit ou vérifié par le projet à partir des poids officiels ; QoreDB ne doit pas dépendre silencieusement d'une quantification communautaire mutable.

Un modèle plus petit ne sera proposé qu'après validation agentique. La génération SQL seule ne suffit pas : le modèle doit produire des appels d'outils valides et terminer correctement une boucle multi-tours.

## Sécurité

- Le serveur écoute exclusivement sur loopback et sur un port attribué dynamiquement.
- Le frontend ne lance jamais directement le processus.
- Les chemins du runtime et du modèle sont construits par le backend.
- Le provider local conserve les mêmes permissions, limites et blocages production que les providers cloud.
- Une interruption de téléchargement ne remplace jamais un artefact déjà vérifié ; le fichier partiel est conservé pour la reprise et les écritures finales utilisent un renommage atomique avec rollback.
- Les archives refusent les chemins absolus et les traversées hors du répertoire de staging.
- Les hashes, versions et licences sont enregistrés avec l'installation locale.
- Le manifest distant est vérifié avant désérialisation et ne reçoit aucune URL du frontend.

## Critères de validation

Le benchmark doit couvrir PostgreSQL, MySQL/MariaDB, SQLite, MongoDB, Redis, SQL Server et ClickHouse. Un scénario est réussi si le modèle :

1. sélectionne le bon outil ;
2. produit des arguments conformes au schéma JSON ;
3. exploite le résultat sans inventer de données ;
4. corrige une erreur de requête lorsque c'est possible ;
5. arrête les appels d'outils et rend une réponse finale ;
6. respecte les refus d'écriture et les limites de scope.

La publication stable nécessite également un test d'installation, de reprise de téléchargement, de démarrage, d'arrêt et de mise à niveau sur chaque cible de la matrice.
