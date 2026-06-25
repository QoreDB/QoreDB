# QoreDB — Fiche produit (v0.1)

## 1. Vision

QoreDB est un client de bases de données desktop, local-first, conçu pour les
développeurs modernes qui travaillent avec des bases SQL et NoSQL et qui en ont
assez des outils lents, lourds et mal conçus.

L'objectif : faire pour les bases de données ce que Linear, Raycast ou VS Code
ont fait pour leurs domaines. Un outil rapide, clair, agréable, puissant et sûr,
sans devenir une usine à gaz.

## 2. Cible principale

QoreDB s'adresse en priorité aux startups early-stage (2–10 devs), aux PME tech
(10–50 devs) et aux développeurs solo (freelance, indie, side projects).

Il ne vise pas au départ les équipes data (BI, analysts), les DBA enterprise ni
les grandes entreprises régulées — ces segments pourront être abordés plus tard
(V3+). Le cœur de QoreDB reste le développeur produit qui écrit du code, gère sa
propre base et veut un outil qui ne le ralentit pas.

## 3. Problème principal à résoudre

Les outils actuels (DBeaver, phpMyAdmin, pgAdmin, etc.) font le job mais sont
perçus comme lents, lourds et fatigants au quotidien : UX médiocre, workflows
mal optimisés, et un design qui n'a pas évolué avec la façon moderne de
travailler.

QoreDB ne cherche pas à les battre sur la quantité de features, mais sur la
qualité de l'expérience.

## 4. Proposition de valeur

Un outil unique pour gérer SQL et NoSQL, avec une interface moderne, rapide et
agréable, augmentée par une intelligence contextuelle :

- une interface claire pour explorer, interroger et modifier des bases ;
- un moteur rapide capable de gérer de gros volumes sans ramer ;
- une expérience cohérente entre SQL et NoSQL ;
- un assistant intelligent qui comprend le contexte.

L'utilisateur doit sentir que QoreDB travaille avec lui, pas contre lui.

## 5. Positionnement technique

QoreDB est une application desktop, local-first et offline-capable, installée sur
la machine du développeur. Ce n'est pas un SaaS web, et l'outil n'envoie pas les
données dans le cloud par défaut. La collaboration et les services distants sont
optionnels, jamais obligatoires.

## 6. SQL + NoSQL comme fondation

QoreDB est conçu dès le départ pour PostgreSQL, MySQL / MariaDB, MongoDB, Redis,
et d'autres moteurs au fil des versions. L'objectif n'est pas seulement de les
supporter, mais d'offrir une expérience unifiée et fluide entre ces mondes —
pas un outil SQL et un outil NoSQL collés, mais une vraie plateforme de données
développeur.

## 7. IA : assistant global

L'IA dans QoreDB n'est pas un gadget : c'est un assistant qui comprend la base,
le schéma et les habitudes de l'utilisateur. Elle aide à écrire des requêtes,
expliquer des résultats, naviguer, détecter des incohérences et suggérer des
optimisations. Elle doit rester contextuelle, respectueuse de la confidentialité,
et progressivement plus utile avec le temps.

## 8. Collaboration sans cloud obligatoire

QoreDB doit permettre de partager des requêtes et des résultats et de collaborer
sur une base, sans imposer de compte cloud, d'upload des données ni de SaaS
centralisé. La collaboration peut passer par des serveurs auto-hébergés, du
peer-to-peer, ou un cloud optionnel.

## 9. Open source & modèle économique

QoreDB est un projet open source : cœur et application ouverts, services premium
optionnels (hébergement, sync, features avancées). L'idée : construire d'abord un
excellent produit open source, puis monétiser les usages avancés.

## 10. Identité produit

QoreDB doit être perçu comme moderne, propre, rapide, sérieux et agréable — un
outil qu'on a envie d'ouvrir tous les jours.
