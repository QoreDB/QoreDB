// SPDX-License-Identifier: Apache-2.0

import { open as openDialog, save } from '@tauri-apps/plugin-dialog';
import { readTextFile, writeTextFile } from '@tauri-apps/plugin-fs';
import { getWorkspaceState } from '../stores/workspaceStore';
import type { QoreNotebook } from './notebookTypes';

const QNB_FILTER = [{ name: 'QoreDB Notebook', extensions: ['qnb'] }];

/** Returns the default notebook directory when a file-based workspace is active. */
function getWorkspaceNotebookDir(): string | undefined {
  const { activeWorkspace } = getWorkspaceState();
  if (activeWorkspace && activeWorkspace.source !== 'default') {
    return `${activeWorkspace.path}/notebooks`;
  }
  return undefined;
}

/** Strip runtime-only fields before saving */
function stripForSave(notebook: QoreNotebook, includeResults: boolean): QoreNotebook {
  return {
    ...notebook,
    metadata: { ...notebook.metadata, updatedAt: new Date().toISOString() },
    cells: notebook.cells.map(cell => ({
      ...cell,
      executionState: 'idle' as const, // stale/running/success/error all reset to idle on save
      lastResult: includeResults ? cell.lastResult : undefined,
    })),
    variables: Object.fromEntries(
      Object.entries(notebook.variables).map(([k, v]) => [k, { ...v, currentValue: undefined }])
    ),
  };
}

export async function saveNotebookToFile(
  notebook: QoreNotebook,
  path: string | null,
  includeResults = false
): Promise<string | null> {
  const wsDir = getWorkspaceNotebookDir();
  const defaultName = `${notebook.metadata.title.replace(/[^a-zA-Z0-9_-]/g, '_')}.qnb`;
  const defaultPath = wsDir ? `${wsDir}/${defaultName}` : defaultName;
  const filePath =
    path ??
    (await save({
      defaultPath,
      filters: QNB_FILTER,
    }));
  if (!filePath) return null;
  const content = JSON.stringify(stripForSave(notebook, includeResults), null, 2);
  await writeTextFile(filePath, content);
  return filePath;
}

export async function openNotebookFromFile(): Promise<{
  notebook: QoreNotebook;
  path: string;
} | null> {
  const wsDir = getWorkspaceNotebookDir();
  const filePath = await openDialog({
    multiple: false,
    filters: QNB_FILTER,
    defaultPath: wsDir,
  });
  if (!filePath || Array.isArray(filePath)) return null;
  const raw = await readTextFile(filePath);
  const notebook = parseNotebook(raw);
  return { notebook, path: filePath };
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
}

function parseNotebook(raw: string): QoreNotebook {
  const notebook: unknown = JSON.parse(raw);
  const invalid = () => {
    throw new Error('Invalid notebook format');
  };
  if (
    !isRecord(notebook) ||
    notebook.version !== 1 ||
    !isRecord(notebook.metadata) ||
    !Array.isArray(notebook.cells) ||
    !isRecord(notebook.variables)
  )
    return invalid();
  const metadata = notebook.metadata;
  if (['id', 'title', 'createdAt', 'updatedAt'].some(key => typeof metadata[key] !== 'string'))
    return invalid();
  const ids = new Set<string>();
  for (const cell of notebook.cells) {
    if (
      !isRecord(cell) ||
      typeof cell.id !== 'string' ||
      !cell.id ||
      ids.has(cell.id) ||
      typeof cell.source !== 'string' ||
      !['sql', 'mongo', 'markdown', 'chart', 'contract', 'ai'].includes(String(cell.type))
    )
      return invalid();
    ids.add(cell.id);
    if (cell.config != null && !isRecord(cell.config)) return invalid();
    if (isRecord(cell.config)) {
      const config = cell.config;
      if (
        (config.label != null && typeof config.label !== 'string') ||
        ['collapsed', 'pinned', 'hideSource'].some(
          key => config[key] != null && typeof config[key] !== 'boolean'
        ) ||
        (config.maxRows != null &&
          (typeof config.maxRows !== 'number' ||
            !Number.isSafeInteger(config.maxRows) ||
            config.maxRows < 0))
      )
        return invalid();
      if (config.namespace != null) {
        const namespace = config.namespace;
        if (
          !isRecord(namespace) ||
          typeof namespace.database !== 'string' ||
          (namespace.schema != null && typeof namespace.schema !== 'string')
        )
          return invalid();
      }
      if (config.chartConfig != null) {
        const chart = config.chartConfig;
        if (
          !isRecord(chart) ||
          typeof chart.sourceLabel !== 'string' ||
          !['bar', 'line', 'pie', 'scatter'].includes(String(chart.type)) ||
          typeof chart.xColumn !== 'string' ||
          !Array.isArray(chart.yColumns) ||
          chart.yColumns.some(column => typeof column !== 'string') ||
          (chart.title != null && typeof chart.title !== 'string')
        )
          return invalid();
      }
    }
  }
  for (const variable of Object.values(notebook.variables)) {
    if (
      !isRecord(variable) ||
      typeof variable.name !== 'string' ||
      !['text', 'number', 'date', 'select'].includes(String(variable.type)) ||
      ['currentValue', 'defaultValue', 'description'].some(
        key => variable[key] != null && typeof variable[key] !== 'string'
      ) ||
      (variable.options != null &&
        (!Array.isArray(variable.options) ||
          variable.options.some(option => typeof option !== 'string')))
    )
      return invalid();
  }
  return notebook as unknown as QoreNotebook;
}

// --- Pending notebook cache (for opening from file menu / palette) ---
const pendingNotebooks = new Map<string, QoreNotebook>();

export function setPendingNotebook(path: string, notebook: QoreNotebook): void {
  pendingNotebooks.set(path, notebook);
}

export function consumePendingNotebook(path: string): QoreNotebook | null {
  const nb = pendingNotebooks.get(path);
  if (nb) pendingNotebooks.delete(path);
  return nb ?? null;
}

export function saveDraft(tabId: string, notebook: QoreNotebook): void {
  try {
    localStorage.setItem(
      `qnb_draft_${tabId}`,
      JSON.stringify({ ...stripForSave(notebook, false), variables: notebook.variables })
    );
  } catch {
    /* storage full, ignore */
  }
}

export function loadDraft(tabId: string): QoreNotebook | null {
  try {
    const raw = localStorage.getItem(`qnb_draft_${tabId}`);
    return raw ? parseNotebook(raw) : null;
  } catch {
    return null;
  }
}

export function clearDraft(tabId: string): void {
  localStorage.removeItem(`qnb_draft_${tabId}`);
}
