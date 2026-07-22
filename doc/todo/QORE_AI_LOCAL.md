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
- [ ] Manifest distant signé pour publier des mises à jour sans nouvelle version de QoreDB.
- [ ] Builds reproductibles de `llama-server` pour toutes les cibles.
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

Le manifest livré avec l'application constitue la source de confiance initiale. La première version épingle `llama.cpp` b10087 pour les six cibles et Qwen 3 8B Q4_K_M à la révision `212c964b8f97cb5edc203d411b767aaae707e653`. Un manifest distant ne pourra le remplacer qu'après ajout d'une signature vérifiée côté Rust.

Le téléchargement utilise HTTP Range lorsqu'un fichier partiel existe. Un serveur qui ne confirme pas la plage demandée provoque un redémarrage propre du fichier au lieu d'une concaténation ambiguë. Un artefact n'est extrait ou déplacé vers son chemin final qu'après vérification de sa taille et de son SHA-256.

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

## Critères de validation

Le benchmark doit couvrir PostgreSQL, MySQL/MariaDB, SQLite, MongoDB, Redis, SQL Server et ClickHouse. Un scénario est réussi si le modèle :

1. sélectionne le bon outil ;
2. produit des arguments conformes au schéma JSON ;
3. exploite le résultat sans inventer de données ;
4. corrige une erreur de requête lorsque c'est possible ;
5. arrête les appels d'outils et rend une réponse finale ;
6. respecte les refus d'écriture et les limites de scope.

La publication stable nécessite également un test d'installation, de reprise de téléchargement, de démarrage, d'arrêt et de mise à niveau sur chaque cible de la matrice.
