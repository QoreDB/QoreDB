// SPDX-License-Identifier: BUSL-1.1

import { beforeEach, describe, expect, it, vi } from 'vitest';
import { invoke } from '@/lib/transport';
import {
  clearTableChangelog,
  computeTemporalDiff,
  exportChangelog,
  generateEntryRollbackSql,
  generateRollbackSql,
  getRowHistory,
  getRowStateAt,
  getTableTimeline,
} from './time-travel';

vi.mock('@/lib/transport', () => ({ invoke: vi.fn() }));

const mockedInvoke = vi.mocked(invoke);
const session = 'active-session';
const database = 'shared-name';
const schema = 'public';
const table = 'users';
const pk = { tenant: 'a', id: { $qoreInt: '9007199254740993' } };

beforeEach(() => mockedInvoke.mockReset());

describe('Time Travel IPC scope', () => {
  it('sends the active session on every history read and rollback', async () => {
    await getTableTimeline(session, database, schema, table, { operation: 'update', offset: 50 });
    await getRowHistory(session, database, schema, table, pk);
    await getRowStateAt(session, database, schema, table, pk, '2026-10-03T00:00:00Z');
    await computeTemporalDiff(session, database, schema, table, 'from', 'to');
    await generateRollbackSql(session, database, schema, table, 'target');
    await generateEntryRollbackSql(session, 'selected-event-id');

    expect(mockedInvoke.mock.calls).toHaveLength(6);
    for (const [, payload] of mockedInvoke.mock.calls) {
      expect(payload).toEqual(expect.objectContaining({ sessionId: session }));
      expect(payload).not.toHaveProperty('driverId');
      expect(payload).not.toHaveProperty('connectionId');
    }
    expect(mockedInvoke).toHaveBeenCalledWith('get_row_history', {
      sessionId: session,
      database,
      schema,
      tableName: table,
      primaryKey: pk,
      limit: undefined,
    });
    expect(mockedInvoke).toHaveBeenLastCalledWith('generate_entry_rollback_sql', {
      sessionId: session,
      entryId: 'selected-event-id',
    });
  });

  it('serializes export filters with Rust field names', async () => {
    mockedInvoke.mockResolvedValueOnce('[]');
    expect(
      await exportChangelog(session, {
        tableName: table,
        namespace: { database, schema },
        operation: 'delete',
        fromTimestamp: 'from',
        toTimestamp: 'to',
        primaryKeySearch: 'tenant',
        limit: 25,
      })
    ).toBe('[]');
    expect(mockedInvoke).toHaveBeenCalledWith('export_changelog', {
      sessionId: session,
      filter: {
        table_name: table,
        namespace: { database, schema },
        operation: 'delete',
        from_timestamp: 'from',
        to_timestamp: 'to',
        primary_key_search: 'tenant',
        limit: 25,
      },
    });
  });

  it('keeps the confirmation token and connection scope when clearing', async () => {
    mockedInvoke
      .mockResolvedValueOnce({ token: 'one-shot-token' })
      .mockResolvedValueOnce({ success: true });
    await clearTableChangelog(session, database, schema, table);
    expect(mockedInvoke).toHaveBeenNthCalledWith(1, 'request_confirmation_token', {
      action: 'clear_table_changelog',
    });
    expect(mockedInvoke).toHaveBeenNthCalledWith(2, 'clear_table_changelog', {
      sessionId: session,
      database,
      schema,
      tableName: table,
      confirmationToken: 'one-shot-token',
    });
  });

  it('propagates export failures instead of producing an empty success', async () => {
    mockedInvoke.mockRejectedValueOnce(new Error('Session not found'));
    await expect(exportChangelog(session, { tableName: table })).rejects.toThrow(
      'Session not found'
    );
  });
});
