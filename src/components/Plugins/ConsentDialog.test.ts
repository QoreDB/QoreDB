// SPDX-License-Identifier: Apache-2.0

import { describe, expect, it } from 'vitest';
import type { InstalledPlugin } from '@/lib/plugins';
import { defaultInitialGrants, requestedCaps } from './ConsentDialog';

function makePlugin(): InstalledPlugin {
  return {
    manifest: {
      id: 'acme.demo',
      name: 'Demo',
      version: '1.0.0',
      runtime: {
        entry: 'plugin.wasm',
        capabilities: {
          log: true,
          notify: true,
          storage: true,
          queryRead: true,
          http: { allowedHosts: ['api.example.com'] },
          fs: true,
          secrets: ['API_KEY'],
        },
      },
      contributes: {
        snippets: [],
        connectionTemplates: [],
        themes: [],
        resultViewers: [],
        commands: [],
        cellActions: [],
        cellRenderers: [],
      },
    },
    source: 'local',
    enabled: true,
    consent: { granted: [] },
  } as unknown as InstalledPlugin;
}

describe('ConsentDialog defaultInitialGrants', () => {
  it('pre-selects only benign capabilities (log, storage) on initial install', () => {
    const plugin = makePlugin();
    expect(requestedCaps(plugin)).toEqual([
      'log',
      'notify',
      'storage',
      'queryRead',
      'http',
      'fs',
      'secrets',
    ]);
    expect(defaultInitialGrants(plugin)).toEqual(['log', 'storage']);
  });

  it('respects explicit initialGrants when editing an already installed plugin', () => {
    const plugin = makePlugin();
    expect(defaultInitialGrants(plugin, ['log', 'queryRead'])).toEqual(['log', 'queryRead']);
    expect(defaultInitialGrants(plugin, [])).toEqual([]);
  });
});
