// SPDX-License-Identifier: BUSL-1.1

import { beforeEach, describe, expect, it, vi } from 'vitest';
import { Driver } from '../connection/drivers';
import { describeTable, listCollections, listNamespaces } from '../tauri';
import { captureBaselineSnapshot, captureSnapshot } from './schemaDiff';

vi.mock('@/lib/tauri', () => ({
  listNamespaces: vi.fn(),
  listCollections: vi.fn(),
  describeTable: vi.fn(),
}));
const namespace = { database: 'fixture', schema: 'public' };
const table = (name: string) => ({ namespace, name, collection_type: 'Table' as const });
const page = (names: string[], total_count = names.length) => ({
  success: true,
  data: { collections: names.map(table), total_count },
});
beforeEach(() => {
  vi.resetAllMocks();
  vi.mocked(listNamespaces).mockResolvedValue({ success: true, namespaces: [namespace] });
  vi.mocked(listCollections).mockResolvedValue(page(['users']));
  vi.mocked(describeTable).mockResolvedValue({
    success: true,
    schema: { columns: [], foreign_keys: [], indexes: [] },
  });
});

describe('schema capture completeness', () => {
  it('refuses namespace failures instead of returning an empty schema', async () => {
    vi.mocked(listNamespaces).mockResolvedValue({
      success: false,
      error: 'synthetic namespace failure',
    });
    await expect(captureSnapshot('session', Driver.Postgres)).rejects.toThrow();
  });
  it('refuses a success response with no namespace payload', async () => {
    vi.mocked(listNamespaces).mockResolvedValue({ success: true });
    await expect(captureSnapshot('session', Driver.Postgres)).rejects.toThrow();
  });
  it('refuses a requested database absent from the namespace response', async () => {
    await expect(captureSnapshot('session', Driver.Postgres, 'missing')).rejects.toThrow();
  });
  it('refuses table-list failures rather than treating existing tables as deleted', async () => {
    vi.mocked(listCollections).mockResolvedValue({
      success: false,
      error: 'synthetic table-list failure',
    });
    await expect(captureSnapshot('session', Driver.Postgres)).rejects.toThrow();
  });
  it('refuses a success response with no collection payload', async () => {
    vi.mocked(listCollections).mockResolvedValue({ success: true });
    await expect(captureSnapshot('session', Driver.Postgres)).rejects.toThrow();
  });
  it('does not treat a short page as complete when the total announces more tables', async () => {
    vi.mocked(listCollections)
      .mockResolvedValueOnce(page(['users'], 2))
      .mockResolvedValueOnce(page(['orders'], 2));
    const result = await captureSnapshot('session', Driver.Postgres);
    expect(Object.keys(result.snapshot.tables)).toEqual([
      'fixture.public.users',
      'fixture.public.orders',
    ]);
    expect(listCollections).toHaveBeenCalledTimes(2);
  });
  it('refuses a later page failure', async () => {
    vi.mocked(listCollections)
      .mockResolvedValueOnce(
        page(
          Array.from({ length: 500 }, (_, i) => `table_${i}`),
          501
        )
      )
      .mockResolvedValueOnce({ success: false });
    await expect(captureSnapshot('session', Driver.Postgres)).rejects.toThrow();
  });
  it('refuses an empty page before the announced total', async () => {
    vi.mocked(listCollections).mockResolvedValue(page([], 1));
    await expect(captureSnapshot('session', Driver.Postgres)).rejects.toThrow();
  });
  it('refuses repeated pages instead of silently overwriting tables', async () => {
    vi.mocked(listCollections).mockResolvedValue(
      page(
        Array.from({ length: 500 }, (_, i) => `table_${i}`),
        1000
      )
    );
    await expect(captureSnapshot('session', Driver.Postgres)).rejects.toThrow();
  });
  it('refuses to declare completeness when the page guard is exhausted', async () => {
    vi.mocked(listCollections).mockImplementation(async (_session, _ns, _search, index) =>
      page([`table_${index}`], 1001)
    );
    await expect(captureSnapshot('session', Driver.Postgres)).rejects.toThrow();
    expect(listCollections).toHaveBeenCalledTimes(1000);
  });
  it('refuses dotted-name key collisions rather than overwriting a table', async () => {
    vi.mocked(listNamespaces).mockResolvedValue({
      success: true,
      namespaces: [
        { database: 'fixture', schema: 'a.b' },
        { database: 'fixture', schema: 'a' },
      ],
    });
    vi.mocked(listCollections)
      .mockResolvedValueOnce(page(['c']))
      .mockResolvedValueOnce(page(['b.c']));
    await expect(captureSnapshot('session', Driver.Postgres)).rejects.toThrow();
  });
  it('keeps an explicitly empty namespace as a valid empty schema', async () => {
    vi.mocked(listCollections).mockResolvedValue(page([]));
    const result = await captureSnapshot('session', Driver.Postgres);
    expect(result.snapshot.tables).toEqual({});
    expect(result.failedTables).toEqual([]);
  });
  it('refuses to publish a baseline when any table description fails', async () => {
    vi.mocked(describeTable).mockResolvedValue({
      success: false,
      error: 'synthetic permission denial',
    });
    await expect(captureBaselineSnapshot('session', Driver.Postgres)).rejects.toThrow();
  });
  it('returns a complete baseline with its original table definition', async () => {
    const baseline = await captureBaselineSnapshot('session', Driver.Postgres);
    expect(baseline.tables['fixture.public.users'].tableName).toBe('users');
  });
  it('keeps individual describe failures visible to comparison callers', async () => {
    vi.mocked(describeTable).mockResolvedValue({
      success: false,
      error: 'synthetic permission denial',
    });
    const result = await captureSnapshot('session', Driver.Postgres);
    expect(result.failedTables).toEqual(['fixture.public.users']);
  });
});
