# QoreDB — Product Hunt Launch Checklist

---

## 1. Avant le launch (2-3 semaines avant)

### Produit

- [ ] Faire un build release stable (macOS, Windows, Linux)
- [ ] Tester le flow complet : install → première connexion → query → export sur chaque OS
- [ ] Vérifier l'auto-updater (le build PH doit pouvoir se mettre à jour proprement)
- [ ] Fixer les bugs critiques restants (tester avec PostgreSQL, MySQL, MongoDB en priorité)
- [ ] S'assurer que l'onboarding première utilisation est fluide (pas de crash, pas de confusion)
- [ ] Vérifier que le vault fonctionne au premier lancement (création master password)

### Site web / Landing page

- [ ] Héberger la landing page (Vercel, Netlify, ou domaine custom qoredb.com)
- [ ] Remplacer tous les `#DOWNLOAD_LINK` et `#GITHUB_LINK` par les vraies URLs
- [ ] Ajouter les meta tags Open Graph avec une image de preview (1200x630px)
- [ ] Tester le rendu mobile de la landing page
- [ ] Mettre en place un lien de téléchargement direct (pas juste GitHub releases)
- [ ] Optionnel : ajouter un formulaire email pour la newsletter / updates

### Assets visuels

- [ ] Préparer le logo QoreDB en 240x240px (icône PH)
- [ ] Préparer une bannière 1270x760px pour la gallery Product Hunt
- [ ] Préparer 3-5 screenshots annotés de l'app (dark mode recommandé) :
  - Screenshot 1 : Vue globale (sidebar + éditeur SQL + résultats)
  - Screenshot 2 : Inline editing dans le data grid
  - Screenshot 3 : ER Diagram interactif
  - Screenshot 4 : Global search / Cmd+K
  - Screenshot 5 : Connection setup avec environnement prod (couleur rouge)
- [ ] Vérifier que la vidéo promo est prête (format : MP4, < 3 min, idéalement 60-90s)
- [ ] Optionnel : créer un GIF animé court (15-30s) du flow principal

### Textes Product Hunt

- [ ] Relire et valider la tagline (fichier `product-hunt-texts.md`)
- [ ] Relire et valider la description courte (260 chars)
- [ ] Relire et valider la description longue
- [ ] Relire et personnaliser le commentaire maker (ajouter des détails perso si besoin)
- [ ] Préparer 3-5 réponses types pour les commentaires courants :
  - "What databases do you plan to add next?"
  - "How does this compare to X?"
  - "What's the pricing model?"
  - "Is there a web/cloud version?"
  - "How do you handle security?"

### GitHub

- [ ] Mettre à jour le README.md avec screenshots, badges, et lien PH
- [ ] Ajouter un badge "Featured on Product Hunt" (après le launch)
- [ ] S'assurer que le CONTRIBUTING.md existe
- [ ] Vérifier que les issues sont bien labellisées (bonne première impression)
- [ ] Créer un tag/release GitHub avec des release notes claires

### Communauté (pré-launch)

- [ ] Prévenir tes contacts devs / indie hackers du launch (date exacte)
- [ ] Poster un teaser sur Twitter/X 3-5 jours avant
- [ ] Optionnel : poster un teaser sur r/selfhosted, r/programming, r/devtools
- [ ] Préparer un post Hacker News (Show HN) pour le jour J ou J+1
- [ ] Optionnel : poster sur IndieHackers, Dev.to, ou un blog perso

---

## 2. Le jour du launch (J-Day)

### Timing

- [ ] **Publier à 00:01 PST** (9h01 heure de Paris) — c'est le reset PH quotidien
- [ ] Être disponible toute la journée pour répondre aux commentaires

### Actions immédiates

- [ ] Poster le premier commentaire maker (depuis `product-hunt-texts.md`)
- [ ] Partager le lien PH sur Twitter/X avec un thread explicatif
- [ ] Envoyer le lien PH à tes contacts proches (demander des upvotes sincères, pas du spam)
- [ ] Poster sur les communautés prévues (HN, Reddit, Discord, Slack devs)
- [ ] Mettre à jour le site web avec un bandeau "Live on Product Hunt"

### Pendant la journée

- [ ] Répondre à CHAQUE commentaire sur PH (dans l'heure si possible)
- [ ] Être authentique et transparent — les makers qui répondent bien rankent mieux
- [ ] Partager les retours intéressants sur Twitter en temps réel
- [ ] Surveiller les downloads / GitHub stars / crash reports
- [ ] Si un bug est remonté : le fixer et déployer un patch rapidement

---

## 3. Après le launch (semaine suivante)

### Capitaliser

- [ ] Ajouter le badge PH sur le site et le README
- [ ] Écrire un post-mortem / retour d'expérience (blog, Twitter thread, ou IndieHackers)
- [ ] Remercier publiquement les supporters
- [ ] Compiler tous les feedbacks PH dans une issue GitHub ou un doc interne

### Suivi produit

- [ ] Prioriser les feature requests les plus demandées
- [ ] Fixer les bugs remontés le jour du launch
- [ ] Surveiller les métriques : downloads, rétention (si telemetry activée), GitHub stars
- [ ] Planifier la prochaine release avec les améliorations issues du feedback

### Presse & visibilité

- [ ] Contacter 2-3 newsletters dev (TLDR, Console.dev, Changelog) avec les résultats PH
- [ ] Optionnel : écrire un article technique (architecture Rust/Tauri, ou retour sur le dev solo)
- [ ] Partager sur LinkedIn pour la visibilité pro

---

## 4. Métriques de succès

| Métrique | Objectif optimiste | Objectif réaliste |
|----------|-------------------|-------------------|
| Upvotes PH (jour 1) | Top 5 du jour | Top 10 du jour |
| Commentaires PH | 30+ | 15+ |
| Downloads (semaine 1) | 2 000+ | 500+ |
| GitHub stars (semaine 1) | 1 000+ | 300+ |
| Bugs critiques remontés | 0 | < 3 |

---

## 5. Rappels importants

**Ne pas faire :**
- Ne pas demander des upvotes en masse à des inconnus (PH détecte et pénalise)
- Ne pas poster sur PH un lundi ou vendredi (moins de trafic)
- Ne pas lancer sans avoir testé le download + install sur les 3 OS
- Ne pas ignorer les commentaires négatifs (répondre avec classe)

**Le meilleur jour pour lancer :**
- **Mardi ou mercredi** — trafic PH maximal
- Éviter les jours où un gros launch est prévu (vérifier le calendrier PH)

**Le facteur #1 de succès sur PH :**
- La qualité du produit + l'engagement du maker dans les commentaires. Le reste est secondaire.
