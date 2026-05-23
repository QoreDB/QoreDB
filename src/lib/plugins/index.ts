// SPDX-License-Identifier: Apache-2.0

/** Tauri bindings for the declarative plugin system. */

import { invoke } from '@tauri-apps/api/core';
import type { InstalledPlugin, PluginCapabilityKind, PluginContributions } from './types';

/** Lists every installed plugin with its runtime state. */
export async function listPlugins(): Promise<InstalledPlugin[]> {
  return invoke('list_plugins');
}

/** Installs (or updates) a plugin from a local folder. */
export async function installPlugin(sourcePath: string): Promise<InstalledPlugin> {
  return invoke('install_plugin', { sourcePath });
}

/** Removes an installed plugin. */
export async function removePlugin(pluginId: string): Promise<void> {
  return invoke('remove_plugin', { pluginId });
}

/** Enables or disables an installed plugin. */
export async function setPluginEnabled(pluginId: string, enabled: boolean): Promise<void> {
  return invoke('set_plugin_enabled', { pluginId, enabled });
}

/** Returns the aggregated contributions of all enabled, compatible plugins. */
export async function getPluginContributions(): Promise<PluginContributions> {
  return invoke('get_plugin_contributions');
}

/** Returns the capabilities currently granted to a plugin. */
export async function getPluginConsent(pluginId: string): Promise<PluginCapabilityKind[]> {
  return invoke('get_plugin_consent', { pluginId });
}

/** Overwrites the capabilities granted to a plugin; the runtime reloads
 *  so the new set takes effect immediately. */
export async function setPluginConsent(
  pluginId: string,
  grants: PluginCapabilityKind[],
): Promise<void> {
  return invoke('set_plugin_consent', { pluginId, grants });
}

export * from './types';
