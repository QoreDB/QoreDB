| Problème | Priorité | Zone sur l’app | Correctif proposé | Done |
| :--- | :--- | :--- | :--- | :---: |
| Stratification verticale floue (menu OS / top bar / header vue) | Haute | Zone haute globale | Renforcer la séparation visuelle entre couches par des rôles explicites : fonds distincts, séparateurs plus marqués, padding différencié par niveau. | [x] |
| Centre de gravité visuel indéfini dans la top bar | Haute | Top bar | Désigner un élément principal (navigation ou recherche) et réduire la dominance des autres ; éviter l'égalité visuelle entre actions critiques et utilitaires. | [x] |
| Champ "Rechercher ou exécuter…" au rôle ambigu | Haute | Top bar | Clarifier son statut par micro-copy, icône dédiée, ou comportement visuel distinct (ex. ouverture explicite de palette vs input passif). | [x] |
| Mélange navigation / action / info dans la top bar | Haute | Top bar | Séparer clairement : navigation (onglets), information de contexte (DB/env), actions globales (icônes), sans les aligner sur un même plan visuel. | [x] |
| Onglets mélangeant vues, documents et actions | Haute | Navigation horizontale | Scinder visuellement ou conceptuellement : onglets de contenu vs bouton d'action "+" hors du groupe. | [~] |
| Modèle mental des onglets ambigu | Moyenne | Navigation horizontale | Rendre explicite la nature des onglets (vue persistante vs document temporaire) via style ou libellé. | [x] |
| Contexte actif (DB, moteur, env) sous-exprimé | Haute | Header de vue / top bar | Donner une zone dédiée, stable, avec hiérarchie claire et lecture rapide, distincte des onglets et actions. | [x] |
| États critiques (DEV/PROD, bac à sable, lecture seule) trop subtils | Haute | Header, status bar | Accentuer la saillance visuelle (couleur, badge, label explicite) pour éviter toute ambiguïté opérationnelle. | [x] |
| Dispersion des indicateurs d'état (session, env, mode) | Moyenne | Global | Regrouper les états critiques dans un même "cluster d'état" cohérent spatialement. | [x] |
| Redondance des accès aux paramètres | Haute | Top bar / zones secondaires | Désigner un point d'entrée principal ; différencier visuellement ou supprimer les doublons non justifiés. | [x] |
| Icônes globales en concurrence avec le chrome OS | Moyenne | Coin haut droit | Isoler visuellement les actions applicatives (groupe, fond, alignement) des contrôles fenêtre natifs. | [x] |
| Sur-exposition de l'identité produit (logo + nom + version) | Moyenne | Bandeau haut gauche | Réduire la présence persistante ; réserver la version à un écran "À propos" ou menu secondaire. | [x] |
| Informations à faible valeur instantanée toujours visibles | Moyenne | Zone haute | Déplacer ces informations vers des zones contextuelles ou accessibles à la demande. | [x] |
| Uniformité excessive des surfaces (light & dark) | Moyenne | Global | Introduire des niveaux de surface plus contrastés pour matérialiser la hiérarchie fonctionnelle. | [x] |
| Contrastes faibles pour éléments secondaires | Moyenne | Tables, toolbars | Augmenter légèrement contraste ou taille pour améliorer la découvrabilité sans surcharger. | [x] |
| Icônes seules sans label sur actions sensibles | Haute | Panneau modifications, toolbars | Ajouter labels, tooltips persistants ou groupements explicites pour éviter erreurs. | [x] |
| Sémantique visuelle ambiguë dans le table viewer | Moyenne | Table viewer | Différencier clairement sélection, édition, mode spécial par codes visuels distincts et cohérents. | [x] |
| Multiplicité des patterns d'édition | Moyenne | Table viewer | Normaliser le pattern principal (ligne vs cellule) et réduire les variantes concurrentes. | [~] |
| Panneau "modifications en attente" coûteux spatialement | Moyenne | Panneau latéral droit | Rendre le panneau collapsable ou contextuel selon l'état (aucune / quelques / nombreuses modifs). | [x] |
| Distinction appliqué / en attente / destructif peu claire | Haute | Modifications en attente | Hiérarchiser visuellement les états et actions (couleur, iconographie, groupement). | [x] |
| Schéma ERD peu robuste à grande échelle | Moyenne | Vue schéma | Renforcer outils de filtrage, focalisation et lecture relationnelle pour limiter l'effet "spaghetti". | [~] |
| Sémantique des compteurs/icônes ERD implicite | Basse | Vue schéma | Clarifier par légende, tooltip ou micro-label. | [~] |
| Sidebar non éprouvée sous forte densité | Moyenne | Sidebar | Prévoir comportements clairs : repli, regroupement, scroll explicite, hiérarchie visuelle renforcée. | [~] |
| États actifs en sidebar peu hiérarchisés | Moyenne | Sidebar | Accentuer la distinction entre actif, ouvert, sélectionné et simplement listé. | [x] |
| Empty states trop vides | Basse | Home / écrans système | Ajouter guidance minimale contextuelle sans transformer en onboarding intrusif. | [~] |
| Overlays concurrençant le message principal | Basse | Home | Ajuster taille/position pour qu'ils n'entrent pas en compétition avec l'action principale. | [~] |
| Dissonance conventions clavier (Cmd vs Ctrl) | Basse | UI globale | Adapter dynamiquement les libellés selon l'OS détecté. | [x] |
| Responsabilité UI floue de certaines infos/actions | Haute | Global | Attribuer explicitement chaque information/action à une zone et un niveau hiérarchique précis. | [x] | 