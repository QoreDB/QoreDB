// SPDX-License-Identifier: BUSL-1.1

import { describe, expect, it } from 'vitest';
import type { TableSchema } from '../../lib/tauri';
import { buildInitialRowModalState, buildRowUpdateData, computePreview } from './rowModalUtils';

describe('row updates with masked values', () => {
  const schema = {
    columns: [
      { name: 'id', data_type: 'integer', nullable: false, is_primary_key: true },
      { name: 'email', data_type: 'text', nullable: false, is_primary_key: false },
      { name: 'salary', data_type: 'integer', nullable: true, is_primary_key: false },
      { name: 'city', data_type: 'text', nullable: true, is_primary_key: false },
    ],
    primary_key: ['id'],
    indexes: [],
    foreign_keys: [],
  } as unknown as TableSchema;
  const initialData = { id: 1, email: '••••••', salary: '••••••', city: 'Paris' };
  const maskedColumns = new Set(['email', 'salary']);

  it('sends only the changed unmasked field and agrees with the preview', () => {
    const state = buildInitialRowModalState({ schema, initialData, mode: 'update' });
    state.formData.city = 'Lyon';
    const args = { columns: schema.columns, initialData, maskedColumns, ...state };
    expect(buildRowUpdateData(args)).toEqual({ city: 'Lyon' });
    expect(
      computePreview({ ...args, schema, effectiveColumns: schema.columns, mode: 'update' })
    ).toEqual({
      type: 'update',
      changes: [{ key: 'city', previous: 'Paris', next: 'Lyon' }],
    });
  });

  it('excludes masked fields even if their form values were changed', () => {
    const state = buildInitialRowModalState({ schema, initialData, mode: 'update' });
    state.formData.email = 'replacement';
    state.formData.salary = '100';
    expect(
      buildRowUpdateData({ columns: schema.columns, initialData, maskedColumns, ...state })
    ).toEqual({});
  });

  it('never parses unchanged placeholders back into updates after masking metadata changes', () => {
    const state = buildInitialRowModalState({ schema, initialData, mode: 'update' });
    state.formData.city = 'Lyon';
    expect(buildRowUpdateData({ columns: schema.columns, initialData, ...state })).toEqual({
      city: 'Lyon',
    });
  });

  it('preserves explicit nulls and primary key changes for unmasked columns', () => {
    const state = buildInitialRowModalState({ schema, initialData, mode: 'update' });
    state.nulls.city = true;
    state.formData.id = '2';
    expect(
      buildRowUpdateData({ columns: schema.columns, initialData, maskedColumns, ...state })
    ).toEqual({ id: 2, city: null });
  });
});
