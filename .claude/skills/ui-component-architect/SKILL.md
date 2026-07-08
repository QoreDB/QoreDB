---
name: ui-component-architect
description: Standardized workflow for creating consistent React UI components using Tailwind, shadcn principles, and (when motion helps) Framer Motion. Use when the user asks to "create a component", "design a button", or "add a UI element".
---

# UI Component Architect

Guides the creation of consistent UI components for QoreDB. See `doc/rules/DESIGN.md`
for the design direction and tokens.

## Core principles

1. **Semantic tokens, no hardcoded values**: use theme tokens (`bg-primary`,
   `text-muted-foreground`, `border-input`, `destructive`, …), never raw hex or
   named colors. The project exposes both the `--q-*` foundation tokens and the
   shadcn-style semantic tokens (`--primary`, `--muted`, `--destructive`, …).
2. **Accessible**: semantic HTML, `focus-visible` states.
3. **Type-safe variants**: use `cva` (class-variance-authority) for variants.
4. **Motion where it helps**: Framer Motion is available (used e.g. in the Tabs).
   Add micro-interactions when they aid clarity — don't animate everything.

## Workflow

### 1. Component structure

New file in `src/components/ui/` (or the relevant feature folder) using the
`assets/component.tsx` template.

- Start with the SPDX header (`// SPDX-License-Identifier: Apache-2.0`).
- **Imports**: `cn` from `@/lib/utils`, `cva`, and `framer-motion` only if you
  animate.
- **Variants**: define visual styles with `cva`. Semantic tokens only.
- **Ref**: forward `ref` correctly.

### 2. Styling rules (Tailwind)

- **Colors**: semantic tokens (`primary`, `secondary`, `accent`, `muted`,
  `destructive`).
- **Spacing**: Tailwind scale (`gap-2`, `p-4`, …).
- **Borders**: `border-input`, `border` (1px default).
- **Radius**: `rounded-md` or `rounded-lg`.

### 3. Usage example

```tsx
import { Component } from './Component';

export function Demo() {
  return (
    <Component variant="outline" size="lg">
      Click Me
    </Component>
  );
}
```

## Template

See `assets/component.tsx` for the full boilerplate.

```tsx
// Quick reference
const componentVariants = cva('base-classes...', {
  variants: {
    variant: { default: '...', outline: '...' },
    size: { default: 'h-9 px-4', sm: 'h-8 px-3' },
  },
  defaultVariants: { variant: 'default', size: 'default' },
});
```

## Checklist

- [ ] SPDX header
- [ ] Used `cva` for variants
- [ ] `cn` imported from `@/lib/utils` (no local mock)
- [ ] Semantic tokens (no raw hex/names)
- [ ] Forwarded `ref` correctly
- [ ] Exported `Component` and `componentVariants`
