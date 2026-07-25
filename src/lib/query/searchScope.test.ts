// SPDX-License-Identifier: Apache-2.0

import { describe, expect, it } from 'vitest';
import type { TableColumn } from '@/lib/tauri';
import { defaultSearchColumns } from './searchScope';

function column(name: string, data_type: string): TableColumn {
  return { name, data_type, nullable: true, is_primary_key: false };
}

describe('defaultSearchColumns', () => {
  it('keeps text columns and drops everything else', () => {
    const columns = [
      column('id', 'integer'),
      column('name', 'character varying'),
      column('bio', 'text'),
      column('created_at', 'timestamp with time zone'),
      column('avatar', 'bytea'),
    ];
    expect(defaultSearchColumns(columns)).toEqual(['name', 'bio']);
  });

  it('falls back to non-binary columns when the table has no text column', () => {
    const columns = [column('id', 'bigint'), column('score', 'numeric'), column('blob', 'blob')];
    expect(defaultSearchColumns(columns)).toEqual(['id', 'score']);
  });

  it('returns nothing when the schema is unknown', () => {
    expect(defaultSearchColumns(undefined)).toEqual([]);
    expect(defaultSearchColumns([])).toEqual([]);
  });
});
