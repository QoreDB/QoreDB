// SPDX-License-Identifier: Apache-2.0

import { Driver } from '../../lib/drivers';
import { TableColumn, TableSchema, Value } from '../../lib/tauri';

export type RowModalMode = 'insert' | 'update';

export type RowModalFormData = Record<string, string>;
export type RowModalNulls = Record<string, boolean>;

export interface RowModalPreviewInsert {
  type: 'insert';
  values: { key: string; value: Value }[];
}

export interface RowModalPreviewUpdate {
  type: 'update';
  changes: { key: string; previous: Value; next: Value }[];
}

export type RowModalPreview = RowModalPreviewInsert | RowModalPreviewUpdate;

export interface InitialRowModalState {
  formData: RowModalFormData;
  nulls: RowModalNulls;
  extraColumns: TableColumn[];
}

export function buildInitialRowModalState({
  schema,
  initialData,
  mode,
  driver,
}: {
  schema: TableSchema;
  initialData?: Record<string, Value>;
  mode: RowModalMode;
  driver?: Driver;
}): InitialRowModalState {
  const initialForm: RowModalFormData = {};
  const initialNulls: RowModalNulls = {};
  const initialExtraCols: TableColumn[] = [];

  schema.columns.forEach(col => {
    const val = initialData?.[col.name];

    if (mode === 'update' && val !== undefined) {
      if (val === null) {
        initialNulls[col.name] = true;
        initialForm[col.name] = '';
      } else {
        initialNulls[col.name] = false;
        initialForm[col.name] = String(val);
      }
    } else {
      initialForm[col.name] = '';
      if (col.nullable && !col.default_value) {
        initialNulls[col.name] = true;
      } else {
        initialNulls[col.name] = false;
      }
    }
  });

  if (mode === 'update' && initialData && driver === Driver.Mongodb) {
    const schemaColNames = new Set(schema.columns.map(c => c.name));
    Object.keys(initialData).forEach(key => {
      if (!schemaColNames.has(key)) {
        const val = initialData[key];
        const inferredType = typeof val;
        let dataType = 'string';
        if (inferredType === 'boolean') dataType = 'boolean';
        else if (inferredType === 'number') dataType = 'double';
        else if (inferredType === 'object' && val !== null) dataType = 'json';

        initialExtraCols.push({
          name: key,
          data_type: dataType,
          nullable: true,
          is_primary_key: false,
        });

        if (val === null) {
          initialNulls[key] = true;
          initialForm[key] = '';
        } else {
          initialNulls[key] = false;
          initialForm[key] = typeof val === 'object' ? JSON.stringify(val) : String(val);
        }
      }
    });
  }

  return { formData: initialForm, nulls: initialNulls, extraColumns: initialExtraCols };
}

export function parseValue(value: string, dataType: string): Value {
  const type = dataType.toLowerCase();
  if (
    type.includes('int') ||
    type.includes('serial') ||
    type.includes('float') ||
    type.includes('double') ||
    type.includes('numeric')
  ) {
    if (value === '' || value === undefined) return null;
    return Number(value);
  }
  if (type.includes('bool')) {
    return value === 'true' || value === '1' || value === 'yes';
  }
  if (type.includes('json')) {
    try {
      return JSON.parse(value);
    } catch {
      return value;
    }
  }
  return value;
}

export function formatPreviewValue(value: Value): string {
  if (value === null) return 'NULL';
  if (typeof value === 'boolean') return value ? 'true' : 'false';
  if (typeof value === 'number') return String(value);
  if (typeof value === 'string') return value;
  return JSON.stringify(value);
}

export function buildColumnsData({
  columns,
  formData,
  nulls,
}: {
  columns: TableColumn[];
  formData: RowModalFormData;
  nulls: RowModalNulls;
}): Record<string, Value> {
  const data: Record<string, Value> = {};

  columns.forEach(col => {
    if (nulls[col.name]) {
      data[col.name] = null;
      return;
    }
    const rawVal = formData[col.name];
    if (rawVal === '' && col.default_value) {
      return;
    }
    data[col.name] = parseValue(rawVal, col.data_type);
  });

  return data;
}

export function computePreview({
  mode,
  schema,
  initialData,
  effectiveColumns,
  formData,
  nulls,
}: {
  mode: RowModalMode;
  schema: TableSchema;
  initialData?: Record<string, Value>;
  effectiveColumns: TableColumn[];
  formData: RowModalFormData;
  nulls: RowModalNulls;
}): RowModalPreview {
  const data = buildColumnsData({ columns: effectiveColumns, formData, nulls });

  if (mode === 'insert') {
    return {
      type: 'insert',
      values: Object.entries(data).map(([key, value]) => ({
        key,
        value,
      })),
    };
  }

  const changes = schema.columns.flatMap(col => {
    if (!(col.name in data)) return [];
    const nextValue = data[col.name];
    const prevValue = initialData?.[col.name];
    const prevSerialized = JSON.stringify(prevValue ?? null);
    const nextSerialized = JSON.stringify(nextValue ?? null);
    if (prevSerialized === nextSerialized) return [];

    return [
      {
        key: col.name,
        previous: prevValue ?? null,
        next: nextValue ?? null,
      },
    ];
  });

  return { type: 'update', changes };
}
