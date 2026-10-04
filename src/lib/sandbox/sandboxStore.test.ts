// SPDX-License-Identifier: Apache-2.0

import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { acknowledgeSandboxChanges, getSandboxSession, saveSandboxState } from './sandboxStore';
import type { SandboxChange } from './sandboxTypes';

function change(id: string, name = 'new'): SandboxChange {
  return {
    id,
    sessionId: 'session',
    timestamp: 1,
    type: 'update',
    namespace: { database: 'fixture' },
    tableName: 'items',
    primaryKey: { columns: { id } },
    newValues: { name },
  };
}
function save(changes: SandboxChange[]) {
  saveSandboxState({
    sessions: { session: { sessionId: 'session', isActive: true, activatedAt: 1, changes } },
  });
}
beforeEach(() => {
  const values = new Map<string, string>();
  vi.stubGlobal('localStorage', {
    getItem: (key: string) => values.get(key) ?? null,
    setItem: (key: string, value: string) => values.set(key, value),
  });
});
afterEach(() => vi.unstubAllGlobals());

describe('sandbox confirmed revisions', () => {
  it('keeps all pending edits after rollback or unknown commit', () => {
    const submitted = [change('one'), change('two')];
    save(submitted);
    acknowledgeSandboxChanges('session', submitted, []);
    expect(getSandboxSession('session').changes).toEqual(submitted);
  });
  it('removes only the exact confirmed indices, preserving failed and newly queued edits', () => {
    const submitted = [change('one'), change('two')];
    save([...submitted, change('new')]);
    acknowledgeSandboxChanges('session', submitted, [0]);
    expect(getSandboxSession('session').changes.map(c => c.id)).toEqual(['two', 'new']);
  });
  it('preserves a revision edited while its earlier version was in flight', () => {
    const submitted = [change('one')];
    save([change('one', 'edited meanwhile')]);
    acknowledgeSandboxChanges('session', submitted, [0]);
    expect(getSandboxSession('session').changes[0].newValues).toEqual({ name: 'edited meanwhile' });
  });
});
