// SPDX-License-Identifier: Apache-2.0

import { useState, useMemo } from 'react';
import { useTranslation } from 'react-i18next';
import { Plus, Trash2, MoveUp, MoveDown, Code } from 'lucide-react';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
  DialogFooter,
} from '@/components/ui/dialog';
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select';
import { Checkbox } from '@/components/ui/checkbox';
import { Driver } from '@/lib/drivers';
import { ColumnDef, getColumnTypes, buildCreateTableSQL, ColumnType } from '@/lib/column-types';
import { Namespace, executeQuery } from '@/lib/tauri';
import { notify } from '@/lib/notify';

interface CreateTableModalProps {
  isOpen: boolean;
  onClose: () => void;
  sessionId: string;
  namespace: Namespace;
  driver: Driver;
  onTableCreated?: (tableName: string) => void;
}

function createEmptyColumn(): ColumnDef {
  return {
    name: '',
    type: 'VARCHAR',
    length: 255,
    precision: undefined,
    scale: undefined,
    nullable: true,
    isPrimaryKey: false,
    isUnique: false,
  };
}

export function CreateTableModal({
  isOpen,
  onClose,
  sessionId,
  namespace,
  driver,
  onTableCreated,
}: CreateTableModalProps) {
  const { t } = useTranslation();
  const [tableName, setTableName] = useState('');
  const [columns, setColumns] = useState<ColumnDef[]>([createEmptyColumn()]);
  const [showSQL, setShowSQL] = useState(false);
  const [loading, setLoading] = useState(false);

  const columnTypes = useMemo(() => getColumnTypes(driver), [driver]);
  const generatedSQL = useMemo(() => {
    if (!tableName.trim() || columns.length === 0) return '';
    const validColumns = columns.filter(c => c.name.trim() && c.type);
    if (validColumns.length === 0) return '';
    return buildCreateTableSQL(namespace, tableName, validColumns, driver);
  }, [namespace, tableName, columns, driver]);

  function addColumn() {
    // Set default type based on driver
    const defaultType = driver === Driver.Mysql ? 'VARCHAR' : 'VARCHAR';
    setColumns(prev => [...prev, { ...createEmptyColumn(), type: defaultType }]);
  }

  function removeColumn(index: number) {
    setColumns(prev => prev.filter((_, i) => i !== index));
  }

  function updateColumn(index: number, updates: Partial<ColumnDef>) {
    setColumns(prev =>
      prev.map((col, i) => {
        if (i !== index) return col;
        const next = { ...col, ...updates };
        if (updates.type) {
          const typeConfig = getTypeConfig(updates.type);
          if (typeConfig?.hasLength) {
            next.length = next.length ?? 255;
            next.precision = undefined;
            next.scale = undefined;
          } else if (typeConfig?.hasPrecision) {
            next.length = undefined;
          } else {
            next.length = undefined;
            next.precision = undefined;
            next.scale = undefined;
          }
        }
        return next;
      })
    );
  }

  function moveColumn(index: number, direction: 'up' | 'down') {
    const newIndex = direction === 'up' ? index - 1 : index + 1;
    if (newIndex < 0 || newIndex >= columns.length) return;

    setColumns(prev => {
      const newCols = [...prev];
      [newCols[index], newCols[newIndex]] = [newCols[newIndex], newCols[index]];
      return newCols;
    });
  }

  function getTypeConfig(typeName: string): ColumnType | undefined {
    return columnTypes.find(t => t.name === typeName);
  }

  async function handleCreate() {
    if (!tableName.trim() || !generatedSQL) {
      notify.error(t('createTable.tableNameRequired'));
      return;
    }

    setLoading(true);
    try {
      const result = await executeQuery(sessionId, generatedSQL, { namespace });
      if (result.success) {
        notify.success(t('createTable.success', { name: tableName }));
        onTableCreated?.(tableName.trim());
        handleClose();
      } else {
        notify.error(t('createTable.failed'), result.error);
      }
    } catch (err) {
      notify.error(t('createTable.failed'), err);
    } finally {
      setLoading(false);
    }
  }

  function handleClose() {
    setTableName('');
    setColumns([createEmptyColumn()]);
    setShowSQL(false);
    onClose();
  }

  return (
    <Dialog open={isOpen} onOpenChange={open => !open && handleClose()}>
      <DialogContent className="max-w-4xl max-h-[90vh] overflow-hidden flex flex-col">
        <DialogHeader>
          <DialogTitle className="flex items-center justify-between">
            <span>{t('createTable.title')}</span>
            <Button
              variant="ghost"
              size="sm"
              onClick={() => setShowSQL(!showSQL)}
              className="text-muted-foreground"
            >
              <Code size={16} className="mr-1" />
              {showSQL ? t('createTable.hideSQL') : t('createTable.showSQL')}
            </Button>
          </DialogTitle>
        </DialogHeader>

        <div className="flex-1 overflow-auto space-y-4 py-4">
          {/* Table name */}
          <div className="flex items-center gap-4">
            <label className="text-sm font-medium w-24">{t('createTable.tableName')}</label>
            <Input
              value={tableName}
              onChange={e => setTableName(e.target.value)}
              placeholder={t('createTable.tableNamePlaceholder')}
              className="flex-1"
            />
          </div>

          {/* Columns */}
          <div className="space-y-2">
            <div className="flex items-center justify-between">
              <label className="text-sm font-medium">{t('createTable.columns')}</label>
              <Button variant="outline" size="sm" onClick={addColumn}>
                <Plus size={14} className="mr-1" />
                {t('createTable.addColumn')}
              </Button>
            </div>

            {/* Column header */}
            <div className="grid grid-cols-[1fr_120px_160px_60px_60px_60px_80px] gap-2 text-xs text-muted-foreground px-2">
              <span>{t('createTable.columnName')}</span>
              <span>{t('createTable.type')}</span>
              <span>{t('createTable.length')}</span>
              <span className="text-center">NULL</span>
              <span className="text-center">PK</span>
              <span className="text-center">UQ</span>
              <span></span>
            </div>

            {/* Column rows */}
            <div className="space-y-1 max-h-75 overflow-auto">
              {columns.map((col, index) => {
                const typeConfig = getTypeConfig(col.type);
                return (
                  <div
                    key={index}
                    className="grid grid-cols-[1fr_120px_160px_60px_60px_60px_80px] gap-2 items-center bg-muted/30 rounded-md px-2 py-1"
                  >
                    <Input
                      value={col.name}
                      onChange={e => updateColumn(index, { name: e.target.value })}
                      placeholder="column_name"
                      className="h-8 text-sm"
                    />
                    <Select
                      value={col.type}
                      onValueChange={value => updateColumn(index, { type: value })}
                    >
                      <SelectTrigger className="h-8 text-sm">
                        <SelectValue />
                      </SelectTrigger>
                      <SelectContent>
                        {columnTypes.map(t => (
                          <SelectItem key={t.name} value={t.name}>
                            {t.name}
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
                            updateColumn(index, {
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
                            updateColumn(index, {
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
                          updateColumn(index, {
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
                        onCheckedChange={checked => updateColumn(index, { nullable: !!checked })}
                      />
                    </div>
                    <div className="flex justify-center">
                      <Checkbox
                        checked={col.isPrimaryKey}
                        onCheckedChange={checked =>
                          updateColumn(index, { isPrimaryKey: !!checked })
                        }
                      />
                    </div>
                    <div className="flex justify-center">
                      <Checkbox
                        checked={col.isUnique}
                        onCheckedChange={checked => updateColumn(index, { isUnique: !!checked })}
                      />
                    </div>
                    <div className="flex gap-1">
                      <Button
                        variant="ghost"
                        size="icon"
                        className="h-7 w-7"
                        onClick={() => moveColumn(index, 'up')}
                        disabled={index === 0}
                      >
                        <MoveUp size={14} />
                      </Button>
                      <Button
                        variant="ghost"
                        size="icon"
                        className="h-7 w-7"
                        onClick={() => moveColumn(index, 'down')}
                        disabled={index === columns.length - 1}
                      >
                        <MoveDown size={14} />
                      </Button>
                      <Button
                        variant="ghost"
                        size="icon"
                        className="h-7 w-7 text-destructive hover:text-destructive"
                        onClick={() => removeColumn(index)}
                        disabled={columns.length <= 1}
                      >
                        <Trash2 size={14} />
                      </Button>
                    </div>
                  </div>
                );
              })}
            </div>
          </div>

          {showSQL && generatedSQL && (
            <div className="space-y-2">
              <label className="text-sm font-medium">{t('createTable.generatedSQL')}</label>
              <pre className="bg-muted/50 p-4 rounded-md text-sm font-mono overflow-auto max-h-37.5">
                {generatedSQL}
              </pre>
            </div>
          )}
        </div>

        <DialogFooter>
          <Button variant="outline" onClick={handleClose} disabled={loading}>
            {t('common.cancel')}
          </Button>
          <Button onClick={handleCreate} disabled={loading || !tableName.trim() || !generatedSQL}>
            {loading ? t('common.creating') : t('createTable.create')}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
