// SPDX-License-Identifier: Apache-2.0

import { Input } from '@/components/ui/input';
import type { TableColumn } from '../../lib/tauri';
import { Checkbox } from '../ui/checkbox';
import { Label } from '../ui/label';

interface RowModalSchemaFieldsProps {
  columns: TableColumn[];
  formData: Record<string, string>;
  nulls: Record<string, boolean>;
  readOnly: boolean;
  onNullToggle: (col: string, isNull: boolean) => void;
  onInputChange: (col: string, value: string) => void;
}

export function RowModalSchemaFields({
  columns,
  formData,
  nulls,
  readOnly,
  onNullToggle,
  onInputChange,
}: RowModalSchemaFieldsProps) {
  return (
    <div className="grid gap-4 py-4">
      {columns.map(col => (
        <div key={col.name} className="grid gap-2">
          <div className="flex items-center justify-between">
            <Label htmlFor={col.name} className="flex items-center gap-2">
              {col.name}
              <span className="text-xs text-muted-foreground font-mono font-normal">
                ({col.data_type})
              </span>
              {col.is_primary_key && (
                <span className="text-xs bg-yellow-100 text-yellow-800 px-1 rounded dark:bg-yellow-900 dark:text-yellow-100">
                  PK
                </span>
              )}
            </Label>

            {col.nullable && (
              <div className="flex items-center space-x-2">
                <Checkbox
                  id={`${col.name}-null`}
                  checked={nulls[col.name] || false}
                  onCheckedChange={checked => onNullToggle(col.name, checked as boolean)}
                  disabled={readOnly}
                />
                <label
                  htmlFor={`${col.name}-null`}
                  className="text-xs font-medium leading-none peer-disabled:cursor-not-allowed peer-disabled:opacity-70 text-muted-foreground"
                >
                  NULL
                </label>
              </div>
            )}
          </div>

          <Input
            id={col.name}
            value={formData[col.name] || ''}
            onChange={e => onInputChange(col.name, e.target.value)}
            disabled={nulls[col.name] || readOnly}
            placeholder={col.default_value ? `Default: ${col.default_value}` : ''}
            className="font-mono text-sm"
          />
        </div>
      ))}
    </div>
  );
}
