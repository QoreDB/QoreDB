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

/** Built-in renderers a viewer contribution may select. */
export type ViewerRenderer = 'json-tree' | 'image' | 'map' | 'chart';

/** What QoreDB matches a result column against to pick a viewer. At least
 *  one of `columnType` / `namePattern` must be set. */
export interface ViewerMatch {
  columnType?: string;
  /** Glob — `*` is the only wildcard. */
  namePattern?: string;
}

/** Declarative cell renderer a plugin contributes. */
export interface ResultViewerContribution {
  id: string;
  match: ViewerMatch;
  renderer: ViewerRenderer;
  /** Renderer-specific options, opaque to the registry. */
  options?: Record<string, unknown>;
}

/** A user-invocable action a plugin contributes. The id surfaced by the
 *  registry is namespaced (`<plugin-id>::<command-id>`); the runtime hook
 *  receives the bare command id. */
export interface CommandContribution {
  id: string;
  label: string;
  description?: string;
}

export interface PluginContributions {
  snippets: SnippetContribution[];
  connectionTemplates: ConnectionTemplateContribution[];
  themes: ThemeContribution[];
  resultViewers: ResultViewerContribution[];
  commands: CommandContribution[];
}

/** Lifecycle hooks an executable plugin may subscribe to. */
export type PluginHookKind = 'preExecute' | 'postExecute';

/** Capabilities a plugin manifest can request. Every value here is wired
 *  end-to-end at runtime. */
export type PluginCapabilityKind =
  | 'log'
  | 'notify'
  | 'storage'
  | 'queryRead'
  | 'http'
  | 'fs'
  | 'secrets';

export interface HttpCapability {
  allowedHosts: string[];
  /** When true, the plugin may reach private / loopback / link-local /
   *  cloud-metadata addresses. Off by default — flagged in the consent UI
   *  when on. */
  allowPrivateNetworks?: boolean;
}

export interface FsCapability {
  scope: 'pluginData';
}

/** Capability block a manifest declares. Every field is honoured at runtime;
 *  manifests declaring an unrecognised capability are rejected at install
 *  time. */
export interface PluginCapabilities {
  log?: boolean;
  notify?: boolean;
  storage?: boolean;
  queryRead?: boolean;
  http?: HttpCapability;
  fs?: FsCapability;
  /** Names of secrets the plugin will ask the host to read. */
  secrets?: string[];
}

/** Executable-runtime descriptor. Absent for purely declarative plugins. */
export interface PluginRuntimeSpec {
  abiVersion: number;
  /** WASM module filename, relative to the plugin folder. */
  entry: string;
  hooks: PluginHookKind[];
  capabilities?: PluginCapabilities;
  /** Optional `sha256-<64 hex>` digest of the WASM module. Plugins shipped
   *  with this field refuse to load on mismatch — the UI surfaces them as
   *  "Signed"; plugins without it are shown as "Unsigned". */
  integrity?: string;
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
  resultViewers: [],
  commands: [],
};
