# Qore AI — Brand foundation

## Brand architecture

- **Product:** QoreDB
- **Intelligence layer:** Qore AI
- **Agent persona:** Q
- **Descriptor:** Your database agent
- **Master tagline:** Ask your database.
- **French campaign line:** Interrogez vos données.

Use **Qore AI** for navigation, documentation, release notes and feature marketing. Use **Q** when the interface speaks about the agent as an actor: “Q is exploring the schema” or “Ask Q”. Do not call the product “Qori”, “Qore Bot” or “QoreDB AI Assistant”.

## Promise

Q understands the schema, finds the right data and acts within the user's guardrails. It makes database intelligence accessible without hiding the queries, tools or permissions involved.

## Personality and voice

Q is precise, calm, transparent and protective. It answers directly, distinguishes facts from assumptions and explains the actions it takes. It never presents database writes as magic or downplays production risk.

- Prefer: “I found 24 active devices using this read-only query.”
- Avoid: “Done! I took care of everything for you.”
- Prefer verbs such as *explore, inspect, verify, query, compare* and *explain*.
- Use first person sparingly; the user's data and result remain the focus.

## Visual system

| Role | Token | Hex | Usage |
|---|---|---:|---|
| Qore lineage | `--q-accent` | `#6B5CFF` | Master mark, navigation, primary emphasis |
| Dark emphasis | `--q-accent` | `#7A6CFF` | Master mark in dark mode |
| Intelligence signal | `--q-ai-signal` | `#38D9FF` | Active reasoning, exploration and live agent state |
| Verified action | `--q-ai-verified` | `#31E6A1` | Successful, verified agent outcomes |
| Dark surface | `--q-bg-0` | `#0B0C0F` | Primary dark background |
| Light foreground | `--q-text-0` | `#F4F6FA` | Primary dark-mode text |

Amber remains reserved for approvals and red for rejected or blocked actions. Cyan must not replace semantic success, warning or error colors.

## Logo usage

The master mark combines four elements: the QoreDB shell, the database cube, a cyan exploration orbit and the Q signal. The compact micro-mark removes the cube and orbit where they would become illegible.

- Master mark minimum size: **40 px** digital.
- Compact micro-mark range: **12–24 px** digital.
- Clear space: at least the width of the Q badge around the mark.
- Use the monochrome asset when color reproduction is unavailable.
- Do not rotate the mark, recolor the cyan signal, add glow, or detach the Q badge.

Assets:

- `/public/brand/qore-ai-mark.svg`
- `/public/brand/qore-ai-mark-dark.svg`
- `/public/brand/qore-ai-mark-mono.svg`
- React UI component: `src/components/Brand/QoreAiMark.tsx`
