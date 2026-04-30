// SPDX-License-Identifier: Apache-2.0

import { Plus, Trash2 } from 'lucide-react';
import { useTranslation } from 'react-i18next';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select';
import type { ForeignKeyDef, ReferentialAction } from '@/lib/ddl';
import type { EditableForeignKey } from './createTableTypes';

interface ForeignKeysEditorProps {
  foreignKeys: EditableForeignKey[];
  availableSourceColumns: string[];
  onAdd: () => void;
  onUpdate: (index: number, updates: Partial<ForeignKeyDef>) => void;
  onRemove: (index: number) => void;
  disabled?: boolean;
}

const REFERENTIAL_ACTIONS: ReferentialAction[] = [
  'NO ACTION',
  'CASCADE',
  'SET NULL',
  'SET DEFAULT',
  'RESTRICT',
];

const NO_ACTION_VALUE = '__no_action__';

function parseColumnList(input: string): string[] {
  return input
    .split(',')
    .map(s => s.trim())
    .filter(Boolean);
}

export function ForeignKeysEditor({
  foreignKeys,
  availableSourceColumns,
  onAdd,
  onUpdate,
  onRemove,
  disabled,
}: ForeignKeysEditorProps) {
  const { t } = useTranslation();

  if (disabled) {
    return (
      <p className="text-sm text-muted-foreground">{t('createTable.foreignKeysUnsupported')}</p>
    );
  }

  return (
    <div className="space-y-3">
      <div className="flex items-center justify-between">
        <p className="text-sm font-medium">{t('createTable.foreignKeys')}</p>
        <Button variant="outline" size="sm" onClick={onAdd}>
          <Plus size={14} className="mr-1" />
          {t('createTable.addForeignKey')}
        </Button>
      </div>

      {foreignKeys.length === 0 && (
        <p className="text-xs text-muted-foreground">{t('createTable.noForeignKeys')}</p>
      )}

      <div className="space-y-3">
        {foreignKeys.map((fk, index) => (
          <div key={fk._id} className="bg-muted/30 rounded-md p-3 space-y-2">
            <div className="grid grid-cols-2 gap-3">
              <div className="space-y-1">
                <label htmlFor={`fk-name-${fk._id}`} className="text-xs text-muted-foreground">
                  {t('createTable.constraintName')}
                </label>
                <Input
                  id={`fk-name-${fk._id}`}
                  value={fk.name ?? ''}
                  onChange={e => onUpdate(index, { name: e.target.value || undefined })}
                  placeholder={t('createTable.constraintNamePlaceholder')}
                  className="h-8 text-sm"
                />
              </div>
              <div className="space-y-1">
                <label htmlFor={`fk-cols-${fk._id}`} className="text-xs text-muted-foreground">
                  {t('createTable.fkSourceColumns')}
                </label>
                <Input
                  id={`fk-cols-${fk._id}`}
                  value={fk.columns.join(', ')}
                  onChange={e => onUpdate(index, { columns: parseColumnList(e.target.value) })}
                  placeholder={availableSourceColumns.slice(0, 2).join(', ') || 'col_a, col_b'}
                  className="h-8 text-sm"
                  list={`fk-cols-list-${fk._id}`}
                />
                <datalist id={`fk-cols-list-${fk._id}`}>
                  {availableSourceColumns.map(c => (
                    <option key={c} value={c} />
                  ))}
                </datalist>
              </div>
            </div>

            <div className="grid grid-cols-3 gap-3">
              <div className="space-y-1">
                <label
                  htmlFor={`fk-ref-schema-${fk._id}`}
                  className="text-xs text-muted-foreground"
                >
                  {t('createTable.fkRefSchema')}
                </label>
                <Input
                  id={`fk-ref-schema-${fk._id}`}
                  value={fk.refSchema ?? ''}
                  onChange={e => onUpdate(index, { refSchema: e.target.value || undefined })}
                  placeholder="public"
                  className="h-8 text-sm"
                />
              </div>
              <div className="space-y-1">
                <label htmlFor={`fk-ref-table-${fk._id}`} className="text-xs text-muted-foreground">
                  {t('createTable.fkRefTable')}
                </label>
                <Input
                  id={`fk-ref-table-${fk._id}`}
                  value={fk.refTable}
                  onChange={e => onUpdate(index, { refTable: e.target.value })}
                  placeholder="other_table"
                  className="h-8 text-sm"
                />
              </div>
              <div className="space-y-1">
                <label htmlFor={`fk-ref-cols-${fk._id}`} className="text-xs text-muted-foreground">
                  {t('createTable.fkRefColumns')}
                </label>
                <Input
                  id={`fk-ref-cols-${fk._id}`}
                  value={fk.refColumns.join(', ')}
                  onChange={e => onUpdate(index, { refColumns: parseColumnList(e.target.value) })}
                  placeholder="id"
                  className="h-8 text-sm"
                />
              </div>
            </div>

            <div className="grid grid-cols-3 gap-3 items-end">
              <div className="space-y-1">
                <span className="text-xs text-muted-foreground">{t('createTable.fkOnDelete')}</span>
                <Select
                  value={fk.onDelete ?? NO_ACTION_VALUE}
                  onValueChange={value =>
                    onUpdate(index, {
                      onDelete:
                        value === NO_ACTION_VALUE ? undefined : (value as ReferentialAction),
                    })
                  }
                >
                  <SelectTrigger className="h-8 text-sm">
                    <SelectValue />
                  </SelectTrigger>
                  <SelectContent>
                    <SelectItem value={NO_ACTION_VALUE}>{t('common.none')}</SelectItem>
                    {REFERENTIAL_ACTIONS.map(action => (
                      <SelectItem key={action} value={action}>
                        {action}
                      </SelectItem>
                    ))}
                  </SelectContent>
                </Select>
              </div>
              <div className="space-y-1">
                <span className="text-xs text-muted-foreground">{t('createTable.fkOnUpdate')}</span>
                <Select
                  value={fk.onUpdate ?? NO_ACTION_VALUE}
                  onValueChange={value =>
                    onUpdate(index, {
                      onUpdate:
                        value === NO_ACTION_VALUE ? undefined : (value as ReferentialAction),
                    })
                  }
                >
                  <SelectTrigger className="h-8 text-sm">
                    <SelectValue />
                  </SelectTrigger>
                  <SelectContent>
                    <SelectItem value={NO_ACTION_VALUE}>{t('common.none')}</SelectItem>
                    {REFERENTIAL_ACTIONS.map(action => (
                      <SelectItem key={action} value={action}>
                        {action}
                      </SelectItem>
                    ))}
                  </SelectContent>
                </Select>
              </div>
              <div className="flex justify-end">
                <Button
                  variant="ghost"
                  size="icon"
                  className="h-8 w-8 text-destructive hover:text-destructive"
                  onClick={() => onRemove(index)}
                  aria-label={t('common.remove')}
                >
                  <Trash2 size={14} />
                </Button>
              </div>
            </div>
          </div>
        ))}
      </div>
    </div>
  );
}
