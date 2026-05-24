// SPDX-License-Identifier: Apache-2.0

/** Marketplace client — fetches the catalog from the QoreDB showcase API.
 *  The catalog shape mirrors the registry's `index.json` (see
 *  qoredb-plugins-registry/schema/registry-entry.schema.json). */

export type MarketplacePluginKind = 'declarative' | 'executable';
export type MarketplaceHook = 'preExecute' | 'postExecute';
export type MarketplaceCapability =
  | 'log'
  | 'notify'
  | 'storage'
  | 'queryRead'
  | 'http'
  | 'fs'
  | 'secrets';

export const PLUGIN_CATEGORIES = [
  'safety',
  'observability',
  'productivity',
  'theming',
  'integrations',
] as const;

export type MarketplaceCategory = (typeof PLUGIN_CATEGORIES)[number];

export interface MarketplaceRuntimeSummary {
  abiVersion: 1;
  entry: string;
  hooks: MarketplaceHook[];
  capabilities: MarketplaceCapability[];
  integrity: string | null;
}

export interface MarketplaceContributionSummary {
  snippets: number;
  connectionTemplates: number;
  themes: number;
  resultViewers: number;
  commands: string[];
}

export interface MarketplaceArchive {
  url: string;
  sha256: string;
  sizeBytes: number;
}

export interface MarketplaceVersion {
  version: string;
  qoredb: string | null;
  category: MarketplaceCategory | null;
  kind: MarketplacePluginKind;
  runtime: MarketplaceRuntimeSummary | null;
  contributes: MarketplaceContributionSummary;
  archive: MarketplaceArchive;
  manifestUrl: string;
}

export interface MarketplacePlugin {
  id: string;
  name: string;
  author: string | null;
  description: string | null;
  category: MarketplaceCategory | null;
  latestVersion: string;
  kind: MarketplacePluginKind;
  versions: MarketplaceVersion[];
}

export interface MarketplaceIndex {
  registryVersion: 1;
  generatedAt: string;
  plugins: MarketplacePlugin[];
}

const DEFAULT_BASE = 'https://qoredb.com';

function baseUrl(): string {
  // Allow override via Vite env so a dev build can hit a local showcase.
  return (
    (import.meta.env?.VITE_QOREDB_MARKETPLACE_URL as string | undefined)?.replace(/\/$/, '') ||
    DEFAULT_BASE
  );
}

export class MarketplaceError extends Error {
  constructor(message: string) {
    super(message);
    this.name = 'MarketplaceError';
  }
}

/** Fetches the full catalog from `<showcase>/api/plugins`. */
export async function fetchMarketplaceIndex(): Promise<MarketplaceIndex> {
  const response = await fetch(`${baseUrl()}/api/plugins`, {
    headers: { Accept: 'application/json' },
  });
  if (!response.ok) {
    throw new MarketplaceError(`Marketplace responded with HTTP ${response.status}`);
  }
  const json = (await response.json()) as MarketplaceIndex;
  if (json.registryVersion !== 1) {
    throw new MarketplaceError(`Unsupported registry version: ${json.registryVersion}`);
  }
  return json;
}

export function findLatestVersion(plugin: MarketplacePlugin): MarketplaceVersion | undefined {
  return plugin.versions.find(v => v.version === plugin.latestVersion);
}
