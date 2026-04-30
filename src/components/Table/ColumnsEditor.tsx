// SPDX-License-Identifier: Apache-2.0

import { ChevronDown, ChevronRight, MoveDown, MoveUp, Plus, Trash2 } from 'lucide-react';
import { useState } from 'react';
import { useTranslation } from 'react-i18next';
import { Button } from '@/components/ui/button';
import { Checkbox } from '@/components/ui/checkbox';
import { Input } from '@/components/ui/input';
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select';
import { Textarea } from '@/components/ui/textarea';
import type { ColumnDef, ColumnType, DdlCapabilities } from '@/lib/ddl';
import { Driver } from '@/lib/drivers';
import type { EditableColumn } from './createTableTypes';

interface ColumnsEditorProps {
  columns: EditableColumn[];
  columnTypes: ColumnType[];
  driver: Driver;
  capabilities: DdlCapabilities;
  onAdd: () => void;
  onUpdate: (index: number, updates: Partial<ColumnDef>) => void;
  onRemove: (index: number) => void;
  onMove: (index: number, direction: 'up' | 'down') => void;
}

export function ColumnsEditor({
  columns,
  columnTypes,
  driver,
  capabilities,
  onAdd,
  onUpdate,
  onRemove,
  onMove,
}: ColumnsEditorProps) {
  const { t } = useTranslation();
  const [expandedIds, setExpandedIds] = useState<Set<string>>(new Set());

  const toggleExpand = (id: string) => {
    setExpandedIds(prev => {
      const next = new Set(prev);
      if (next.has(id)) next.delete(id);
      else next.add(id);
      return next;
    });
  };

  const supportsAutoIncrement = driver === Driver.Mysql || driver === Driver.Mariadb;
  const showCommentField = capabilities.inlineColumnComments || capabilities.separateColumnComments;

  return (
    <div className="space-y-2">
      <div className="flex items-center justify-between">
        <p className="text-sm font-medium">{t('createTable.columns')}</p>
        <Button variant="outline" size="sm" onClick={onAdd}>
          <Plus size={14} className="mr-1" />
          {t('createTable.addColumn')}
        </Button>
      </div>

      <div className="grid grid-cols-[24px_1fr_120px_160px_60px_60px_60px_80px] gap-2 text-xs text-muted-foreground px-2">
        <span></span>
        <span>{t('createTable.columnName')}</span>
        <span>{t('createTable.type')}</span>
        <span>{t('createTable.length')}</span>
        <span className="text-center">NULL</span>
        <span className="text-center">PK</span>
        <span className="text-center">UQ</span>
        <span></span>
      </div>

      <div className="space-y-1 max-h-87.5 overflow-auto">
        {columns.map((col, index) => {
          const typeConfig = columnTypes.find(c => c.name === col.type);
          const isExpanded = expandedIds.has(col._id);
          return (
            <div key={col._id} className="bg-muted/30 rounded-md">
              <div className="grid grid-cols-[24px_1fr_120px_160px_60px_60px_60px_80px] gap-2 items-center px-2 py-1">
                <Button
                  variant="ghost"
                  size="icon"
                  className="h-7 w-6"
                  onClick={() => toggleExpand(col._id)}
                  aria-label={t('createTable.toggleAdvanced')}
                >
                  {isExpanded ? <ChevronDown size={14} /> : <ChevronRight size={14} />}
                </Button>
                <Input
                  value={col.name}
                  onChange={e => onUpdate(index, { name: e.target.value })}
                  placeholder="column_name"
                  className="h-8 text-sm"
                />
                <Select value={col.type} onValueChange={value => onUpdate(index, { type: value })}>
                  <SelectTrigger className="h-8 text-sm">
                    <SelectValue />
                  </SelectTrigger>
                  <SelectContent>
                    {columnTypes.map(ct => (
                      <SelectItem key={ct.name} value={ct.name}>
                        {ct.name}
                      </SelectItem>
                    ))}
                  </SelectContent>
                </Select>
                {typeConfig?.hasPrecision ? (
                  <div className="flex items-center gap-1">
                    <Input
                      type="number"
                      value={col.precision || ''}
                      onChange={e =>
                        onUpdate(index, {
                          precision: parseInt(e.target.value, 10) || undefined,
                        })
                      }
                      placeholder="10"
                      className="h-8 text-sm"
                      aria-label={t('createTable.precision')}
                    />
                    <Input
                      type="number"
                      value={col.scale || ''}
                      onChange={e =>
                        onUpdate(index, {
                          scale: parseInt(e.target.value, 10) || undefined,
                        })
                      }
                      placeholder="0"
                      className="h-8 text-sm"
                      aria-label={t('createTable.scale')}
                    />
                  </div>
                ) : (
                  <Input
                    type="number"
                    value={col.length || ''}
                    onChange={e =>
                      onUpdate(index, {
                        length: parseInt(e.target.value, 10) || undefined,
                      })
                    }
                    disabled={!typeConfig?.hasLength}
                    placeholder={typeConfig?.hasLength ? '255' : '-'}
                    className="h-8 text-sm"
                    aria-label={t('createTable.length')}
                  />
                )}
                <div className="flex justify-center">
                  <Checkbox
                    checked={col.nullable}
                    onCheckedChange={checked => onUpdate(index, { nullable: !!checked })}
                  />
                </div>
                <div className="flex justify-center">
                  <Checkbox
                    checked={col.isPrimaryKey}
                    onCheckedChange={checked => onUpdate(index, { isPrimaryKey: !!checked })}
                  />
                </div>
                <div className="flex justify-center">
                  <Checkbox
                    checked={col.isUnique}
                    onCheckedChange={checked => onUpdate(index, { isUnique: !!checked })}
                  />
                </div>
                <div className="flex gap-1">
                  <Button
                    variant="ghost"
                    size="icon"
                    className="h-7 w-7"
                    onClick={() => onMove(index, 'up')}
                    disabled={index === 0}
                  >
                    <MoveUp size={14} />
                  </Button>
                  <Button
                    variant="ghost"
                    size="icon"
                    className="h-7 w-7"
                    onClick={() => onMove(index, 'down')}
                    disabled={index === columns.length - 1}
                  >
                    <MoveDown size={14} />
                  </Button>
                  <Button
                    variant="ghost"
                    size="icon"
                    className="h-7 w-7 text-destructive hover:text-destructive"
                    onClick={() => onRemove(index)}
                    disabled={columns.length <= 1}
                  >
                    <Trash2 size={14} />
                  </Button>
                </div>
              </div>

              {isExpanded && (
                <div className="grid grid-cols-2 gap-3 px-2 pb-2 pt-1 border-t border-border/40">
                  {col._originalName && col._originalName !== col.name && (
                    <p className="col-span-2 text-xs text-amber-600 dark:text-amber-400">
                      {t('alterTable.renamedFrom', { name: col._originalName })}
                    </p>
                  )}
                  <div className="space-y-1">
                    <label
                      htmlFor={`col-default-${col._id}`}
                      className="text-xs text-muted-foreground"
                    >
                      {t('createTable.defaultValue')}
                    </label>
                    <Input
                      id={`col-default-${col._id}`}
                      value={col.defaultValue ?? ''}
                      onChange={e => onUpdate(index, { defaultValue: e.target.value || undefined })}
                      placeholder={t('createTable.defaultValuePlaceholder')}
                      className="h-8 text-sm font-mono"
                    />
                  </div>
                  {showCommentField && (
                    <div className="space-y-1">
                      <label
                        htmlFor={`col-comment-${col._id}`}
                        className="text-xs text-muted-foreground"
                      >
                        {t('createTable.columnComment')}
                      </label>
                      <Textarea
                        id={`col-comment-${col._id}`}
                        value={col.comment ?? ''}
                        onChange={e => onUpdate(index, { comment: e.target.value || undefined })}
                        rows={2}
                        className="text-sm"
                      />
                    </div>
                  )}
                  {supportsAutoIncrement && (
                    <div className="flex items-center gap-2 pt-5">
                      <Checkbox
                        id={`col-ai-${col._id}`}
                        checked={!!col.isAutoIncrement}
                        onCheckedChange={checked => onUpdate(index, { isAutoIncrement: !!checked })}
                      />
                      <label htmlFor={`col-ai-${col._id}`} className="text-sm">
                        {t('createTable.autoIncrement')}
                      </label>
                    </div>
                  )}
                </div>
              )}
            </div>
          );
        })}
      </div>
    </div>
  );
}
