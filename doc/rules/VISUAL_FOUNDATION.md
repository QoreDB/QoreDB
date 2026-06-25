# QoreDB — Visual Foundation v1

Objectif : une UI aussi lisible et dense que GitHub, aussi élégante que Stripe,
pensée pour des outils data.

## Palette de base (neutres)

| Token        | Light     | Dark      | Usage           |
| ------------ | --------- | --------- | --------------- |
| `--q-bg-0`   | `#FFFFFF` | `#0B0C0F` | Fond principal  |
| `--q-bg-1`   | `#F6F7F9` | `#14151A` | Panneaux        |
| `--q-bg-2`   | `#ECEEF1` | `#1C1E24` | Surfaces        |
| `--q-border` | `#DDE0E5` | `#2A2D35` | Séparations     |
| `--q-text-0` | `#0E1116` | `#F4F6FA` | Texte principal |
| `--q-text-1` | `#5B6070` | `#9AA0AE` | Labels          |
| `--q-text-2` | `#8A90A0` | `#6B7280` | Metadata        |

## Accent

Un seul accent principal, pas d'arc-en-ciel.

| Token               | Light     | Dark                     | Usage                 |
| ------------------- | --------- | ------------------------ | --------------------- |
| `--q-accent`        | `#6B5CFF` | `#7A6CFF`                | Sélection, focus, CTA |
| `--q-accent-soft`   | `#E7E5FF` | `rgba(122,108,255,0.15)` | Surbrillance          |
| `--q-accent-strong` | `#5847FF` | `#9A8CFF`                | Boutons               |

## États sémantiques

| Type    | Light     | Dark      | Usage       |
| ------- | --------- | --------- | ----------- |
| Success | `#16A34A` | `#22C55E` | OK          |
| Warning | `#F59E0B` | `#FBBF24` | Attention   |
| Error   | `#DC2626` | `#F87171` | Danger      |
| Info    | `#3B82F6` | `#60A5FA` | Information |

Ces couleurs ne sont jamais décoratives.

## Typographie

```
--q-font-ui: Inter, system-ui, -apple-system, Segoe UI, sans-serif;
--q-font-code: JetBrains Mono, Fira Code, monospace;
```

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

Fort contraste titres / corps, lisibilité et rigueur avant tout.

## Spacing

Base 4px, UI dense mais respirable.

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

## Radius

| Token      | Value | Usage   |
| ---------- | ----- | ------- |
| `--q-r-sm` | 4px   | Inputs  |
| `--q-r-md` | 6px   | Buttons |
| `--q-r-lg` | 10px  | Cards   |
| `--q-r-xl` | 16px  | Panels  |

Pas de « pills » partout : tout doit rester sérieux.

## Shadows

Subtiles.

```
--q-shadow-sm: 0 1px 2px rgba(0,0,0,0.04)
--q-shadow-md: 0 6px 16px rgba(0,0,0,0.08)
--q-shadow-focus: 0 0 0 3px rgba(122,108,255,0.35)
```

## Règles d'usage

- Aucune valeur en dur (`#fff`, `#000`, `16px`, `border-radius: 8px`) : uniquement des tokens Qore.
- UI neutre, data contrastée, accent rare et précis.
