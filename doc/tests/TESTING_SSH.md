# Tester le tunnel SSH (local, Windows)

QoreDB supporte le tunneling SSH en lançant le client OpenSSH natif (`ssh.exe`).

## 1) Démarrer l'infra locale (Docker)

### 1.1 Créer `authorized_keys` (non commité)

- Créer le dossier : `dev\ssh\`
- Mettre votre clé publique dans : `dev\ssh\authorized_keys`

Exemple (PowerShell) :

- Générer une clé (si besoin) : `ssh-keygen -t ed25519 -f $env:USERPROFILE\.ssh\qoredb_dev`
- Créer le dossier : `New-Item -ItemType Directory -Force dev\ssh | Out-Null`
- Écrire la clé publique dans `authorized_keys` :
  `Get-Content $env:USERPROFILE\.ssh\qoredb_dev.pub | Set-Content dev\ssh\authorized_keys`

### 1.2 Lancer les conteneurs

- `docker compose up -d`

Note : l'image `linuxserver/openssh-server` désactive le port-forwarding par défaut (`AllowTcpForwarding no`).
Pour que QoreDB puisse faire un tunnel `ssh -L`, on monte un override `sshd_config.d` :

- Fichier : `dev\\ssh\\sshd_config.d\\10-qoredb-forwarding.conf`
- Monté dans le conteneur : `/config/sshd/sshd_config.d/10-qoredb-forwarding.conf`

Ça démarre :

- `ssh-bastion` sur `localhost:2222` (utilisateur `qoredb`)
- `postgres` sur `localhost:54321`
- `mysql` sur `localhost:3306`
- `mongodb` sur `localhost:27017`

## 2) Tester dans QoreDB (avec tunnel SSH)

Exemple PostgreSQL **via SSH** :

Base de données :

- Hôte DB : `postgres`
- Port DB : `5432`
- Utilisateur/mot de passe : `qoredb` / `qoredb_test`
- Nom de base : `testdb`

Tunnel SSH :

- Activer le tunnel SSH
- Hôte SSH : `127.0.0.1`
- Port SSH : `2222`
- Utilisateur SSH : `qoredb`
- Chemin de clé privée : par ex. `C:\\Users\\<vous>\\.ssh\\qoredb_dev`
- Politique de host key : commencer avec `accept_new`
- ProxyJump : laisser vide (inutile en local)

Pourquoi l'hôte DB est `postgres` (et pas `localhost`) :

- Avec `ssh -L local:remote_host:remote_port`, le `remote_host` est résolu côté serveur SSH.
- Dans Docker, le bastion résout `postgres` (nom de service).

## 3) Cas négatifs (à vérifier)

- Mettre `strict` dès la première connexion : doit échouer tant que l'hôte n'est pas “trusted”.
- Mettre un mauvais chemin de clé : doit échouer vite, avec le stderr SSH dans l'erreur.
- Stopper le conteneur `postgres` : le tunnel peut monter, mais la connexion DB doit échouer.

## 4) Option VPS (plus réaliste)

Si tu veux simuler un setup proche prod :

- Faire tourner la DB sur une interface privée et/ou la firewall.
- Exposer uniquement SSH.
- Dans QoreDB, mettre un host DB joignable depuis le VPS (souvent `127.0.0.1`).
