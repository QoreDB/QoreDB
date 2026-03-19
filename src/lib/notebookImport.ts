// SPDX-License-Identifier: Apache-2.0

import { createCell, createEmptyNotebook, type QoreNotebook } from './notebookTypes';

/**
 * Import a .sql file into a notebook.
 * Splits by semicolons (respecting quoted strings) into SQL cells.
 */
export function importFromSql(content: string, title?: string): QoreNotebook {
  const statements = splitSqlStatements(content);
  const nb = createEmptyNotebook(title ?? 'Imported SQL');
  nb.cells = statements.map(s => createCell('sql', s.trim()));
  if (nb.cells.length === 0) nb.cells = [createCell('sql')];
  return nb;
}

/**
 * Import a .md file into a notebook.
 * Fenced code blocks (```sql) become SQL cells, everything else becomes Markdown cells.
 */
export function importFromMarkdown(content: string, title?: string): QoreNotebook {
  const nb = createEmptyNotebook(title ?? 'Imported Markdown');
  const cells: ReturnType<typeof createCell>[] = [];

  const lines = content.split('\n');
  let currentMarkdown = '';
  let inCodeBlock = false;
  let codeContent = '';

  for (const line of lines) {
    if (!inCodeBlock && /^```(?:sql|mongo)?\s*$/i.test(line)) {
      // Flush markdown buffer
      if (currentMarkdown.trim()) {
        cells.push(createCell('markdown', currentMarkdown.trim()));
        currentMarkdown = '';
      }
      inCodeBlock = true;
      codeContent = '';
    } else if (inCodeBlock && line.startsWith('```')) {
      // End code block → SQL cell
      cells.push(createCell('sql', codeContent.trim()));
      inCodeBlock = false;
      codeContent = '';
    } else if (inCodeBlock) {
      codeContent += (codeContent ? '\n' : '') + line;
    } else {
      currentMarkdown += (currentMarkdown ? '\n' : '') + line;
    }
  }

  // Flush remaining
  if (inCodeBlock && codeContent.trim()) {
    cells.push(createCell('sql', codeContent.trim()));
  }
  if (currentMarkdown.trim()) {
    cells.push(createCell('markdown', currentMarkdown.trim()));
  }

  nb.cells = cells.length > 0 ? cells : [createCell('sql')];
  return nb;
}

/**
 * Split SQL by semicolons, respecting quoted strings.
 */
function splitSqlStatements(sql: string): string[] {
  const statements: string[] = [];
  let current = '';
  let inSingleQuote = false;
  let inDoubleQuote = false;
  let escaped = false;

  for (const ch of sql) {
    if (escaped) {
      current += ch;
      escaped = false;
      continue;
    }
    if (ch === '\\') {
      current += ch;
      escaped = true;
      continue;
    }
    if (ch === "'" && !inDoubleQuote) {
      inSingleQuote = !inSingleQuote;
      current += ch;
      continue;
    }
    if (ch === '"' && !inSingleQuote) {
      inDoubleQuote = !inDoubleQuote;
      current += ch;
      continue;
    }
    if (ch === ';' && !inSingleQuote && !inDoubleQuote) {
      if (current.trim()) statements.push(current.trim());
      current = '';
      continue;
    }
    current += ch;
  }

  if (current.trim()) statements.push(current.trim());
  return statements;
}
