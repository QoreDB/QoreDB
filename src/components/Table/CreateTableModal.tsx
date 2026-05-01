// SPDX-License-Identifier: Apache-2.0

import { useEffect, useMemo, useRef, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { Button } from '@/components/ui/button';
import {
  Dialog,
  DialogContent,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog';
import { Input } from '@/components/ui/input';
import { Textarea } from '@/components/ui/textarea';
import type { Driver } from '@/lib/connection/drivers';
import {
  buildCreateTableStatements,
  buildQualifiedTableName,
  type CheckConstraintDef,
  type ColumnDef,
  type ForeignKeyDef,
  getColumnTypes,
  getDdlCapabilities,
  type IndexDef,
  type TableDefinition,
} from '@/lib/ddl';
import { notify } from '@/lib/notify';
import { executeQuery, type Namespace } from '@/lib/tauri';
import { cn } from '@/lib/utils';
import { ChecksEditor } from './ChecksEditor';
import { ColumnsEditor } from './ColumnsEditor';
import { CreateTablePreview } from './CreateTablePreview';
import type {
  CreateTableSection,
  EditableCheck,
  EditableColumn,
  EditableForeignKey,
  EditableIndex,
} from './createTableTypes';
import { ForeignKeysEditor } from './ForeignKeysEditor';
import { IndexesEditor } from './IndexesEditor';
import { WarningsBanner } from './WarningsBanner';

interface CreateTableModalProps {
  isOpen: boolean;
  onClose: () => void;
  sessionId: string;
  namespace: Namespace;
  driver: Driver;
  onTableCreated?: (tableName: string) => void;
}

function makeColumnId(seed: number): string {
  return `column-${seed}`;
}

function createEmptyColumn(id: string): EditableColumn {
  return {
    _id: id,
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

function createEmptyForeignKey(id: string): EditableForeignKey {
  return {
    _id: id,
    columns: [],
    refTable: '',
    refColumns: [],
  };
}

function createEmptyIndex(id: string): EditableIndex {
  return { _id: id, name: '', columns: [] };
}

function createEmptyCheck(id: string): EditableCheck {
  return { _id: id, expression: '' };
}

function stripId<T extends { _id: string }>(item: T): Omit<T, '_id'> {
  const { _id: _omit, ...rest } = item;
  return rest;
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
  const idCounterRef = useRef(1);
  const nextId = (prefix: string): string => {
    const id = `${prefix}-${idCounterRef.current}`;
    idCounterRef.current += 1;
    return id;
  };

  const [tableName, setTableName] = useState('');
  const [tableComment, setTableComment] = useState('');
  const [columns, setColumns] = useState<EditableColumn[]>([createEmptyColumn(makeColumnId(0))]);
  const [foreignKeys, setForeignKeys] = useState<EditableForeignKey[]>([]);
  const [indexes, setIndexes] = useState<EditableIndex[]>([]);
  const [checks, setChecks] = useState<EditableCheck[]>([]);
  const [activeSection, setActiveSection] = useState<CreateTableSection>('columns');
  const [loading, setLoading] = useState(false);

  const capabilities = useMemo(() => getDdlCapabilities(driver), [driver]);
  const columnTypes = useMemo(() => getColumnTypes(driver), [driver]);

  const tableDefinition = useMemo<TableDefinition>(() => {
    const validColumns = columns
      .filter(c => c.name.trim() && c.type)
      .map(c => stripId(c) as ColumnDef);
    return {
      namespace,
      tableName: tableName.trim(),
      columns: validColumns,
      comment: tableComment.trim() || undefined,
      foreignKeys: foreignKeys
        .filter(fk => fk.columns.length > 0 && fk.refTable.trim() && fk.refColumns.length > 0)
        .map(fk => stripId(fk) as ForeignKeyDef),
      indexes: indexes
        .filter(idx => idx.name.trim() && idx.columns.length > 0)
        .map(idx => stripId(idx) as IndexDef),
      checks: checks.filter(c => c.expression.trim()).map(c => stripId(c) as CheckConstraintDef),
    };
  }, [namespace, tableName, tableComment, columns, foreignKeys, indexes, checks]);

  const buildResult = useMemo(() => {
    if (!tableDefinition.tableName || tableDefinition.columns.length === 0) {
      return { statements: [], warnings: [] };
    }
    return buildCreateTableStatements(tableDefinition, driver);
  }, [tableDefinition, driver]);

  const generatedSQL = useMemo(() => buildResult.statements.join('\n\n'), [buildResult]);

  const sourceColumnNames = useMemo(
    () => columns.map(c => c.name).filter(n => n.trim().length > 0),
    [columns]
  );

  function reset() {
    setTableName('');
    setTableComment('');
    setColumns([createEmptyColumn(nextId('column'))]);
    setForeignKeys([]);
    setIndexes([]);
    setChecks([]);
    setActiveSection('columns');
  }

  // biome-ignore lint/correctness/useExhaustiveDependencies: reset only when modal opens
  useEffect(() => {
    if (!isOpen) return;
    reset();
  }, [isOpen]);

  function addColumn() {
    setColumns(prev => [...prev, createEmptyColumn(nextId('column'))]);
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
          const typeConfig = columnTypes.find(ct => ct.name === updates.type);
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

  function addForeignKey() {
    setForeignKeys(prev => [...prev, createEmptyForeignKey(nextId('fk'))]);
  }
  function removeForeignKey(index: number) {
    setForeignKeys(prev => prev.filter((_, i) => i !== index));
  }
  function updateForeignKey(index: number, updates: Partial<ForeignKeyDef>) {
    setForeignKeys(prev => prev.map((fk, i) => (i === index ? { ...fk, ...updates } : fk)));
  }

  function addIndex() {
    setIndexes(prev => [...prev, createEmptyIndex(nextId('idx'))]);
  }
  function removeIndex(index: number) {
    setIndexes(prev => prev.filter((_, i) => i !== index));
  }
  function updateIndex(index: number, updates: Partial<IndexDef>) {
    setIndexes(prev => prev.map((idx, i) => (i === index ? { ...idx, ...updates } : idx)));
  }

  function addCheck() {
    setChecks(prev => [...prev, createEmptyCheck(nextId('chk'))]);
  }
  function removeCheck(index: number) {
    setChecks(prev => prev.filter((_, i) => i !== index));
  }
  function updateCheck(index: number, updates: Partial<CheckConstraintDef>) {
    setChecks(prev => prev.map((c, i) => (i === index ? { ...c, ...updates } : c)));
  }

  async function handleCreate() {
    if (!tableName.trim()) {
      notify.error(t('createTable.tableNameRequired'));
      return;
    }
    if (buildResult.statements.length === 0) {
      notify.error(t('createTable.noColumns'));
      return;
    }

    setLoading(true);
    try {
      const fullName = buildQualifiedTableName(namespace, tableName.trim(), driver);
      const [createStmt, ...rest] = buildResult.statements;
      const createResult = await executeQuery(sessionId, createStmt, { namespace });
      if (!createResult.success) {
        notify.error(t('createTable.failed'), createResult.error);
        return;
      }

      for (const stmt of rest) {
        const r = await executeQuery(sessionId, stmt, { namespace });
        if (!r.success) {
          await executeQuery(sessionId, `DROP TABLE ${fullName};`, { namespace }).catch(
            () => undefined
          );
          notify.error(t('createTable.failed'), r.error);
          return;
        }
      }

      notify.success(t('createTable.success', { name: tableName }));
      onTableCreated?.(tableName.trim());
      handleClose();
    } catch (err) {
      notify.error(t('createTable.failed'), err);
    } finally {
      setLoading(false);
    }
  }

  function handleClose() {
    reset();
    onClose();
  }

  const sections: { id: CreateTableSection; label: string; show: boolean }[] = [
    { id: 'columns', label: t('createTable.tabColumns'), show: true },
    {
      id: 'foreignKeys',
      label: t('createTable.tabForeignKeys'),
      show: capabilities.supportsForeignKeys,
    },
    {
      id: 'indexes',
      label: t('createTable.tabIndexes'),
      show: capabilities.supportsIndexes,
    },
    {
      id: 'checks',
      label: t('createTable.tabChecks'),
      show: capabilities.supportsCheckConstraints,
    },
    { id: 'sql', label: t('createTable.tabSql'), show: true },
  ];
  const visibleSections = sections.filter(s => s.show);

  const supportsTableComment = capabilities.inlineTableComment || capabilities.separateTableComment;
  const canSubmit = !loading && tableName.trim() && buildResult.statements.length > 0;

  return (
    <Dialog open={isOpen} onOpenChange={open => !open && handleClose()}>
      <DialogContent className="max-w-4xl max-h-[90vh] overflow-hidden flex flex-col">
        <DialogHeader>
          <DialogTitle>{t('createTable.title')}</DialogTitle>
        </DialogHeader>

        <div className="flex-1 overflow-auto space-y-4 py-3">
          <div className="grid grid-cols-2 gap-4">
            <div className="space-y-1">
              <label htmlFor="create-table-name" className="text-sm font-medium">
                {t('createTable.tableName')}
              </label>
              <Input
                id="create-table-name"
                value={tableName}
                onChange={e => setTableName(e.target.value)}
                placeholder={t('createTable.tableNamePlaceholder')}
              />
            </div>
            {supportsTableComment && (
              <div className="space-y-1">
                <label htmlFor="create-table-comment" className="text-sm font-medium">
                  {t('createTable.tableComment')}
                </label>
                <Textarea
                  id="create-table-comment"
                  value={tableComment}
                  onChange={e => setTableComment(e.target.value)}
                  rows={2}
                />
              </div>
            )}
          </div>

          <div className="flex border-b border-border">
            {visibleSections.map(section => (
              <button
                key={section.id}
                type="button"
                onClick={() => setActiveSection(section.id)}
                className={cn(
                  'px-3 py-1.5 text-sm border-b-2 -mb-px transition-colors',
                  activeSection === section.id
                    ? 'border-primary text-foreground'
                    : 'border-transparent text-muted-foreground hover:text-foreground'
                )}
              >
                {section.label}
              </button>
            ))}
          </div>

          <div className="min-h-75">
            {activeSection === 'columns' && (
              <ColumnsEditor
                columns={columns}
                columnTypes={columnTypes}
                driver={driver}
                capabilities={capabilities}
                onAdd={addColumn}
                onUpdate={updateColumn}
                onRemove={removeColumn}
                onMove={moveColumn}
              />
            )}
            {activeSection === 'foreignKeys' && (
              <ForeignKeysEditor
                foreignKeys={foreignKeys}
                availableSourceColumns={sourceColumnNames}
                onAdd={addForeignKey}
                onUpdate={updateForeignKey}
                onRemove={removeForeignKey}
                disabled={!capabilities.supportsForeignKeys}
              />
            )}
            {activeSection === 'indexes' && (
              <IndexesEditor
                indexes={indexes}
                availableSourceColumns={sourceColumnNames}
                capabilities={capabilities}
                onAdd={addIndex}
                onUpdate={updateIndex}
                onRemove={removeIndex}
              />
            )}
            {activeSection === 'checks' && (
              <ChecksEditor
                checks={checks}
                onAdd={addCheck}
                onUpdate={updateCheck}
                onRemove={removeCheck}
                disabled={!capabilities.supportsCheckConstraints}
              />
            )}
            {activeSection === 'sql' && (
              <CreateTablePreview
                sql={generatedSQL}
                warnings={buildResult.warnings}
                driver={driver}
              />
            )}
          </div>
        </div>

        {buildResult.warnings.length > 0 && activeSection !== 'sql' && (
          <div className="px-1">
            <WarningsBanner warnings={buildResult.warnings} />
          </div>
        )}

        <DialogFooter>
          <Button variant="outline" onClick={handleClose} disabled={loading}>
            {t('common.cancel')}
          </Button>
          <Button onClick={handleCreate} disabled={!canSubmit}>
            {loading ? t('common.creating') : t('createTable.create')}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
