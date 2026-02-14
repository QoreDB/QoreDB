import { useCallback, useEffect, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { toast } from 'sonner';

import { Button } from '@/components/ui/button';
import {
  Dialog,
  DialogContent,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog';
import { Input } from '@/components/ui/input';
import { Label } from '@/components/ui/label';
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select';

import {
  Namespace,
  VirtualRelation,
  addVirtualRelation,
  updateVirtualRelation,
  listCollections,
  describeTable,
  TableColumn,
} from '@/lib/tauri';

interface VirtualRelationDialogProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  sessionId: string;
  connectionId: string;
  namespace: Namespace;
  sourceTable?: string;
  sourceColumn?: string;
  existingRelation?: VirtualRelation;
  onSaved: () => void;
}

export function VirtualRelationDialog({
  open,
  onOpenChange,
  sessionId,
  connectionId,
  namespace,
  sourceTable: initialSourceTable,
  sourceColumn: initialSourceColumn,
  existingRelation,
  onSaved,
}: VirtualRelationDialogProps) {
  const { t } = useTranslation();
  const isEdit = !!existingRelation;

  const [sourceTable, setSourceTable] = useState('');
  const [sourceColumn, setSourceColumn] = useState('');
  const [referencedTable, setReferencedTable] = useState('');
  const [referencedColumn, setReferencedColumn] = useState('');
  const [label, setLabel] = useState('');
  const [saving, setSaving] = useState(false);

  const [tables, setTables] = useState<string[]>([]);
  const [sourceColumns, setSourceColumns] = useState<TableColumn[]>([]);
  const [referencedColumns, setReferencedColumns] = useState<TableColumn[]>([]);

  // Load tables list
  useEffect(() => {
    if (!open) return;
    listCollections(sessionId, namespace).then(result => {
      if (result.success && result.data) {
        setTables(result.data.collections.map(c => c.name));
      }
    });
  }, [open, sessionId, namespace]);

  // Reset form when opening
  useEffect(() => {
    if (!open) return;
    if (existingRelation) {
      setSourceTable(existingRelation.source_table);
      setSourceColumn(existingRelation.source_column);
      setReferencedTable(existingRelation.referenced_table);
      setReferencedColumn(existingRelation.referenced_column);
      setLabel(existingRelation.label ?? '');
    } else {
      setSourceTable(initialSourceTable ?? '');
      setSourceColumn(initialSourceColumn ?? '');
      setReferencedTable('');
      setReferencedColumn('');
      setLabel('');
    }
  }, [open, existingRelation, initialSourceTable, initialSourceColumn]);

  // Load source table columns
  const loadColumns = useCallback(
    async (tableName: string): Promise<TableColumn[]> => {
      if (!tableName) return [];
      const result = await describeTable(sessionId, namespace, tableName);
      return result.success && result.schema ? result.schema.columns : [];
    },
    [sessionId, namespace]
  );

  useEffect(() => {
    if (!open || !sourceTable) {
      setSourceColumns([]);
      return;
    }
    loadColumns(sourceTable).then(setSourceColumns);
  }, [open, sourceTable, loadColumns]);

  useEffect(() => {
    if (!open || !referencedTable) {
      setReferencedColumns([]);
      return;
    }
    loadColumns(referencedTable).then(setReferencedColumns);
  }, [open, referencedTable, loadColumns]);

  function close() {
    onOpenChange(false);
  }

  async function handleSave() {
    if (!sourceTable || !sourceColumn || !referencedTable || !referencedColumn) return;

    setSaving(true);
    try {
      const relation: VirtualRelation = {
        id: existingRelation?.id ?? crypto.randomUUID(),
        source_database: namespace.database,
        source_schema: namespace.schema ?? undefined,
        source_table: sourceTable,
        source_column: sourceColumn,
        referenced_table: referencedTable,
        referenced_column: referencedColumn,
        referenced_schema: namespace.schema ?? undefined,
        referenced_database: undefined,
        label: label.trim() || undefined,
      };

      const result = isEdit
        ? await updateVirtualRelation(connectionId, relation)
        : await addVirtualRelation(connectionId, relation);

      if (result.success) {
        toast.success(t('virtualRelations.saveSuccess'));
        onSaved();
        close();
      } else {
        toast.error(t('virtualRelations.saveError'), {
          description: result.error,
        });
      }
    } catch (err) {
      toast.error(t('virtualRelations.saveError'), {
        description: err instanceof Error ? err.message : String(err),
      });
    } finally {
      setSaving(false);
    }
  }

  const isValid = sourceTable && sourceColumn && referencedTable && referencedColumn;

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="max-w-lg">
        <DialogHeader>
          <DialogTitle>
            {isEdit ? t('virtualRelations.editTitle') : t('virtualRelations.addTitle')}
          </DialogTitle>
        </DialogHeader>

        <p className="text-xs text-muted-foreground">{t('virtualRelations.description')}</p>

        <div className="grid gap-4 py-2">
          {/* Source Table */}
          <div className="grid gap-2">
            <Label>{t('virtualRelations.sourceTable')}</Label>
            <Select
              value={sourceTable}
              onValueChange={v => {
                setSourceTable(v);
                setSourceColumn('');
              }}
            >
              <SelectTrigger>
                <SelectValue placeholder={t('virtualRelations.selectTable')} />
              </SelectTrigger>
              <SelectContent>
                {tables.map(name => (
                  <SelectItem key={name} value={name}>
                    {name}
                  </SelectItem>
                ))}
              </SelectContent>
            </Select>
          </div>

          {/* Source Column */}
          <div className="grid gap-2">
            <Label>{t('virtualRelations.sourceColumn')}</Label>
            <Select
              value={sourceColumn}
              onValueChange={setSourceColumn}
              disabled={!sourceTable || sourceColumns.length === 0}
            >
              <SelectTrigger>
                <SelectValue placeholder={t('virtualRelations.selectColumn')} />
              </SelectTrigger>
              <SelectContent>
                {sourceColumns.map(col => (
                  <SelectItem key={col.name} value={col.name}>
                    {col.name}{' '}
                    <span className="text-muted-foreground ml-1 text-xs">({col.data_type})</span>
                  </SelectItem>
                ))}
              </SelectContent>
            </Select>
          </div>

          {/* Referenced Table */}
          <div className="grid gap-2">
            <Label>{t('virtualRelations.referencedTable')}</Label>
            <Select
              value={referencedTable}
              onValueChange={v => {
                setReferencedTable(v);
                setReferencedColumn('');
              }}
            >
              <SelectTrigger>
                <SelectValue placeholder={t('virtualRelations.selectTable')} />
              </SelectTrigger>
              <SelectContent>
                {tables.map(name => (
                  <SelectItem key={name} value={name}>
                    {name}
                  </SelectItem>
                ))}
              </SelectContent>
            </Select>
          </div>

          {/* Referenced Column */}
          <div className="grid gap-2">
            <Label>{t('virtualRelations.referencedColumn')}</Label>
            <Select
              value={referencedColumn}
              onValueChange={setReferencedColumn}
              disabled={!referencedTable || referencedColumns.length === 0}
            >
              <SelectTrigger>
                <SelectValue placeholder={t('virtualRelations.selectColumn')} />
              </SelectTrigger>
              <SelectContent>
                {referencedColumns.map(col => (
                  <SelectItem key={col.name} value={col.name}>
                    {col.name}{' '}
                    <span className="text-muted-foreground ml-1 text-xs">({col.data_type})</span>
                  </SelectItem>
                ))}
              </SelectContent>
            </Select>
          </div>

          {/* Label */}
          <div className="grid gap-2">
            <Label>{t('virtualRelations.label')}</Label>
            <Input
              value={label}
              onChange={e => setLabel(e.target.value)}
              placeholder={t('virtualRelations.labelPlaceholder')}
            />
          </div>
        </div>

        <DialogFooter>
          <Button variant="outline" onClick={close}>
            {t('common.cancel')}
          </Button>
          <Button onClick={handleSave} disabled={!isValid || saving}>
            {t('virtualRelations.save')}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
