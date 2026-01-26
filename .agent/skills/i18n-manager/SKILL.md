---
name: i18n-manager
description: Workflow for managing internationalization (i18n) in QoreDB. Use when the user asks to "add a translation", "translate this text", or "fix missing keys". Handles sync between en.json and fr.json.
---

# i18n Manager

This skill manages the synchronization of translation keys between English (`en.json`) and French (`fr.json`).

## Workflow

### 1. Adding a New Key

When the user asks to add a text (e.g., "Add 'Hello' to the home page"):

1.  **Read Both Files**: Read `src/locales/en.json` and `src/locales/fr.json`.
2.  **Determine Structure**: Decide on a dot-notation path (e.g., `home.welcome.title`).
3.  **Insert Ordered**: Insert the key in **both** files at the **same alphabetical position**. This is critical to prevent merge conflicts.
4.  **Infer Translation**: If the user only provided one language, infer the other (Agent capability).

### 2. Extracting Hardcoded Text

When refactoring a component:

1.  **Identify Text**: Find literal strings in JSX (e.g., `<span>Submit</span>`).
2.  **Create Key**: Create a key (e.g., `common.actions.submit`).
3.  **Replace in Code**: Replace with `{t('common.actions.submit')}`.
4.  **Update JSONs**: Add the key to both JSON files.

## Rules

*   **Snake Case Keys**: Use `snake_case` or `camelCase` for keys, consistently. (Current project: `camelCase`).
*   **Alphabetical Order**: Keys MUST be sorted alphabetically.
*   **No Missing Keys**: Never add a key to `en.json` without adding it to `fr.json` (even if the value is temporary English).

## Example Update

**Before:**
```json
{
  "about": "About",
  "contact": "Contact"
}
```

**After (Adding "Blog"):**
```json
{
  "about": "About",
  "blog": "Blog", // Inserted alphabetically
  "contact": "Contact"
}
```
