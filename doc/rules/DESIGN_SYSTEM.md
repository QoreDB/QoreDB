# **QoreDB — Design & Product DNA**

> This document defines the **visual, UX and product direction** of QoreDB.
> It is the source of truth for all UI, UX and product decisions.

---

## 1. What QoreDB is

QoreDB is a **modern, local-first database client** for developers.

It is designed for:

* SQL + NoSQL
* production data
* long sessions
* high-stakes operations

It must feel:

> **calm, precise, fast, and trustworthy**

Not playful.
Not flashy.
Not enterprise-bloated.

---

## 2. Design Philosophy

QoreDB UI is based on two references:

### 🧱 GitHub Primer — Structure

We take from Primer:

* layout logic
* spacing discipline
* accessibility
* component hierarchy
* data-dense patterns

Primer defines **how things work**.

---

### 🎨 Stripe Sessions — Visual language

We take from Stripe Sessions:

* typography contrast
* elegance
* rhythm
* subtle accents
* premium feel

Stripe defines **how things feel**.

---

### QoreDB = Primer × Stripe

> **Primer gives us bones.
> Stripe gives us skin.**

---

## 3. What QoreDB UI must feel like

When opening QoreDB, users should feel:

* “I can trust this with production data”
* “Nothing here is accidental”
* “Everything is where it should be”
* “This tool respects my time”

The UI should feel:

* calm
* quiet
* confident
* extremely clear

---

## 4. Density rules

QoreDB is a **data-heavy tool**, not a marketing site.

Rules:

* Tables and editors are dense
* Navigation and chrome are light
* Whitespace is used to separate **meaning**, not to decorate

If forced to choose:

> **Clarity beats beauty.**

---

## 5. Color philosophy

* The base UI is neutral (dark & light)
* Data must always be more visible than the UI
* Color is used only for:

  * state
  * selection
  * focus
  * danger / success
  * key actions

Bright colors and gradients:

* are allowed only in

  * onboarding
  * empty states
  * highlights
* never in tables, grids or editors

---

## 6. Typography philosophy

* Sans-serif for UI
* Monospace for data and code
* High contrast between:

  * titles
  * sections
  * content
* Titles are expressive (Stripe-inspired)
* UI labels are subtle (Primer-inspired)

The typography must:

> guide the eye without screaming.

---

## 7. Layout philosophy

Base layout:

```
┌────────────────────────────────────┐
│ Top bar (context, actions)          │
├──────────────┬─────────────────────┤
│ Sidebar      │ Main work area       │
│ (databases)  │ (tabs, editors, data)│
│              │                     │
├──────────────┴─────────────────────┤
│ Status bar (env, connection, state) │
└────────────────────────────────────┘
```

* Sidebar = navigation
* Main area = work
* Status bar = truth (prod/dev, connected, etc)

---

## 8. UX principles

QoreDB is optimized for:

* keyboard
* power users
* long sessions

Rules:

* everything should be reachable via keyboard
* no modal spam
* no “are you sure?” for safe actions
* dangerous actions must be visually distinct

---

## 9. Data respect

QoreDB UI must **respect data**:

* no truncation without explicit control
* no hidden mutations
* no auto-destructive actions
* production must always look different from dev

The UI must protect the user from mistakes.

---

## 10. What QoreDB is NOT

QoreDB must never become:

* a BI dashboard
* a charting tool
* a marketing UI
* a SaaS-first product
* a toy admin panel

It is a **professional instrument**.

---

## 11. Decision rule

When in doubt:

> **Would a developer trust this UI with their production database at 2am?**

If the answer is no → redesign.