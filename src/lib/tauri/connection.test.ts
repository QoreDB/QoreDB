// SPDX-License-Identifier: Apache-2.0

import { beforeEach, describe, expect, it, vi } from 'vitest';
import {
  setActiveWorkspace,
  setRecentWorkspaces,
  setWorkspaceLoading,
} from '@/lib/stores/workspaceStore';
import { invoke } from '@/lib/transport';
import { connectSavedConnection } from './connection';
import type { ConnectionResponse } from './types';

vi.mock('@/lib/transport', () => ({ invoke: vi.fn() }));
vi.mock('@/i18n', () => ({ default: { t: () => 'Context changed' } }));
const mockedInvoke = vi.mocked(invoke);
const connected = { success: true, session_id: 'session-a' };

beforeEach(() => {
  mockedInvoke.mockReset();
  setActiveWorkspace(null, 'a');
});

describe('saved connection workspace scope', () => {
  it('returns a current session with the requested project, including the default workspace', async () => {
    mockedInvoke.mockResolvedValue(connected);
    for (const projectId of ['a', 'default']) {
      setActiveWorkspace(null, projectId);
      expect(await connectSavedConnection(projectId, 'same-id')).toEqual(connected);
      expect(mockedInvoke).toHaveBeenLastCalledWith('connect_saved_connection', {
        projectId,
        connectionId: 'same-id',
      });
    }
  });

  it('rejects a stale project or a transition before invoking the backend', async () => {
    await expect(connectSavedConnection('b', 'same-id')).rejects.toThrow('Context changed');
    setWorkspaceLoading(true);
    await expect(connectSavedConnection('a', 'same-id')).rejects.toThrow('Context changed');
    expect(mockedInvoke).not.toHaveBeenCalled();
  });

  for (const change of ['switch', 'roundtrip', 'failed-transition']) {
    it(`closes a late session without returning it after ${change}`, async () => {
      let resolve!: (value: ConnectionResponse) => void;
      mockedInvoke.mockImplementationOnce(
        () =>
          new Promise<ConnectionResponse>(done => {
            resolve = done;
          })
      );
      mockedInvoke.mockResolvedValue({ success: true });
      const pending = connectSavedConnection('a', 'same-id');
      if (change === 'failed-transition') {
        setWorkspaceLoading(true);
        setWorkspaceLoading(false);
      } else {
        setActiveWorkspace(null, 'b');
        if (change === 'roundtrip') setActiveWorkspace(null, 'a');
      }
      resolve(connected);
      await expect(pending).rejects.toThrow('Context changed');
      expect(mockedInvoke).toHaveBeenLastCalledWith('disconnect', { sessionId: 'session-a' });
      expect(mockedInvoke).toHaveBeenCalledTimes(2);
    });
  }

  for (const failure of ['response', 'transport']) {
    it(`does not expose a stale session when disconnect fails through ${failure}`, async () => {
      mockedInvoke.mockImplementationOnce(async () => {
        setActiveWorkspace(null, 'b');
        return connected;
      });
      if (failure === 'response') mockedInvoke.mockResolvedValue({ success: false });
      else mockedInvoke.mockRejectedValue(new Error('Synthetic disconnect failure'));
      const warning = vi.spyOn(console, 'warn').mockImplementation(() => {});
      try {
        await expect(connectSavedConnection('a', 'same-id')).rejects.toThrow('Context changed');
        expect(warning).toHaveBeenCalledOnce();
      } finally {
        warning.mockRestore();
      }
    });
  }

  it('preserves ordinary errors and does not invalidate a recent-workspace refresh', async () => {
    mockedInvoke.mockImplementationOnce(async () => {
      setRecentWorkspaces([]);
      return connected;
    });
    expect(await connectSavedConnection('a', 'same-id')).toEqual(connected);
    const failure = { success: false, error: 'Synthetic connection failure' };
    mockedInvoke.mockResolvedValueOnce(failure);
    expect(await connectSavedConnection('a', 'same-id')).toEqual(failure);
    mockedInvoke.mockRejectedValueOnce(new Error('Synthetic IPC failure'));
    await expect(connectSavedConnection('a', 'same-id')).rejects.toThrow('Synthetic IPC failure');
  });
});
