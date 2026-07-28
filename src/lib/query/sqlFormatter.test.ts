// SPDX-License-Identifier: Apache-2.0

import { describe, expect, it } from 'vitest';
import { Driver } from '../connection/drivers';
import { formatSql } from './sqlFormatter';

describe('formatSql', () => {
  it('loads the formatter and applies the selected dialect', async () => {
    const formatted = await formatSql('select id, name from users where id = 1', Driver.Postgres);

    expect(formatted).toContain('SELECT');
    expect(formatted).toContain('FROM');
    expect(formatted).not.toContain('select id');
  });
});
