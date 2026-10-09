// SPDX-License-Identifier: Apache-2.0

/// <reference types="node" />

import { mkdtemp, readFile, rm, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { exportToHtml, exportToMarkdown } from './notebookExport';
import { importFromMarkdown, importFromSql } from './notebookImport';
import { loadDraft, openNotebookFromFile, saveDraft, saveNotebookToFile } from './notebookIO';
import { createCell, createEmptyNotebook } from './notebookTypes';

const dialog = vi.hoisted(() => ({ open: vi.fn(), save: vi.fn() }));
vi.mock('@tauri-apps/plugin-dialog', () => dialog);
vi.mock('@tauri-apps/plugin-fs', () => ({
  readTextFile: (path: string) => readFile(path, 'utf8'),
  writeTextFile: (path: string, content: string) => writeFile(path, content, 'utf8'),
}));
let directory: string;
beforeEach(async () => {
  directory = await mkdtemp(join(tmpdir(), 'qore-notebook-'));
});
afterEach(async () => {
  vi.clearAllMocks();
  vi.unstubAllGlobals();
  await rm(directory, { recursive: true, force: true });
});

describe('notebook file round trips (native plugins replaced by Node file IO)', () => {
  it('preserves sources, exact values, metadata and config without runtime results', async () => {
    const notebook = createEmptyNotebook('Été <synthetic>');
    notebook.cells = [
      createCell('sql', 'SELECT 9007199254740993;'),
      createCell('mongo', '{"find":"users"}'),
      createCell('markdown', 'Notes'),
      createCell('chart'),
    ];
    notebook.cells[0].config = { label: 'numbers', namespace: { database: 'fixture' } };
    notebook.cells[3].config = {
      collapsed: false,
      pinned: true,
      hideSource: false,
      maxRows: 100,
      chartConfig: {
        sourceLabel: 'numbers',
        type: 'bar',
        xColumn: 'x',
        yColumns: ['y'],
        title: 'Values',
      },
    };
    notebook.cells[0].lastResult = {
      type: 'table',
      columns: [],
      rows: [{ values: ['9007199254740993'] }],
    };
    notebook.cells[0].executionState = 'running';
    notebook.variables = {
      amount: {
        name: 'amount',
        type: 'number',
        defaultValue: '0.12345678901234567890',
        currentValue: '42',
      },
    };
    const path = join(directory, 'notebook.qnb');
    await saveNotebookToFile(notebook, path);
    dialog.open.mockResolvedValue(path);
    const loaded = await openNotebookFromFile();
    expect(loaded?.notebook.cells.map(cell => [cell.type, cell.source, cell.config])).toEqual(
      notebook.cells.map(cell => [cell.type, cell.source, cell.config])
    );
    expect(loaded?.notebook.metadata.title).toBe(notebook.metadata.title);
    expect(loaded?.notebook.variables.amount.defaultValue).toBe('0.12345678901234567890');
    expect(loaded?.notebook.variables.amount.currentValue).toBeUndefined();
    expect(loaded?.notebook.cells[0].lastResult).toBeUndefined();
    expect(notebook.cells[0].lastResult?.rows?.[0].values[0]).toBe('9007199254740993');
    expect(notebook.cells[0].executionState).toBe('running');
  });

  it.each([
    { version: 1, cells: [] },
    { ...createEmptyNotebook(), cells: [{ id: 'x', type: 'sql', source: 42 }] },
    { ...createEmptyNotebook(), variables: null },
    {
      ...createEmptyNotebook(),
      cells: [
        { id: 'x', type: 'sql', source: '' },
        { id: 'x', type: 'sql', source: '' },
      ],
    },
  ])('rejects an invalid structure before replacing the notebook: %j', async invalid => {
    const path = join(directory, 'invalid.qnb');
    await writeFile(path, JSON.stringify(invalid));
    dialog.open.mockResolvedValue(path);
    await expect(openNotebookFromFile()).rejects.toThrow();
  });

  it('preserves Mongo and SQL cell types through Markdown export/import', () => {
    const notebook = createEmptyNotebook('Mixed');
    notebook.cells = [createCell('mongo', '{"find":"users"}'), createCell('sql', 'SELECT 1')];
    const imported = importFromMarkdown(exportToMarkdown(notebook));
    expect(
      imported.cells.filter(cell => cell.type !== 'markdown').map(cell => [cell.type, cell.source])
    ).toEqual(notebook.cells.map(cell => [cell.type, cell.source]));
  });

  it('preserves fenced content within Markdown cells', () => {
    const markdown = '# Code\n\n```js\nconsole.log(1)\n```\n\nAfter';
    const imported = importFromMarkdown(markdown);
    expect(imported.cells).toHaveLength(1);
    expect(imported.cells[0].source).toBe(markdown);
  });

  it.each([
    '-- keep this; comment\nSELECT 1; SELECT 2;',
    'SELECT $body$a;b$body$; SELECT 2;',
    'SELECT `semi;colon` FROM t; SELECT 2;',
    'SELECT [semi;colon] FROM t; SELECT 2;',
    'CREATE TRIGGER audit AFTER INSERT ON t BEGIN INSERT INTO log VALUES (1); INSERT INTO log VALUES (2); END;',
  ])('keeps complex SQL intact for the engine parser: %s', source => {
    const imported = importFromSql(source);
    expect(imported.cells.map(cell => cell.source)).toEqual([source]);
  });

  it('still splits simple SQL while preserving quoted semicolons', () => {
    expect(importFromSql("SELECT 'a;b'; SELECT 2;").cells.map(cell => cell.source)).toEqual([
      "SELECT 'a;b'",
      'SELECT 2',
    ]);
  });

  it('uses a longer Markdown fence when code contains backticks', () => {
    const notebook = createEmptyNotebook();
    notebook.cells[0].source = "SELECT 'hello\n```\nworld'";
    const imported = importFromMarkdown(exportToMarkdown(notebook));
    expect(imported.cells.find(cell => cell.type === 'sql')?.source).toBe(notebook.cells[0].source);
  });

  it('drafts retain current inputs without persisting result rows', () => {
    const storage = new Map<string, string>();
    vi.stubGlobal('localStorage', {
      setItem: (key: string, value: string) => storage.set(key, value),
      getItem: (key: string) => storage.get(key),
    });
    const notebook = createEmptyNotebook();
    notebook.cells[0].lastResult = {
      type: 'table',
      rows: [{ values: ['synthetic-sensitive-row'] }],
    };
    notebook.variables = { amount: { name: 'amount', type: 'number', currentValue: '42' } };
    saveDraft('fixture', notebook);
    expect(storage.get('qnb_draft_fixture')).not.toContain('synthetic-sensitive-row');
    expect(loadDraft('fixture')?.variables.amount.currentValue).toBe('42');
  });

  it('writes escaped HTML and excludes results unless requested', async () => {
    const notebook = createEmptyNotebook('<script>synthetic</script>');
    notebook.cells[0].lastResult = {
      type: 'table',
      columns: [{ name: 'value', data_type: 'text', nullable: false }],
      rows: [{ values: ['<img src=x onerror=alert(1)>'] }],
    };
    const path = join(directory, 'export.html');
    await writeFile(path, exportToHtml(notebook, true));
    const html = await readFile(path, 'utf8');
    expect(html).toContain('&lt;img src=x onerror=alert(1)&gt;');
    expect(html).not.toContain('<script>');
    expect(exportToHtml(notebook)).not.toContain('onerror');
  });
});

it.each([
  { config: { namespace: { database: 42 } } },
  { config: { label: { invalid: true } } },
  {
    config: {
      chartConfig: { sourceLabel: 'a', type: 'bar', xColumn: 'x', yColumns: 'not-an-array' },
    },
  },
  { config: { chartConfig: { sourceLabel: 'a', type: 'unknown', xColumn: 'x', yColumns: [] } } },
  { config: { chartConfig: { sourceLabel: 'a', type: 'bar', xColumn: 'x', yColumns: [42] } } },
  { config: { pinned: 'false' } },
  { config: { maxRows: -1 } },
])('rejects invalid persisted cell configuration before opening: %j', async patch => {
  const notebook = createEmptyNotebook();
  Object.assign(notebook.cells[0], patch);
  const path = join(directory, 'invalid-nested.qnb');
  await writeFile(path, JSON.stringify(notebook));
  dialog.open.mockResolvedValue(path);
  await expect(openNotebookFromFile()).rejects.toThrow();
});
