// SPDX-License-Identifier: Apache-2.0

import { describe, expect, it } from 'vitest';
import type { TableSchema } from '@/lib/tauri';
import { searchCost, sortCost } from './indexCost';

const schema: TableSchema = {
  columns: [],
  foreign_keys: [],
  indexes: [
    { name: 'pk', columns: ['id'], is_unique: true, is_primary: true, index_type: 'btree' },
    {
      name: 'by_name',
      columns: ['name'],
      is_unique: false,
      is_primary: false,
      index_type: 'btree',
    },
    {
      name: 'by_org_then_city',
      columns: ['org', 'city'],
      is_unique: false,
      is_primary: false,
      index_type: 'btree',
    },
    {
      name: 'by_hash',
      columns: ['token'],
      is_unique: false,
      is_primary: false,
      index_type: 'hash',
    },
  ],
};

describe('searchCost', () => {
  it('always scans in substring mode, indexed column or not', () => {
    expect(searchCost(schema, ['name'], 'contains')).toBe('leading_wildcard');
  });

  it('reaches an index on a single indexed column in prefix mode', () => {
    expect(searchCost(schema, ['name'], 'starts_with')).toBeNull();
  });

  it('scans on a column with no usable index', () => {
    expect(searchCost(schema, ['bio'], 'starts_with')).toBe('no_index');
    // Hash indexes answer equality only.
    expect(searchCost(schema, ['token'], 'starts_with')).toBe('no_index');
    // Second position of a composite index is not a usable prefix.
    expect(searchCost(schema, ['city'], 'starts_with')).toBe('no_index');
    expect(searchCost(schema, ['org'], 'starts_with')).toBeNull();
  });

  it('scans when the OR spans several columns', () => {
    expect(searchCost(schema, ['name', 'org'], 'starts_with')).toBe('multi_column');
  });

  it('reports no index when the scope is unknown', () => {
    expect(searchCost(schema, undefined, 'starts_with')).toBe('no_index');
    expect(searchCost(null, ['name'], 'starts_with')).toBe('no_index');
  });
});

describe('sortCost', () => {
  it('follows an index when the column leads one', () => {
    expect(sortCost(schema, 'id')).toBeNull();
    expect(sortCost(schema, 'org')).toBeNull();
  });

  it('sorts the table otherwise', () => {
    expect(sortCost(schema, 'city')).toBe('no_index');
    expect(sortCost(schema, 'bio')).toBe('no_index');
    expect(sortCost(null, 'id')).toBe('no_index');
  });
});
