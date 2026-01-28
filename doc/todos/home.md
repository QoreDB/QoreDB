# Écran d’accueil QoreDB — Spécification fonctionnelle

## Principe directeur
L’écran d’accueil doit avoir **deux états distincts**, exclusifs, déterminés par la présence ou non de connexions existantes.  
Il ne s’agit pas de deux écrans séparés dans la navigation, mais de **deux affichages conditionnels** du même écran.

---

## État A — Aucun connexion existante (first run réel)

### Objectif
Permettre à un utilisateur **nouveau** de créer sa première connexion sans ambiguïté.

### Structure de l’écran
- Zone centrale unique, focalisée
- Sidebar présente mais vide (ou minimaliste)
- Aucun bandeau de restauration de session

### Contenu central
- Logo QoreDB (présence modérée, non dominante)
- Titre fonctionnel  
  **« Aucune connexion configurée »**
- Texte descriptif court et factuel  
  « Créez votre première connexion pour commencer à travailler avec vos bases de données. »
- Action principale (unique, dominante)  
  **Bouton : “Créer une connexion”**
- Action secondaire (discrète)  
  Recherche / actions rapides (optionnel, dépriorisé)

### Règles UX
- Aucun wording de type “Bienvenue”.
- Une seule action mise en avant.
- Densité visuelle modérée mais orientée action.
- Aucune redondance avec la sidebar.

---

## État B — Connexions existantes (cas majoritaire)

### Objectif
Permettre à un utilisateur **récurrent** de reprendre rapidement son travail ou d’initier une nouvelle session.

### Structure de l’écran
- Sidebar pleinement active (connexions visibles)
- Contenu central aligné sur l’état réel du système
- Priorité visuelle à la continuité de travail

### Bloc principal — Reprise de session (si applicable)
Affiché uniquement si une session précédente est détectée.

- Titre explicite  
  **« Reprendre la session précédente »**
- Description factuelle  
  « Une session inachevée a été trouvée pour : Pulse »
- Actions :
  - Action primaire : **Restaurer la session**
  - Action secondaire : **Ignorer**

Ce bloc est :
- Centré ou top-centre
- L’élément le plus saillant de l’écran

### Contenu secondaire (hors restauration)
- Titre fonctionnel  
  **« Aucune session active »**
- Actions disponibles :
  - Sélectionner une connexion existante depuis la sidebar
  - Action secondaire : **Nouvelle connexion** (discrète, non centrale)
  - Recherche globale / actions rapides (visuellement secondaire)

### Règles UX
- Aucune sémantique d’accueil ou d’onboarding.
- Le centre de l’écran ne doit jamais contredire la sidebar.
- Une hiérarchie stricte :
  1. Reprendre
  2. Continuer autrement
  3. Créer du nouveau

---

## Éléments communs aux deux états

### À supprimer
- Titre « Bienvenue sur QoreDB »
- Texte générique non contextuel
- Redondance des CTA “Nouvelle connexion” entre centre et sidebar

### À conserver
- Top bar globale (recherche rapide, menu OS)
- Sidebar (adaptée à l’état)
- Status bar (état système réel)

### Règles globales
- Aucun écran ne doit paraître “vide” si des données existent.
- Le langage doit être :
  - factuel
  - orienté action
  - non marketing
- L’écran d’accueil est un **état de transition**, jamais une vitrine.

---

## Résultat attendu

- Lecture immédiate de l’état de l’application
- Décision utilisateur évidente, sans arbitrage inutile
- Continuité mentale entre ouverture de l’app et reprise de travail
- Suppression de toute ambiguïté entre découverte et usage expert