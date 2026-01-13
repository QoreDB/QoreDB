# **QoreDB — Backlog produit (POC)**

## 🧪 **POC — “Ça marche, je peux l’utiliser”**

> Objectif : remplacer DBeaver/phpMyAdmin pour 20–30 % de leur usage réel.

### 🧱 Data Engine Kernel

- [x] **Interface DataEngine** — Définir une API commune pour toutes les bases
- [x] **Driver PostgreSQL** — Implémentation du kernel
- [x] **Driver MySQL** — Implémentation du kernel
- [x] **Driver MongoDB** — Implémentation NoSQL du kernel
- [x] **Registry de drivers** — Système de plugins internes
- [x] **Mapping universel**
  - namespace (db / schema / bucket)
  - collection (table / collection)
  - record
- [x] **Normalisation des erreurs**
- [x] **Normalisation des résultats (cursor / rows / json)**


### 🔌 Connexions

* [x] **PostgreSQL** — Connexion et exécution de requêtes
* [x] **MySQL / MariaDB** — Connexion et exécution de requêtes
* [x] **MongoDB** — Connexion et requêtes NoSQL
* [x] **Connexions multiples** — Gérer plusieurs bases en parallèle
* [ ] **Test de connexion** — Vérifier avant d’enregistrer
* [x] **SSL / TLS** — Connexions sécurisées
* [x] **SSH Tunnel** — Accès aux bases privées

### 🔐 Sécurité locale

* [x] **Coffre chiffré** — Stocker les credentials localement de façon sûre
* [x] **Jamais en clair** — Aucun mot de passe accessible depuis l’UI
* [x] **Isolation par projet** — Une base ≠ une autre
* [x] **Verrouillage au démarrage** — Protéger l’app quand elle s’ouvre

### 🧭 Interface

* [x] **Sidebar connexions** — Liste claire des bases
* [x] **Arbre DB** — Bases → schémas → tables / collections
* [x] **Onglets** — Plusieurs requêtes ouvertes
* [x] **Dark mode** — Lisible de nuit
* [x] **Recherche globale** — Trouver tables / collections rapidement

### ✍️ SQL

* [x] **Éditeur SQL** — Écrire et exécuter
* [x] **Exécution par sélection** — Lancer une partie du script
* [x] **Résultats tabulaires** — Voir les données
* [x] **Scroll virtuel** — Gros datasets sans lag
* [x] **Annulation** — Stopper une requête longue

### 🍃 NoSQL

* [x] **Requêtes Mongo** — find(), aggregate(), etc.
* [x] **Navigation collections** — Explorer la base
* [x] **Aperçu JSON** — Voir les documents

### 📊 Data grid

* [x] **Affichage performant** — Pas de freeze (TanStack Table + Virtual Scroll)
* [x] **Copy / paste** — Vers Excel, code, etc.
* [x] **Sélection multiple**
* [x] **Tri simple**
* [x] **Colonnes auto-size** (via TanStack Table)

### 🧰 Qualité de vie

* [ ] **Historique des requêtes**
* [ ] **Favoris**
* [ ] **Sessions sauvegardées**
* [ ] **Logs d’erreurs**

