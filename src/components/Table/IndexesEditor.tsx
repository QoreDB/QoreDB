// SPDX-License-Identifier: Apache-2.0

import { Plus, Trash2 } from 'lucide-react';
import { useTranslation } from 'react-i18next';
import { Button } from '@/components/ui/button';
import { Checkbox } from '@/components/ui/checkbox';
import { Input } from '@/components/ui/input';
import type { DdlCapabilities, IndexDef } from '@/lib/ddl';
import type { EditableIndex } from './createTableTypes';

interface IndexesEditorProps {
  indexes: EditableIndex[];
  availableSourceColumns: string[];
  capabilities: DdlCapabilities;
  onAdd: () => void;
  onUpdate: (index: number, updates: Partial<IndexDef>) => void;
  onRemove: (index: number) => void;
}

function parseColumnList(input: string): string[] {
  return input
    .split(',')
    .map(s => s.trim())
    .filter(Boolean);
}

export function IndexesEditor({
  indexes,
  availableSourceColumns,
  capabilities,
  onAdd,
  onUpdate,
  onRemove,
}: IndexesEditorProps) {
  const { t } = useTranslation();

  if (!capabilities.supportsIndexes) {
    return <p className="text-sm text-muted-foreground">{t('createTable.indexesUnsupported')}</p>;
  }

  return (
    <div className="space-y-3">
      <div className="flex items-center justify-between">
        <p className="text-sm font-medium">{t('createTable.indexes')}</p>
        <Button variant="outline" size="sm" onClick={onAdd}>
          <Plus size={14} className="mr-1" />
          {t('createTable.addIndex')}
        </Button>
      </div>

      {indexes.length === 0 && (
        <p className="text-xs text-muted-foreground">{t('createTable.noIndexes')}</p>
      )}

      <div className="space-y-3">
        {indexes.map((idx, index) => (
          <div key={idx._id} className="bg-muted/30 rounded-md p-3 space-y-2">
            <div className="grid grid-cols-2 gap-3">
              <div className="space-y-1">
                <label htmlFor={`idx-name-${idx._id}`} className="text-xs text-muted-foreground">
                  {t('createTable.indexName')}
                </label>
                <Input
                  id={`idx-name-${idx._id}`}
                  value={idx.name}
                  onChange={e => onUpdate(index, { name: e.target.value })}
                  placeholder="idx_table_col"
                  className="h-8 text-sm"
                />
              </div>
              <div className="space-y-1">
                <label htmlFor={`idx-cols-${idx._id}`} className="text-xs text-muted-foreground">
                  {t('createTable.indexColumns')}
                </label>
                <Input
                  id={`idx-cols-${idx._id}`}
                  value={idx.columns.join(', ')}
                  onChange={e => onUpdate(index, { columns: parseColumnList(e.target.value) })}
                  placeholder={availableSourceColumns.slice(0, 2).join(', ') || 'col_a, col_b'}
                  className="h-8 text-sm"
                  list={`idx-cols-list-${idx._id}`}
                />
                <datalist id={`idx-cols-list-${idx._id}`}>
                  {availableSourceColumns.map(c => (
                    <option key={c} value={c} />
                  ))}
                </datalist>
              </div>
            </div>

            <div className="grid grid-cols-[auto_1fr_1fr_auto] gap-3 items-end">
              <div className="flex items-center gap-2 pb-2">
                <Checkbox
                  id={`idx-uniq-${idx._id}`}
                  checked={!!idx.unique}
                  disabled={!capabilities.supportsUniqueIndex}
                  onCheckedChange={checked => onUpdate(index, { unique: !!checked })}
                />
                <label htmlFor={`idx-uniq-${idx._id}`} className="text-sm">
                  {t('createTable.indexUnique')}
                </label>
              </div>
              {capabilities.supportsIndexMethod && (
                <div className="space-y-1">
                  <label
                    htmlFor={`idx-method-${idx._id}`}
                    className="text-xs text-muted-foreground"
                  >
                    {t('createTable.indexMethod')}
                  </label>
                  <Input
                    id={`idx-method-${idx._id}`}
                    value={idx.method ?? ''}
                    onChange={e => onUpdate(index, { method: e.target.value || undefined })}
                    placeholder="btree, gin, hash..."
                    className="h-8 text-sm"
                  />
                </div>
              )}
              {capabilities.supportsPartialIndex && (
                <div className="space-y-1">
                  <label htmlFor={`idx-where-${idx._id}`} className="text-xs text-muted-foreground">
                    {t('createTable.indexWhere')}
                  </label>
                  <Input
                    id={`idx-where-${idx._id}`}
                    value={idx.where ?? ''}
                    onChange={e => onUpdate(index, { where: e.target.value || undefined })}
                    placeholder={t('createTable.indexWherePlaceholder')}
                    className="h-8 text-sm font-mono"
                  />
                </div>
              )}
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
        ))}
      </div>
    </div>
  );
}
