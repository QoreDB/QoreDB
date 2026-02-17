# Implémentation côté app QoreDB: licence Pro (spec précise)

## 1. Objectif

Ce document décrit exactement ce qu'il reste à implémenter dans l'application desktop QoreDB (Tauri/React/Rust) pour exploiter le système de licence Pro déjà en place côté site showcase.

## 2. Ce qui existe déjà côté showcase

Les briques backend/web suivantes sont déjà implémentées dans `QoreDB-showcase`.

1. Achat Pro Stripe (paiement unique)

- `POST /api/checkout`
- mode `payment`
- `line_items` basés sur `STRIPE_PRICE_ID`

2. Webhook Stripe

- `POST /api/webhooks/stripe`
- événements gérés:
  - `checkout.session.completed`
  - `payment_intent.payment_failed`
  - `charge.refunded`
- génération de la licence au `checkout.session.completed`
- stockage de la licence dans les metadata du PaymentIntent Stripe

3. Pages client

- `/{locale}/pricing`
- `/{locale}/purchase/success?session_id=...` (affiche la clé)
- `/{locale}/license` (status / resend)

4. API support licence

- `POST /api/license/status`
- `POST /api/license/resend`

5. Crypto licence

- `lib/license/generate.ts`
- signature Ed25519 via `@noble/ed25519`

## 3. Contrat exact de la clé de licence

La clé de licence fournie à l'utilisateur est:

1. `base64(envelope_json)`

2. `envelope_json` est:

```json
{
  "payload": {
    "email": "user@example.com",
    "tier": "pro",
    "issued_at": "2026-02-17T13:00:00.000Z",
    "expires_at": null,
    "payment_id": "pi_xxx"
  },
  "signature": "base64_ed25519_signature"
}
```

3. La signature est calculée sur:

- `UTF-8(JSON.stringify(payload))`

## 4. Clé publique côté app

L'app QoreDB doit embarquer la clé publique Ed25519 correspondant à la clé privée du showcase.

Format conseillé:

- `PUBLIC_KEY_BASE64` dans une constante compile-time de l'app (ou config embarquée signée)

Règle:

- jamais embarquer la clé privée

## 5. Vérification offline à implémenter dans l'app

## 5.1 Pipeline de validation

1. Lire la chaîne saisie utilisateur
2. `base64 decode`
3. Parser JSON -> `envelope`
4. Valider le schéma minimum

- `payload.email`: string non vide
- `payload.tier`: doit valoir `"pro"`
- `payload.issued_at`: date ISO valide
- `payload.expires_at`: `null` ou date ISO
- `payload.payment_id`: string non vide
- `signature`: string base64 valide

5. Recalculer le message signé

- `JSON.stringify(payload)`

6. Vérifier la signature Ed25519 avec la clé publique embarquée
7. Refuser la licence si signature invalide
8. Si `expires_at != null`, refuser si expirée
9. Si OK, activer Pro

## 5.2 Critères de robustesse

1. Validation purement offline

- aucun appel réseau requis pour activer

2. Tolérance UX

- message utilisateur explicite selon l'erreur
- ne jamais crasher l'app sur input invalide

3. Idempotence

- recoller une clé déjà active ne doit pas casser l'état

## 6. Stockage local de la licence dans l'app

Implémentation recommandée:

1. Stocker la clé brute + payload validé dans le vault local existant (ou équivalent chiffré)
2. Revalider la signature au démarrage de l'app
3. Ne jamais stocker un booléen `isPro=true` sans preuve (clé)
4. Source de vérité: la clé signée

Données à conserver localement:

- `license_key` (string)
- `validated_payload` (cache, facultatif)
- `validated_at` (audit local, facultatif)

## 7. Gating des features Pro

Créer un service central de licensing, par exemple:

- `LicenseService.getStatus(): core | pro_active | pro_invalid | pro_expired`

Utiliser ce service dans toutes les features Pro:

1. Sandbox avancé
2. Visual Diff
3. ER Diagram avancé
4. Audit log avancé
5. Assistant IA Pro
6. Exports avancés

Règle UX:

- en Core: afficher lock + CTA vers pricing
- en Pro: accès direct

## 8. Écrans app à implémenter

## 8.1 Settings > Licence

Contenu minimum:

1. Champ textarea `Coller votre clé`
2. Bouton `Activer`
3. Bouton `Supprimer la licence`
4. État actuel:

- `Aucune licence`
- `Pro actif`
- `Invalide`
- `Expirée`

5. Détails affichés:

- email
- payment_id
- issued_at
- expires_at

## 8.2 Erreurs UX recommandées

1. `INVALID_BASE64`
2. `INVALID_JSON`
3. `INVALID_FORMAT`
4. `INVALID_SIGNATURE`
5. `EXPIRED_LICENSE`
6. `UNSUPPORTED_TIER`

## 9. Intégration avec les endpoints showcase (optionnel mais recommandé)

Ces endpoints ne sont pas nécessaires à la validation offline, mais utiles en self-service support.

1. Vérifier statut

- `POST /api/license/status`
- body:

```json
{ "email": "user@example.com", "paymentId": "pi_xxx" }
```

- réponses:
  - `200` -> `status`, `licenseKey`, `amount`, `currency`, `createdAt`
  - `404` -> `status: "not_found"`

2. Renvoyer la clé par email

- `POST /api/license/resend`
- body:

```json
{ "email": "user@example.com" }
```

- réponses:
  - `200` -> `status: "sent"`
  - `404` -> `status: "not_found"`

## 10. Metadata Stripe utiles (diagnostic support)

Le backend showcase écrit ces metadata sur le PaymentIntent:

1. `qoredb_license_key`
2. `qoredb_customer_email`
3. `qoredb_payment_status` (`active`, `failed`, `refunded`)
4. `qoredb_license_sent_at` (si email envoyé)
5. `qoredb_license_email_last_error` (si échec email)

## 11. Sécurité à respecter côté app

1. Ne pas accepter de licence sans vérification Ed25519
2. Ne pas dériver les droits Pro depuis un flag UI local
3. Nettoyer les logs (ne pas logger la clé complète)
4. Ajouter un mécanisme anti-tamper simple (checksum local de config, optionnel)

## 12. Plan d'implémentation conseillé (app)

1. Créer module `license/verify` (parser + verify + erreurs typées)
2. Créer stockage sécurisé `license/store`
3. Créer `LicenseService` global
4. Brancher gating des features Pro
5. Ajouter écran Settings > Licence
6. Ajouter télémétrie locale non sensible (facultatif)
7. Ajouter tests unitaires et tests e2e

## 13. Checklist de tests app

## 13.1 Tests unitaires

1. clé valide -> `pro_active`
2. base64 invalide -> erreur dédiée
3. JSON invalide -> erreur dédiée
4. signature invalide -> refus
5. `tier != pro` -> refus
6. `expires_at` dépassée -> refus

## 13.2 Tests d'intégration

1. coller clé valide depuis la page success -> Pro actif
2. redémarrer app -> Pro reste actif
3. supprimer clé -> retour Core
4. clé tronquée -> refus propre
5. clé d'un autre environnement (mauvaise clé publique) -> refus

## 13.3 Test de non-régression

1. Toutes features Core restent disponibles sans licence
2. Toutes features Pro restent verrouillées sans licence

## 14. Définition de done

Implémentation considérée terminée quand:

1. activation offline fonctionnelle
2. persistance/revalidation au démarrage OK
3. gating Pro partout où nécessaire
4. UX licence claire et non bloquante
5. tests unitaires + intégration passent
