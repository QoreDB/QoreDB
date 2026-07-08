# QoreDB — Design

Visual direction, tokens, and UX rules for QoreDB. A local-first database
client for developers: SQL and NoSQL, production data, long sessions,
high-stakes operations. The UI must be calm, precise, fast, and trustworthy —
never chatty or decorative.

## Source Of Truth

`src/index.css` is the token reference. This document describes their semantics
and usage rules; if there is any divergence, `index.css` wins. No hard-coded
values in components (`#fff`, `16px`, `border-radius: 8px`): tokens only.

## Colors

Neutral foundation. Data should always be more visible than the UI.

| Token        | Light     | Dark      | Usage           |
| ------------ | --------- | --------- | --------------- |
| `--q-bg-0`   | `#ffffff` | `#0b0c0f` | Main background |
| `--q-bg-1`   | `#f6f7f9` | `#1c1e24` | Panels          |
| `--q-bg-2`   | `#eceef1` | `#272930` | Surfaces        |
| `--q-border` | `#dde0e5` | `#2a2d35` | Separators      |
| `--q-text-0` | `#0e1116` | `#f4f6fa` | Primary text    |
| `--q-text-1` | `#5b6070` | `#c0c4cc` | Labels          |
| `--q-text-2` | `#8a90a0` | `#6b7280` | Metadata        |

One accent only, no rainbow.

| Token               | Light     | Dark                     | Usage                 |
| ------------------- | --------- | ------------------------ | --------------------- |
| `--q-accent`        | `#6b5cff` | `#7a6cff`                | Selection, focus, CTA |
| `--q-accent-soft`   | `#e7e5ff` | `rgba(122,108,255,0.15)` | Highlight             |
| `--q-accent-strong` | `#5847ff` | `#9a8cff`                | Buttons               |

Semantic states, never decorative.

| Token         | Light     | Dark      | Usage       |
| ------------- | --------- | --------- | ----------- |
| `--q-success` | `#16a34a` | `#22c55e` | OK          |
| `--q-warning` | `#f59e0b` | `#fbbf24` | Warning     |
| `--q-error`   | `#dc2626` | `#f87171` | Danger      |
| `--q-info`    | `#3b82f6` | `#60a5fa` | Information |

Environments (with `-soft` variants for backgrounds).

| Token             | Light     | Dark      | Usage      |
| ----------------- | --------- | --------- | ---------- |
| `--q-env-dev`     | `#16a34a` | `#22c55e` | Dev        |
| `--q-env-staging` | `#f59e0b` | `#fbbf24` | Staging    |
| `--q-env-prod`    | `#dc2626` | `#f87171` | Production |

Bright colors and gradients are allowed only in onboarding, empty states, and
highlights. Never in tables, grids, or editors.

## Typography, Radius, Shadows

Tailwind tokens defined in the `@theme` block of `src/index.css`.

| Token          | Value                                        | Usage       |
| -------------- | --------------------------------------------- | ----------- |
| `--font-sans`  | `Inter, system-ui, -apple-system, sans-serif` | UI          |
| `--font-mono`  | `JetBrains Mono, Fira Code, monospace`        | Data, code  |
| `--radius-sm`  | `4px`                                         | Inputs      |
| `--radius-md`  | `6px`                                         | Buttons     |
| `--radius-lg`  | `10px`                                        | Cards       |
| `--radius-xl`  | `16px`                                        | Panels      |
| `--shadow-sm`  | `0 1px 2px rgba(0,0,0,0.04)`                  | Elevation 1 |
| `--shadow-md`  | `0 6px 16px rgba(0,0,0,0.08)`                 | Elevation 2 |

Strong contrast between headings and body text. Sans-serif for the UI,
monospace for data and code. Spacing and text sizes use the Tailwind scale
(4px base) through utilities — no dedicated tokens.

## Usage Rules

- Neutral UI, high-contrast data, accent used rarely and precisely.
- Density: tables and editors stay dense, navigation and chrome stay light.
  White space separates meaning; it does not decorate. Choose clarity over
  beauty.
- Keep radii restrained: no pills everywhere, keep it serious.

## UX Rules

- Keyboard first: everything must be reachable by keyboard (power users,
  long sessions).
- No modal spam, no "are you sure?" for safe actions.
- Dangerous actions must be visually distinct.
- Production must always be clearly distinguishable from dev (environment
  colors).
- Respect the data: no truncation without explicit control, no hidden
  mutation, no self-destructive action. The UI protects the user from errors.

## What QoreDB Is Not

Not a BI dashboard, not a charting tool, not a marketing UI, not a toy admin
panel. A professional instrument.

Decision rule when in doubt: would a developer trust this UI with their
production database at 2 a.m.? If not, it needs work.
