# Audit de Conformité RGPD - QoreDB

## Résumé Exécutif

L'audit du projet **QoreDB** révèle une architecture **"Local-First"** (Priorité au Local) qui respecte fondamentalement les principes du RGPD (Règlement Général sur la Protection des Données). L'application minimise la collecte de données, stocke les informations sensibles localement de manière sécurisée et adopte une approche **Opt-In** (consentement préalable) pour la télémétrie.

**Niveau de Conformité : ÉLEVÉ**

---

## 1. Principes de Protection des Données (Privacy by Design)

### 1.1 Minimisation des Données

- **Architecture :** L'application fonctionne localement. Les bases de données des utilisateurs ne transitent jamais par des serveurs tiers gérés par QoreDB.
- **Télémétrie :** Les données collectées se limitent à des compteurs d'événements (ex: "app_opened", "query_executed") sans inclure le contenu sensible (ex: le texte des requêtes SQL est exclu).

### 1.2 Limitation du Stockage

- **Logs Applicatifs :** Les journaux sont stockés localement sur la machine de l'utilisateur (`~/.qoredb/logs` ou `%APPDATA%\QoreDB\logs`).
- **Rotation :** Une politique de rétention automatique de **14 jours** est implémentée dans le code (`src-tauri/src/observability.rs`). Les anciens logs sont supprimés automatiquement.

### 1.3 Intégrité et Confidentialité

- **Identifiants :** Les mots de passe et clés de connexion aux bases de données sont stockés dans le coffre-fort sécurisé du système d'exploitation (Keychain sur macOS, Credential Manager sur Windows, Secret Service sur Linux) via la bibliothèque `keyring`. Ils ne sont pas stockés en clair dans des fichiers de configuration.
- **Rédaction des Logs :** Le code utilise un wrapper `Sensitive<T>` (`src-tauri/src/observability/sensitive.rs`) qui remplace automatiquement les données sensibles par `[REDACTED]` ou `***` lors de la sérialisation ou de l'affichage dans les logs.

---

## 2. Analyse des Flux de Données (Data Flow)

### 2.1 Télémétrie (PostHog)

- **Service :** PostHog (Configuration par défaut sur serveurs UE : `eu.i.posthog.com`).
- **Consentement :** **Opt-In par défaut**. Le service d'analyse (`AnalyticsService.ts`) vérifie explicitement si l'utilisateur a activé l'option (`analytics_enabled`) avant d'initialiser le SDK ou d'envoyer des événements.
- **Données envoyées :**
  - Événements d'usage (ouverture app, succès/échec connexion).
  - Type de driver utilisé (Postgres, MySQL, etc.).
  - Nombre de lignes retournées (mais PAS les données elles-mêmes).
- **Droit à l'oubli :** La fonction `resetIdentity()` est appelée lorsque l'utilisateur désactive l'analyse.

### 2.2 Mises à jour (Auto-Updater)

- **Service :** Tauri Plugin Updater.
- **Flux :** L'application interroge les "Releases" GitHub (`https://github.com/raphplt/QoreDB/releases`).
- **Implication RGPD :** L'adresse IP de l'utilisateur est visible par GitHub (Microsoft) lors de la vérification des mises à jour. C'est un fonctionnement standard pour les applications Desktop open-source.

### 2.3 Rapports de Crash

- Aucun service tiers de rapport de crash (ex: Sentry) n'a été détecté.
- Les "Panics" (crashs Rust) sont capturés et écrits dans les logs locaux.

---

## 3. Recommandations

Bien que le projet soit très conforme, voici quelques points d'attention pour maintenir ce niveau :

1.  **Bannière de Consentement :** S'assurer que l'interface utilisateur (Onboarding) demande clairement le consentement pour l'activation des statistiques anonymes (ce qui semble être le cas via `OnboardingModal.tsx`).
2.  **Documentation :** Ajouter une section "Confidentialité" dans le `README.md` ou l'application expliquant que les données restent locales.
3.  **Vérification des Logs :** Lors des futures développements, continuer d'utiliser systématiquement `Sensitive<T>` pour tout nouveau champ contenant des données utilisateur.

---

_Audit généré automatiquement le 21 Février 2025._
