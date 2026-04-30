// SPDX-License-Identifier: Apache-2.0

import { Loader2 } from 'lucide-react';
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
import {
  buildAlterTableSQL,
  type CheckConstraintDef,
  type ColumnDef,
  diffTableDefinitions,
  type ForeignKeyDef,
  getColumnTypes,
  getDdlCapabilities,
  type IndexDef,
  type TableDefinition,
} from '@/lib/ddl';
import type { Driver } from '@/lib/drivers';
import { notify } from '@/lib/notify';
import { describeTable, executeQuery, type Namespace, type TableSchema } from '@/lib/tauri';
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
import {
  buildAlterSnapshot,
  tableSchemaToColumns,
  tableSchemaToForeignKeys,
  tableSchemaToIndexes,
} from './loadAlterTable';
import { WarningsBanner } from './WarningsBanner';

interface AlterTableModalProps {
  isOpen: boolean;
  onClose: () => void;
  sessionId: string;
  namespace: Namespace;
  driver: Driver;
  tableName: string;
  initialSchema?: TableSchema | null;
  onTableAltered?: (newName: string) => void;
}

export function AlterTableModal({
  isOpen,
  onClose,
  sessionId,
  namespace,
  driver,
  tableName,
  initialSchema,
  onTableAltered,
}: AlterTableModalProps) {
  const { t } = useTranslation();
  const idCounterRef = useRef(1);
  const idGen = {
    next: (prefix: string) => {
      const id = `${prefix}-${idCounterRef.current}`;
      idCounterRef.current += 1;
      return id;
    },
  };

  const capabilities = useMemo(() => getDdlCapabilities(driver), [driver]);
  const columnTypes = useMemo(() => getColumnTypes(driver), [driver]);

  const [loadingSchema, setLoadingSchema] = useState(false);
  const [originalTableName] = useState(tableName);
  const [editTableName, setEditTableName] = useState(tableName);
  const [tableComment, setTableComment] = useState('');
  const [columns, setColumns] = useState<EditableColumn[]>([]);
  const [foreignKeys, setForeignKeys] = useState<EditableForeignKey[]>([]);
  const [indexes, setIndexes] = useState<EditableIndex[]>([]);
  const [checks, setChecks] = useState<EditableCheck[]>([]);
  const [originalSnapshot, setOriginalSnapshot] = useState<TableDefinition | null>(null);
  const [activeSection, setActiveSection] = useState<CreateTableSection>('columns');
  const [applying, setApplying] = useState(false);

  function loadFromSchema(schema: TableSchema) {
    const cols = tableSchemaToColumns(schema, idGen);
    const fks = tableSchemaToForeignKeys(schema, idGen);
    const idx = tableSchemaToIndexes(schema, idGen);
    setColumns(cols);
    setForeignKeys(fks);
    setIndexes(idx);
    setChecks([]);
    setOriginalSnapshot(buildAlterSnapshot(namespace, tableName, cols, fks, idx));
  }

  useEffect(() => {
    if (!isOpen) return;
    setEditTableName(tableName);
    setTableComment('');
    setActiveSection('columns');
    if (initialSchema) {
      loadFromSchema(initialSchema);
      return;
    }
    setLoadingSchema(true);
    describeTable(sessionId, namespace, tableName)
      .then(res => {
        if (res.success && res.schema) {
          loadFromSchema(res.schema);
        } else {
          notify.error(t('alterTable.loadFailed'), res.error);
          onClose();
        }
      })
      .catch(err => {
        notify.error(t('alterTable.loadFailed'), err);
        onClose();
      })
      .finally(() => setLoadingSchema(false));
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [isOpen, sessionId, namespace, tableName, initialSchema]);

  const afterDefinition = useMemo<TableDefinition>(() => {
    return {
      namespace,
      tableName: editTableName.trim() || tableName,
      comment: tableComment.trim() || undefined,
      columns: columns
        .filter(c => c.name.trim())
        .map(c => {
          const { _id, _originalName, ...rest } = c;
          void _id;
          void _originalName;
          return rest as ColumnDef;
        }),
      foreignKeys: foreignKeys
        .filter(fk => fk.columns.length > 0 && fk.refTable.trim() && fk.refColumns.length > 0)
        .map(fk => {
          const { _id, ...rest } = fk;
          void _id;
          return rest as ForeignKeyDef;
        }),
      indexes: indexes
        .filter(idx => idx.name.trim() && idx.columns.length > 0)
        .map(idx => {
          const { _id, ...rest } = idx;
          void _id;
          return rest as IndexDef;
        }),
      checks: checks
        .filter(c => c.expression.trim())
        .map(c => {
          const { _id, ...rest } = c;
          void _id;
          return rest as CheckConstraintDef;
        }),
    };
  }, [namespace, editTableName, tableName, tableComment, columns, foreignKeys, indexes, checks]);

  const buildResult = useMemo(() => {
    if (!originalSnapshot) return { statements: [], warnings: [] };
    const renames = columns
      .filter(c => c._originalName && c._originalName !== c.name && c.name.trim().length > 0)
      .map(c => ({ from: c._originalName as string, to: c.name }));
    const ops = diffTableDefinitions(originalSnapshot, afterDefinition, {
      columnRenames: renames,
      tableRename:
        originalTableName !== afterDefinition.tableName
          ? { from: originalTableName, to: afterDefinition.tableName }
          : undefined,
    });
    return buildAlterTableSQL(originalSnapshot, ops, driver);
  }, [originalSnapshot, afterDefinition, columns, originalTableName, driver]);

  const generatedSQL = useMemo(() => buildResult.statements.join('\n\n'), [buildResult]);
  const sourceColumnNames = useMemo(
    () => columns.map(c => c.name).filter(n => n.trim().length > 0),
    [columns]
  );
  const hasChanges = buildResult.statements.length > 0;

  function addColumn() {
    setColumns(prev => [
      ...prev,
      {
        _id: idGen.next('column'),
        name: '',
        type: 'VARCHAR',
        length: 255,
        nullable: true,
        isPrimaryKey: false,
        isUnique: false,
      },
    ]);
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
    setForeignKeys(prev => [
      ...prev,
      { _id: idGen.next('fk'), columns: [], refTable: '', refColumns: [] },
    ]);
  }
  function removeForeignKey(index: number) {
    setForeignKeys(prev => prev.filter((_, i) => i !== index));
  }
  function updateForeignKey(index: number, updates: Partial<ForeignKeyDef>) {
    setForeignKeys(prev => prev.map((fk, i) => (i === index ? { ...fk, ...updates } : fk)));
  }

  function addIndex() {
    setIndexes(prev => [...prev, { _id: idGen.next('idx'), name: '', columns: [] }]);
  }
  function removeIndex(index: number) {
    setIndexes(prev => prev.filter((_, i) => i !== index));
  }
  function updateIndex(index: number, updates: Partial<IndexDef>) {
    setIndexes(prev => prev.map((idx, i) => (i === index ? { ...idx, ...updates } : idx)));
  }

  function addCheck() {
    setChecks(prev => [...prev, { _id: idGen.next('chk'), expression: '' }]);
  }
  function removeCheck(index: number) {
    setChecks(prev => prev.filter((_, i) => i !== index));
  }
  function updateCheck(index: number, updates: Partial<CheckConstraintDef>) {
    setChecks(prev => prev.map((c, i) => (i === index ? { ...c, ...updates } : c)));
  }

  async function handleApply() {
    if (!hasChanges) {
      notify.info(t('alterTable.noChanges'));
      return;
    }

    setApplying(true);
    try {
      for (const stmt of buildResult.statements) {
        const r = await executeQuery(sessionId, stmt, { namespace });
        if (!r.success) {
          notify.error(t('alterTable.failed'), r.error);
          return;
        }
      }
      notify.success(t('alterTable.success', { name: editTableName }));
      onTableAltered?.(editTableName.trim() || originalTableName);
      onClose();
    } catch (err) {
      notify.error(t('alterTable.failed'), err);
    } finally {
      setApplying(false);
    }
  }

  const sections: { id: CreateTableSection; label: string; show: boolean }[] = [
    { id: 'columns', label: t('createTable.tabColumns'), show: true },
    {
      id: 'foreignKeys',
      label: t('createTable.tabForeignKeys'),
      show: capabilities.supportsForeignKeys,
    },
    { id: 'indexes', label: t('createTable.tabIndexes'), show: capabilities.supportsIndexes },
    {
      id: 'checks',
      label: t('createTable.tabChecks'),
      show: capabilities.supportsCheckConstraints,
    },
    { id: 'sql', label: t('alterTable.tabDiff'), show: true },
  ];
  const visibleSections = sections.filter(s => s.show);

  const supportsTableComment = capabilities.inlineTableComment || capabilities.separateTableComment;

  return (
    <Dialog open={isOpen} onOpenChange={open => !open && onClose()}>
      <DialogContent className="max-w-4xl max-h-[90vh] overflow-hidden flex flex-col">
        <DialogHeader>
          <DialogTitle>{t('alterTable.title', { name: originalTableName })}</DialogTitle>
        </DialogHeader>

        {loadingSchema ? (
          <div className="flex items-center justify-center py-10 text-muted-foreground">
            <Loader2 className="animate-spin mr-2" size={16} />
            {t('alterTable.loading')}
          </div>
        ) : (
          <>
            <div className="flex-1 overflow-auto space-y-4 py-3">
              <div className="grid grid-cols-2 gap-4">
                <div className="space-y-1">
                  <label htmlFor="alter-table-name" className="text-sm font-medium">
                    {t('createTable.tableName')}
                  </label>
                  <Input
                    id="alter-table-name"
                    value={editTableName}
                    onChange={e => setEditTableName(e.target.value)}
                  />
                  {editTableName !== originalTableName && (
                    <p className="text-xs text-amber-600 dark:text-amber-400">
                      {t('alterTable.renamedTable', { from: originalTableName })}
                    </p>
                  )}
                </div>
                {supportsTableComment && (
                  <div className="space-y-1">
                    <label htmlFor="alter-table-comment" className="text-sm font-medium">
                      {t('createTable.tableComment')}
                    </label>
                    <Textarea
                      id="alter-table-comment"
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
                    sql={generatedSQL || `-- ${t('alterTable.noChanges')}`}
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
              <Button variant="outline" onClick={onClose} disabled={applying}>
                {t('common.cancel')}
              </Button>
              <Button onClick={handleApply} disabled={!hasChanges || applying}>
                {applying ? t('alterTable.applying') : t('alterTable.apply')}
              </Button>
            </DialogFooter>
          </>
        )}
      </DialogContent>
    </Dialog>
  );
}
