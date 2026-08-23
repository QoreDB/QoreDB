// SPDX-License-Identifier: Apache-2.0

import { describe, expect, it } from 'vitest';
import { getDriverDocsPath } from './externalLinks';

describe('getDriverDocsPath', () => {
  it.each([
    ['postgres', 'connections/postgresql'],
    ['supabase', 'connections/postgresql'],
    ['mariadb', 'connections/mysql'],
    ['documentdb', 'connections/mongodb'],
    ['valkey', 'connections/redis'],
    ['motherduck', 'connections/duckdb'],
    ['opensearch', 'connections/opensearch'],
  ])('maps %s to its documentation page', (driver, expected) => {
    expect(getDriverDocsPath(driver)).toBe(expected);
  });
});
