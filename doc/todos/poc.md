# **QoreDB — Backlog produit (POC)**

## 🧪 **POC — “Ça marche, je peux l’utiliser”**

> Objectif : remplacer DBeaver/phpMyAdmin pour 20–30 % de leur usage réel.

### 🔌 Connexions

* [ ] **PostgreSQL** — Connexion et exécution de requêtes
* [ ] **MySQL / MariaDB** — Connexion et exécution de requêtes
* [ ] **MongoDB** — Connexion et requêtes NoSQL
* [ ] **Connexions multiples** — Gérer plusieurs bases en parallèle
* [ ] **Test de connexion** — Vérifier avant d’enregistrer
* [ ] **SSL / TLS** — Connexions sécurisées
* [ ] **SSH Tunnel** — Accès aux bases privées

### 🔐 Sécurité locale

* [ ] **Coffre chiffré** — Stocker les credentials localement de façon sûre
* [ ] **Jamais en clair** — Aucun mot de passe accessible depuis l’UI
* [ ] **Isolation par projet** — Une base ≠ une autre
* [ ] **Verrouillage au démarrage** — Protéger l’app quand elle s’ouvre

### 🧭 Interface

* [ ] **Sidebar connexions** — Liste claire des bases
* [ ] **Arbre DB** — Bases → schémas → tables / collections
* [ ] **Onglets** — Plusieurs requêtes ouvertes
* [ ] **Dark mode** — Lisible de nuit
* [ ] **Recherche globale** — Trouver tables / collections rapidement

### ✍️ SQL

* [ ] **Éditeur SQL** — Écrire et exécuter
* [ ] **Exécution par sélection** — Lancer une partie du script
* [ ] **Résultats tabulaires** — Voir les données
* [ ] **Scroll virtuel** — Gros datasets sans lag
* [ ] **Annulation** — Stopper une requête longue

### 🍃 NoSQL

* [ ] **Requêtes Mongo** — find(), aggregate(), etc.
* [ ] **Navigation collections** — Explorer la base
* [ ] **Aperçu JSON** — Voir les documents

### 📊 Data grid

* [ ] **Affichage performant** — Pas de freeze
* [ ] **Copy / paste** — Vers Excel, code, etc.
* [ ] **Sélection multiple**
* [ ] **Tri simple**
* [ ] **Colonnes auto-size**

### 🧰 Qualité de vie

* [ ] **Historique des requêtes**
* [ ] **Favoris**
* [ ] **Sessions sauvegardées**
* [ ] **Logs d’erreurs**

### 🧱 Data Engine Kernel

- [ ] **Interface DataEngine** — Définir une API commune pour toutes les bases
- [ ] **Driver PostgreSQL** — Implémentation du kernel
- [ ] **Driver MySQL** — Implémentation du kernel
- [ ] **Driver MongoDB** — Implémentation NoSQL du kernel
- [ ] **Registry de drivers** — Système de plugins internes
- [ ] **Mapping universel**
  - namespace (db / schema / bucket)
  - collection (table / collection)
  - record
- [ ] **Normalisation des erreurs**
- [ ] **Normalisation des résultats (cursor / rows / json)**
