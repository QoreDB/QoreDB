// SPDX-License-Identifier: Apache-2.0

import { createCell, createEmptyNotebook, type QoreNotebook } from './notebookTypes';

/**
 * Import a .sql file into a notebook.
 * Splits by semicolons (respecting quoted strings) into SQL cells.
 */
export function importFromSql(content: string, title?: string): QoreNotebook {
  // Complex dialect constructs need the engine's parser. Keep the whole script
  // rather than splitting a comment, quoted identifier or procedural body.
  const complex = /--|\/\*|\$\w*\$|`|\[|\bBEGIN\b/i.test(content);
  const statements = complex ? [content] : splitSqlStatements(content);
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
  let currentMarkdown: string[] = [];
  let codeContent: string[] = [];
  let fence: { marker: string; length: number; type?: 'sql' | 'mongo' } | undefined;
  const flushMarkdown = () => {
    if (currentMarkdown.join('\n').trim())
      cells.push(createCell('markdown', currentMarkdown.join('\n').trim()));
    currentMarkdown = [];
  };

  for (const line of lines) {
    const match = /^ {0,3}(`{3,}|~{3,})(.*)$/.exec(line);
    if (!fence && match) {
      const language = match[2].trim().toLowerCase();
      const type =
        language === 'mongo' ? 'mongo' : language === 'sql' || !language ? 'sql' : undefined;
      fence = { marker: match[1][0], length: match[1].length, type };
      if (type) {
        flushMarkdown();
        codeContent = [];
      } else currentMarkdown.push(line);
    } else if (
      fence &&
      match &&
      match[1][0] === fence.marker &&
      match[1].length >= fence.length &&
      !match[2].trim()
    ) {
      if (fence.type) cells.push(createCell(fence.type, codeContent.join('\n')));
      else currentMarkdown.push(line);
      fence = undefined;
      codeContent = [];
    } else if (fence?.type) {
      codeContent.push(line);
    } else {
      currentMarkdown.push(line);
    }
  }
  if (fence?.type && codeContent.length) cells.push(createCell(fence.type, codeContent.join('\n')));
  flushMarkdown();

  nb.cells = cells.length > 0 ? cells : [createCell('sql')];
  return nb;
}

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
