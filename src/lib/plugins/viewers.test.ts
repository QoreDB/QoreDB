// SPDX-License-Identifier: Apache-2.0

import { describe, expect, it } from 'vitest';
import type { CellContribution } from './types';
import { findCellContributions, findViewerFor } from './viewers';

const contributions: CellContribution[] = [
  { id: 'acme::any', label: 'Any column' },
  { id: 'acme::jwt', label: 'Decode JWT', match: { namePattern: '*_token' } },
  { id: 'acme::geo', label: 'Geometry', match: { columnType: 'GEOMETRY' } },
];

describe('findCellContributions', () => {
  it('keeps unmatched actions for every column', () => {
    expect(findCellContributions({ name: 'id', columnType: 'int4' }, contributions)).toEqual([
      contributions[0],
    ]);
  });

  it('matches name globs and case-insensitive column types', () => {
    const ids = (column: { name: string; columnType: string }) =>
      findCellContributions(column, contributions).map(c => c.id);
    expect(ids({ name: 'refresh_token', columnType: 'text' })).toEqual(['acme::any', 'acme::jwt']);
    expect(ids({ name: 'shape', columnType: 'geometry' })).toEqual(['acme::any', 'acme::geo']);
    expect(ids({ name: 'token_refresh', columnType: 'text' })).toEqual(['acme::any']);
  });
});

describe('findViewerFor', () => {
  it('returns the first matching viewer', () => {
    const viewer = findViewerFor({ name: 'payload', columnType: 'jsonb' }, [
      { id: 'a::json', match: { columnType: 'jsonb' }, renderer: 'json-tree' },
      { id: 'b::json', match: { namePattern: 'payload' }, renderer: 'json-tree' },
    ]);
    expect(viewer?.id).toBe('a::json');
  });
});
