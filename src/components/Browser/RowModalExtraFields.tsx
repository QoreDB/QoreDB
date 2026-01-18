import { Trash } from 'lucide-react';
import { TableColumn } from '../../lib/tauri';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { Label } from '../ui/label';
import { Checkbox } from '../ui/checkbox';

interface RowModalExtraFieldsProps {
  columns: TableColumn[];
  formData: Record<string, string>;
  nulls: Record<string, boolean>;
  readOnly: boolean;
  title: string;
  onNullToggle: (col: string, isNull: boolean) => void;
  onInputChange: (col: string, value: string) => void;
  onRemove: (col: string) => void;
}

export function RowModalExtraFields({
  columns,
  formData,
  nulls,
  readOnly,
  title,
  onNullToggle,
  onInputChange,
  onRemove,
}: RowModalExtraFieldsProps) {
  if (columns.length === 0) return null;

  return (
    <div className="grid gap-4 py-4 border-t border-border mt-2">
      <div className="text-xs font-semibold uppercase text-muted-foreground">{title}</div>
      {columns.map((col) => (
        <div key={col.name} className="grid gap-2">
          <div className="flex items-center justify-between">
            <Label htmlFor={col.name} className="flex items-center gap-2">
              {col.name}
              <span className="text-xs text-muted-foreground font-mono font-normal">
                ({col.data_type})
              </span>
            </Label>

            <div className="flex items-center gap-2">
              {col.nullable && (
                <div className="flex items-center space-x-2">
                  <Checkbox
                    id={`${col.name}-null`}
                    checked={nulls[col.name] || false}
                    onCheckedChange={(checked) => onNullToggle(col.name, checked as boolean)}
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
              <Button
                type="button"
                variant="ghost"
                size="icon"
                className="h-6 w-6 text-muted-foreground hover:text-destructive"
                onClick={() => onRemove(col.name)}
                disabled={readOnly}
              >
                <Trash size={12} />
              </Button>
            </div>
          </div>

          <Input
            id={col.name}
            value={formData[col.name] || ''}
            onChange={(e) => onInputChange(col.name, e.target.value)}
            disabled={nulls[col.name] || readOnly}
            className="font-mono text-sm"
          />
        </div>
      ))}
    </div>
  );
}
