---
name: ui-component-architect
description: Standardized workflow for creating premium, animated React UI components using Tailwind, Shadcn principles, and Framer Motion. Use when the user asks to "create a component", "design a button", or "add a UI element".
---

# UI Component Architect

This skill guides the creation of premium, consistent UI components for QoreDB.

## Core Principles

1.  **Premium Aesthetics**: Avoid default browser styles. Use `bg-primary`, `text-muted-foreground`, etc., from the theme.
2.  **Interactive**: All interactive elements must have hover/active states and micro-animations.
3.  **Accessible**: Use semantic HTML and `focus-visible` states.
4.  **Type-Safe**: Use `cva` (Class Variance Authority) for variant management.

## Workflow

### 1. Component Structure

Create a new file in `src/components/ui/` (or specific feature folder) using the `assets/component.tsx` template.

*   **Imports**: `framer-motion`, `cn` (clsx/tailwind-merge), `cva`.
*   **Variants**: Define visual styles using `cva`. Avoid hardcoded colors (e.g., `bg-blue-500`); use CSS variables (e.g., `bg-primary`).
*   **Animation**: Wrap the root element in `motion.<element>` and add subtle `initial`/`animate` props.

### 2. Styling Rules (Tailwind)

*   **Colors**: Use semantic tokens: `primary`, `secondary`, `accent`, `muted`, `destructive`.
*   **Spacing**: Use `gap-2`, `p-4`, etc.
*   **Borders**: `border-input`, `border` (1px default).
*   **Radius**: `rounded-md` or `rounded-lg`.

### 3. Usage Example

```tsx
import { Component } from "./Component";

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
// Quick Reference
const componentVariants = cva(
  "base-classes...",
  {
    variants: {
      variant: { default: "...", outline: "..." },
      size: { default: "h-9 px-4", sm: "h-8 px-3" }
    },
    defaultVariants: { variant: "default", size: "default" }
  }
);
```

## Checklist

- [ ] Used `cva` for variants
- [ ] Added `framer-motion` for micro-interactions
- [ ] Used semantic colors (not raw hex/names)
- [ ] Forwarded `ref` correctly
- [ ] Exported `Component` and `componentVariants`
