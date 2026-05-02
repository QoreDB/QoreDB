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
  const notebook = JSON.parse(raw) as QoreNotebook;
  if (notebook.version !== 1 || !Array.isArray(notebook.cells)) {
    throw new Error('Invalid notebook format');
  }
  return { notebook, path: filePath };
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

/** Auto-save draft to localStorage */
export function saveDraft(tabId: string, notebook: QoreNotebook): void {
  try {
    localStorage.setItem(`qnb_draft_${tabId}`, JSON.stringify(notebook));
  } catch {
    /* storage full, ignore */
  }
}

export function loadDraft(tabId: string): QoreNotebook | null {
  try {
    const raw = localStorage.getItem(`qnb_draft_${tabId}`);
    return raw ? JSON.parse(raw) : null;
  } catch {
    return null;
  }
}

export function clearDraft(tabId: string): void {
  localStorage.removeItem(`qnb_draft_${tabId}`);
}
