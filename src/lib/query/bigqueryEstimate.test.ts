// SPDX-License-Identifier: Apache-2.0

import { beforeEach, describe, expect, it, vi } from 'vitest';
import { executeQuery } from '../tauri/query';
import { estimateBigQueryScan } from './bigqueryEstimate';

vi.mock('../tauri/query', () => ({ executeQuery: vi.fn() }));

describe('estimateBigQueryScan', () => {
  beforeEach(() => vi.resetAllMocks());

  it('only requests a dry run in the selected project and dataset', async () => {
    vi.mocked(executeQuery).mockResolvedValue({
      success: true,
      result: {
        columns: [{ name: 'total_bytes_processed', data_type: 'INT64', nullable: true }],
        rows: [{ values: [1024] }],
        execution_time_ms: 0,
      },
    });
    const namespace = { database: 'data-project', schema: 'sales' };
    expect(await estimateBigQueryScan('session', 'SELECT * FROM orders', namespace)).toBe(1024);
    expect(executeQuery).toHaveBeenCalledExactlyOnceWith(
      'session',
      'EXPLAIN SELECT * FROM orders',
      { namespace }
    );
  });

  it.each([
    null,
    -1,
    Number.NaN,
    Number.POSITIVE_INFINITY,
    'invalid',
  ])('does not report an unavailable estimate (%s) as a free query', async bytes => {
    vi.mocked(executeQuery).mockResolvedValue({
      success: true,
      result: {
        columns: [{ name: 'total_bytes_processed', data_type: 'INT64', nullable: true }],
        rows: [{ values: [bytes] }],
        execution_time_ms: 0,
      },
    });
    expect(await estimateBigQueryScan('session', 'SELECT 1')).toBeNull();
  });

  it('propagates dry-run failures without executing the query', async () => {
    vi.mocked(executeQuery).mockResolvedValue({ success: false, error: 'Access denied' });
    await expect(estimateBigQueryScan('session', 'DELETE FROM orders')).rejects.toThrow(
      'Access denied'
    );
    expect(executeQuery).toHaveBeenCalledTimes(1);
  });
});
