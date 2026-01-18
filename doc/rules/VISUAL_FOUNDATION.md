# **QoreDB — Visual Foundation v1**

## 🎯 Objectif

Créer une UI qui est :

* aussi lisible et dense que GitHub
* aussi élégante et moderne que Stripe Sessions
* mais pensée pour des **outils data**

---

# 1️⃣ Palette de base

### Base (neutres)

| Token        | Light     | Dark      | Usage           |
| ------------ | --------- | --------- | --------------- |
| `--q-bg-0`   | `#FFFFFF` | `#0B0C0F` | Fond principal  |
| `--q-bg-1`   | `#F6F7F9` | `#14151A` | Panneaux        |
| `--q-bg-2`   | `#ECEEF1` | `#1C1E24` | Surfaces        |
| `--q-border` | `#DDE0E5` | `#2A2D35` | Séparations     |
| `--q-text-0` | `#0E1116` | `#F4F6FA` | Texte principal |
| `--q-text-1` | `#5B6070` | `#9AA0AE` | Labels          |
| `--q-text-2` | `#8A90A0` | `#6B7280` | Metadata        |

---

# 2️⃣ Accent (Stripe-inspired)

Un seul accent principal. Pas d’arc-en-ciel.

| Token               | Light     | Dark                     | Usage                 |
| ------------------- | --------- | ------------------------ | --------------------- |
| `--q-accent`        | `#6B5CFF` | `#7A6CFF`                | Sélection, focus, CTA |
| `--q-accent-soft`   | `#E7E5FF` | `rgba(122,108,255,0.15)` | Surbrillance          |
| `--q-accent-strong` | `#5847FF` | `#9A8CFF`                | Boutons               |

---

# 3️⃣ États sémantiques

| Type    | Light     | Dark      | Usage       |
| ------- | --------- | --------- | ----------- |
| Success | `#16A34A` | `#22C55E` | OK          |
| Warning | `#F59E0B` | `#FBBF24` | Attention   |
| Error   | `#DC2626` | `#F87171` | Danger      |
| Info    | `#3B82F6` | `#60A5FA` | Information |

Ces couleurs **ne sont jamais décoratives**.

---

# 4️⃣ Typographie

### UI

```
--q-font-ui: Inter, system-ui, -apple-system, Segoe UI, sans-serif;
```

### Code

```
--q-font-code: JetBrains Mono, Fira Code, monospace;
```

---

## Échelle

| Token         | Size | Usage         |
| ------------- | ---- | ------------- |
| `--q-text-xs` | 11px | Metadata      |
| `--q-text-sm` | 13px | Labels        |
| `--q-text-md` | 14px | UI            |
| `--q-text-lg` | 16px | Contenu       |
| `--q-text-xl` | 18px | Section title |
| `--q-h3`      | 24px | View title    |
| `--q-h2`      | 32px | Page          |
| `--q-h1`      | 40px | Hero          |

Stripe influence = **fort contraste titres / corps**
Primer influence = **lisibilité et rigueur**

---

# 5️⃣ Spacing

Base 4px.

```
--q-1: 4px
--q-2: 8px
--q-3: 12px
--q-4: 16px
--q-6: 24px
--q-8: 32px
--q-12: 48px
--q-16: 64px
```

UI dense mais respirable.

---

# 6️⃣ Radius

| Token      | Value | Usage   |
| ---------- | ----- | ------- |
| `--q-r-sm` | 4px   | Inputs  |
| `--q-r-md` | 6px   | Buttons |
| `--q-r-lg` | 10px  | Cards   |
| `--q-r-xl` | 16px  | Panels  |

Pas de “pills” partout.
Tout doit rester sérieux.

---

# 7️⃣ Shadows

Subtiles, comme Stripe.

```
--q-shadow-sm: 0 1px 2px rgba(0,0,0,0.04)
--q-shadow-md: 0 6px 16px rgba(0,0,0,0.08)
--q-shadow-focus: 0 0 0 3px rgba(122,108,255,0.35)
```

---

# 8️⃣ Comment tu dois t’en servir

Quand tu codes QoreDB :

* aucun `#fff`, `#000`, `16px`, `border-radius: 8px`
* uniquement des tokens Qore
* UI = neutre
* Data = contrastée
* Accent = rare et précis

---

# 9️⃣ Ce que tu as maintenant

Tu as :

* une identité visuelle
* une palette
* une typographie
* une grammaire

Sans Figma.
Sans bullshit.

Tu peux maintenant faire une UI qui :

> ressemble à un produit qui pourrait exister en 2026.