// SPDX-License-Identifier: Apache-2.0

/**
 * Plugin system types — mirror of `src-tauri/src/plugins/mod.rs`.
 *
 * v0.1.29 ships two flavours: declarative contributions (snippet packs,
 * connection templates, themes) and optional executable runtimes that wire
 * sandboxed WASM into the query lifecycle.
 */

export interface SnippetContribution {
  id: string;
  label: string;
  description: string;
  template: string;
}

export interface ConnectionTemplateContribution {
  id: string;
  name: string;
  description?: string;
  driver: string;
  defaults: Record<string, unknown>;
}

export interface ThemeContribution {
  id: string;
  name: string;
  description?: string;
  /** QoreDB design tokens (`--q-*`) for light mode. */
  light: Record<string, string>;
  /** QoreDB design tokens (`--q-*`) for dark mode. */
  dark: Record<string, string>;
}

export interface PluginContributions {
  snippets: SnippetContribution[];
  connectionTemplates: ConnectionTemplateContribution[];
  themes: ThemeContribution[];
}

/** Lifecycle hooks an executable plugin may subscribe to. */
export type PluginHookKind = 'preExecute' | 'postExecute';

/** Phase 2 capabilities a plugin manifest can request. */
export type PluginCapabilityKind = 'log' | 'notify' | 'storage' | 'queryRead';

/** Capability block a manifest declares — only the Phase 2 ones are honoured
 *  at runtime yet; Phase 3 ones (http/fs/secrets/queryExec) parse but never
 *  grant access. */
export interface PluginCapabilities {
  log?: boolean;
  notify?: boolean;
  storage?: boolean;
  queryRead?: boolean;
}

/** Executable-runtime descriptor. Absent for purely declarative plugins. */
export interface PluginRuntimeSpec {
  abiVersion: number;
  /** WASM module filename, relative to the plugin folder. */
  entry: string;
  hooks: PluginHookKind[];
  capabilities?: PluginCapabilities;
}

/** Tauri event payload emitted when a plugin issues a `notify` call. */
export interface PluginNotifyEvent {
  pluginId: string;
  level: 'info' | 'success' | 'warning' | 'error';
  message: string;
}

export interface PluginManifest {
  id: string;
  name: string;
  version: string;
  author?: string;
  description?: string;
  /** Optional QoreDB version requirement, e.g. ">=0.1.29". */
  qoredb?: string;
  contributes: PluginContributions;
  /** Set when the plugin ships sandboxed WASM code. */
  runtime?: PluginRuntimeSpec;
}

export interface InstalledPlugin {
  manifest: PluginManifest;
  /** Folder name under the plugins directory. */
  dirName: string;
  /** Whether the plugin is enabled (its contributions are active). */
  enabled: boolean;
  /** Whether the plugin's `qoredb` requirement matches this build. */
  compatible: boolean;
}

export const EMPTY_CONTRIBUTIONS: PluginContributions = {
  snippets: [],
  connectionTemplates: [],
  themes: [],
};
