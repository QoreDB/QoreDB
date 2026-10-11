// SPDX-License-Identifier: Apache-2.0

import { beforeEach, describe, expect, it, vi } from 'vitest';
import { invoke } from '@/lib/transport';
import { applySandboxChanges, type SandboxChangeDto } from './sandbox';

vi.mock('@/lib/transport', () => ({ invoke: vi.fn() }));
const mockedInvoke = vi.mocked(invoke);
const changes: SandboxChangeDto[] = [
  {
    change_type: 'update',
    namespace: { database: 'fixture' },
    table_name: 'items',
    primary_key: { columns: { id: { $qoreInt: '9007199254740993' } } },
    new_values: { name: 'new' },
  },
];
beforeEach(() => {
  mockedInvoke.mockReset();
});

describe('sandbox batch IPC', () => {
  it('requires an explicit acknowledgement and preserves exact values', async () => {
    mockedInvoke.mockResolvedValue({
      success: true,
      applied_count: 1,
      applied_indices: [0],
      outcome_unknown: false,
      failed_changes: [],
    });
    await applySandboxChanges('session', changes);
    expect(mockedInvoke).toHaveBeenLastCalledWith('apply_sandbox_changes', {
      sessionId: 'session',
      changes,
      useTransaction: true,
      acknowledgedDangerous: false,
    });
    await applySandboxChanges('session', changes, true, true);
    expect(mockedInvoke).toHaveBeenLastCalledWith('apply_sandbox_changes', {
      sessionId: 'session',
      changes,
      useTransaction: true,
      acknowledgedDangerous: true,
    });
  });

  it('preserves partial confirmation independently of an uncertain failed write', async () => {
    const partial = {
      success: false,
      applied_count: 1,
      applied_indices: [0],
      outcome_unknown: true,
      failed_changes: [{ index: 1, error: 'connection lost' }],
    };
    mockedInvoke.mockResolvedValue(partial);
    expect(await applySandboxChanges('session', changes, false)).toEqual(partial);
  });

  it('treats a lost response as unknown and never retries the mutation', async () => {
    mockedInvoke.mockRejectedValue(new Error('IPC disconnected'));
    expect(await applySandboxChanges('session', changes)).toEqual({
      success: false,
      applied_count: 0,
      applied_indices: [],
      outcome_unknown: true,
      error: 'IPC disconnected',
      failed_changes: [],
    });
    expect(mockedInvoke).toHaveBeenCalledTimes(1);
  });
});
