// SPDX-License-Identifier: Apache-2.0

import { invoke } from '@/lib/transport';

// ============================================
// WORKSPACE COMMANDS
// ============================================

export type WorkspaceSource = 'detected' | 'manual' | 'default';

export interface WorkspaceManifest {
  version: number;
  name: string;
  created_at: string;
  updated_at: string;
}

export interface WorkspaceInfo {
  path: string;
  manifest: WorkspaceManifest;
  source: WorkspaceSource;
}

export interface RecentWorkspace {
  path: string;
  name: string;
  last_opened: string;
}

export interface WorkspaceResponse {
  success: boolean;
  workspace?: WorkspaceInfo;
  error?: string;
}

export async function detectWorkspace(): Promise<WorkspaceInfo | null> {
  return invoke('detect_workspace');
}

export async function getActiveWorkspace(): Promise<WorkspaceInfo> {
  return invoke('get_active_workspace');
}

export async function getWorkspaceProjectId(): Promise<string> {
  return invoke('get_workspace_project_id');
}

export async function createWorkspace(
  projectDir: string,
  name: string
): Promise<WorkspaceResponse> {
  return invoke('create_workspace', { projectDir, name });
}

export async function openWorkspace(qoredbPath: string): Promise<WorkspaceResponse> {
  return invoke('open_workspace', { qoredbPath });
}

export async function switchWorkspace(qoredbPath: string): Promise<WorkspaceResponse> {
  return invoke('switch_workspace', { qoredbPath });
}

export async function switchToDefaultWorkspace(): Promise<WorkspaceInfo> {
  return invoke('switch_to_default_workspace');
}

export async function renameWorkspace(newName: string): Promise<WorkspaceResponse> {
  return invoke('rename_workspace', { newName });
}

export async function listRecentWorkspaces(): Promise<RecentWorkspace[]> {
  return invoke('list_recent_workspaces');
}

// ============================================
// WORKSPACE QUERY LIBRARY
// ============================================

export interface WorkspaceQueryLibrary {
  version: number;
  folders: unknown[];
  items: unknown[];
}

export async function wsGetQueryLibrary(): Promise<WorkspaceQueryLibrary | null> {
  return invoke('ws_get_query_library');
}

export async function wsSaveQueryLibrary(library: WorkspaceQueryLibrary): Promise<boolean> {
  return invoke('ws_save_query_library', { library });
}

export async function importDefaultConnections(): Promise<number> {
  return invoke('import_default_connections');
}
