// SPDX-License-Identifier: Apache-2.0

import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { notify } from '../notify';
import {
  acknowledgeSandboxChanges,
  createBulkInsertChanges,
  getSandboxSession,
  saveSandboxState,
} from './sandboxStore';
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

  it('stages bulk insert rows in a single batch for Data Generator in Sandbox mode', () => {
    const created = createBulkInsertChanges('session', { database: 'fixture' }, 'items', [
      { id: 1, name: 'Alpha' },
      { id: 2, name: 'Bravo' },
    ]);
    expect(created).toHaveLength(2);
    const session = getSandboxSession('session');
    expect(session.changes).toHaveLength(2);
    expect(session.changes.map(c => [c.type, c.newValues])).toEqual([
      ['insert', { id: 1, name: 'Alpha' }],
      ['insert', { id: 2, name: 'Bravo' }],
    ]);
  });

  it('evicts backup storage on quota exceeded and notifies if quota remains full', () => {
    const errorSpy = vi.spyOn(notify, 'error').mockImplementation(() => {});
    const values = new Map<string, string>([['qoredb_sandbox_backup', 'large-backup']]);
    let failCount = 0;
    vi.stubGlobal('localStorage', {
      getItem: (key: string) => values.get(key) ?? null,
      removeItem: (key: string) => values.delete(key),
      setItem: (key: string, value: string) => {
        if (failCount === 0) {
          failCount += 1;
          throw new DOMException('QuotaExceededError', 'QuotaExceededError');
        }
        values.set(key, value);
      },
    });

    const recovered = saveSandboxState({
      sessions: {
        session: { sessionId: 'session', isActive: true, activatedAt: 1, changes: [change('one')] },
      },
    });
    expect(recovered).toBe(true);
    expect(values.has('qoredb_sandbox_backup')).toBe(false);
    expect(errorSpy).not.toHaveBeenCalled();

    vi.stubGlobal('localStorage', {
      getItem: (key: string) => values.get(key) ?? null,
      removeItem: (key: string) => values.delete(key),
      setItem: () => {
        throw new DOMException('QuotaExceededError', 'QuotaExceededError');
      },
    });

    const failed = saveSandboxState({ sessions: {} });
    expect(failed).toBe(false);
    expect(errorSpy).toHaveBeenCalledTimes(1);
    errorSpy.mockRestore();
  });
});
