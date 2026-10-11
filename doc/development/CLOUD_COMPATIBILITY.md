# Suivi de la compatibilité cloud

QoreDB suit la compatibilité avec les services de bases de données hébergées
sans imposer un compte ou un abonnement pour chaque fournisseur. Les tests
locaux et simulés constituent la couverture régulière ; les essais sur un
service réel complètent cette couverture lorsqu'un environnement est disponible.

## Décision pour la 0.1.40

Décision du 11 octobre 2026 : l'absence de compte, de crédits ou de dataset de
test cloud ne bloque pas la release. La matrice cloud devient un suivi de
compatibilité à compléter à la demande. Aucun abonnement dédié n'est requis
pour remplir cette matrice.

Un scénario non exécuté reste « non vérifié sur le service réel ». Cette
décision ne transforme pas les simulations en validations fournisseur et ne
lève pas un défaut bloquant identifié, notamment une corruption de données ou
un contournement des protections existantes.

## Niveaux de validation

Le niveau et le résultat sont deux informations distinctes. Un test peut réussir
localement tout en laissant le parcours cloud non vérifié.

| Niveau | Ce que la validation établit | Limite |
| --- | --- | --- |
| Moteur local | Comportement observé sur une base réelle locale, pour les scénarios exécutés | Ne valide pas les particularités du fournisseur hébergé |
| Réponses simulées | Traitement par QoreDB des requêtes, réponses et erreurs couvertes par les fixtures | Ne valide pas l'authentification ni le comportement du service réel |
| Service réel | Parcours exécuté sur le fournisseur, avec une configuration et une version de QoreDB identifiées | Ne couvre que les scénarios et le mode d'accès testés |
| Non exécuté | Aucun résultat pour ce scénario et cet environnement | À conserver comme non vérifié, jamais comme réussi |

Un service compatible avec un protocole ne reçoit pas automatiquement le statut
« vérifié sur le service réel » parce que son moteur de substitution passe les
tests. Les [limites des drivers](../tests/DRIVER_LIMITATIONS.md) décrivent les
restrictions connues ; les rapports datés conservent les preuves d'exécution.

## Couverture sans compte cloud

Pour un driver ou un parcours modifié, choisir les tests pertinents dans la
[matrice de tests](TESTING.md) :

- moteur local pour les chemins partagés de connexion, lecture, types et requêtes ;
- tests ciblés de construction de l'authentification, des paramètres et des requêtes ;
- réponses synthétiques pour pagination, conversion des valeurs, expiration,
  refus d'accès, limites de débit, délais et annulation, selon les capacités exposées ;
- régression reproductible pour chaque défaut corrigé, avec données synthétiques.

L'absence de credentials ne doit pas désactiver les tests locaux ou simulés.
Un test d'intégration ignoré faute d'environnement doit être signalé comme tel
dans le bilan, même si la commande globale termine avec succès.

## Quand réaliser un essai sur le service

Déclencher une recette ciblée lorsqu'un compte de test est disponible, qu'un
retour utilisateur identifie un problème propre au fournisseur ou qu'une
évolution du driver nécessite une confirmation réelle. Prioriser les parcours
affectés plutôt que toutes les combinaisons de moteurs et de fonctionnalités.

Utiliser un environnement de test et des données synthétiques. Les preuves
partagées ne contiennent ni credentials, ni identifiants de compte sensibles,
ni lignes provenant d'une base utilisateur. Un retour utilisateur peut fournir
une reproduction et des logs expurgés sans transmettre l'accès à son compte.

## Fiche de suivi

Créer ou actualiser une fiche par service et scénario dans un rapport sous
`doc/tests/`, ou dans l'issue du défaut concerné. Une entrée doit indiquer :

- le service, l'identifiant du driver et le scénario précis ;
- la date, la version ou le commit QoreDB, et l'environnement de test ;
- le mode d'authentification et de transport, sans secrets ;
- le niveau de validation et le résultat : réussi, échoué ou non exécuté ;
- le lien vers le test, le rapport ou l'issue, avec les limites observées ;
- pour un scénario non exécuté, la raison et l'occasion prévue de le vérifier.

Conserver les validations locales et simulées lorsqu'une preuve réelle s'ajoute.
Après une modification du chemin concerné, une ancienne recette reste une
preuve historique ; elle ne valide pas automatiquement la nouvelle version.
Un constat réel de défaut est décrit dans les limites du driver et suivi par
une régression, sans attendre de compléter toute la matrice cloud.
