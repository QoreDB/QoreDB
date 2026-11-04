// SPDX-License-Identifier: BUSL-1.1

import { BookOpen, Loader2 } from 'lucide-react';
import { useEffect, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { Button } from '@/components/ui/button';
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog';
import { Input } from '@/components/ui/input';
import { Label } from '@/components/ui/label';
import {
  aiGetSemanticContext,
  aiSaveSemanticContext,
  type SemanticContextFile,
  type TableSemanticAnnotation,
} from '@/lib/ai';
import { notify } from '@/lib/notify';
import { describeTable, type Namespace, type TableColumn } from '@/lib/tauri';

interface SemanticContextDialogProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  sessionId: string;
  namespace: Namespace;
  tableName: string;
}

export function SemanticContextDialog({
  open,
  onOpenChange,
  sessionId,
  namespace,
  tableName,
}: SemanticContextDialogProps) {
  const { t } = useTranslation();
  const [loading, setLoading] = useState(false);
  const [saving, setSaving] = useState(false);
  const [contextFile, setContextFile] = useState<SemanticContextFile | null>(null);
  const [tableColumns, setTableColumns] = useState<TableColumn[]>([]);

  const [description, setDescription] = useState('');
  const [domain, setDomain] = useState('');
  const [columnNotes, setColumnNotes] = useState<Record<string, string>>({});
  const [implicitRelationsText, setImplicitRelationsText] = useState('');
  const [businessRulesText, setBusinessRulesText] = useState('');

  useEffect(() => {
    if (!open) return;
    let cancelled = false;
    setLoading(true);

    Promise.all([
      aiGetSemanticContext().catch(
        (): SemanticContextFile => ({
          version: 1,
          business_rules: [],
          conventions: [],
          tables: {},
        })
      ),
      describeTable(sessionId, namespace, tableName).catch(() => null),
    ])
      .then(([ctx, desc]) => {
        if (cancelled) return;
        setContextFile(ctx);
        const cols = desc?.success && desc.schema ? desc.schema.columns : [];
        setTableColumns(cols);

        const existingKey =
          Object.keys(ctx.tables).find(k => k.toLowerCase() === tableName.toLowerCase()) ??
          tableName;
        const tableEntry: TableSemanticAnnotation | undefined = ctx.tables[existingKey];

        setDescription(tableEntry?.description ?? '');
        setDomain(tableEntry?.domain ?? '');
        setColumnNotes({ ...(tableEntry?.columns ?? {}) });
        setImplicitRelationsText((tableEntry?.implicit_relations ?? []).join('\n'));
        setBusinessRulesText((ctx.business_rules ?? []).join('\n'));
      })
      .finally(() => {
        if (!cancelled) setLoading(false);
      });

    return () => {
      cancelled = true;
    };
  }, [open, sessionId, namespace, tableName]);

  const handleSave = async () => {
    setSaving(true);
    try {
      const base: SemanticContextFile = contextFile ?? {
        version: 1,
        business_rules: [],
        conventions: [],
        tables: {},
      };

      const cleanedColumns: Record<string, string> = {};
      for (const [col, note] of Object.entries(columnNotes)) {
        const trimmed = note.trim();
        if (trimmed) {
          cleanedColumns[col] = trimmed;
        }
      }

      const implicitRelations = implicitRelationsText
        .split('\n')
        .map(line => line.trim())
        .filter(Boolean);

      const businessRules = businessRulesText
        .split('\n')
        .map(line => line.trim())
        .filter(Boolean);

      const nextTables = { ...base.tables };
      const existingKey =
        Object.keys(nextTables).find(k => k.toLowerCase() === tableName.toLowerCase()) ?? tableName;

      const hasTableContent =
        description.trim().length > 0 ||
        domain.trim().length > 0 ||
        Object.keys(cleanedColumns).length > 0 ||
        implicitRelations.length > 0;

      if (hasTableContent) {
        nextTables[existingKey] = {
          description: description.trim() || undefined,
          domain: domain.trim() || undefined,
          columns: cleanedColumns,
          implicit_relations: implicitRelations,
        };
      } else {
        delete nextTables[existingKey];
      }

      const nextContext: SemanticContextFile = {
        ...base,
        version: 1,
        business_rules: businessRules,
        tables: nextTables,
      };

      await aiSaveSemanticContext(nextContext);
      notify.success(t('ai.semanticContext.saved'));
      onOpenChange(false);
    } catch (err) {
      notify.error(t('common.error'), err);
    } finally {
      setSaving(false);
    }
  };

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="sm:max-w-[560px] max-h-[85vh] flex flex-col">
        <DialogHeader>
          <DialogTitle className="flex items-center gap-2 text-base">
            <BookOpen className="h-4 w-4 text-primary" />
            {t('ai.semanticContext.title', { table: tableName })}
          </DialogTitle>
          <DialogDescription className="text-xs">
            {t('ai.semanticContext.description')}
          </DialogDescription>
        </DialogHeader>

        {loading ? (
          <div className="flex items-center justify-center py-8 text-muted-foreground text-xs gap-2">
            <Loader2 className="h-4 w-4 animate-spin" />
            {t('common.loading')}
          </div>
        ) : (
          <div className="space-y-4 overflow-y-auto pr-1 py-2 text-xs">
            <div className="grid grid-cols-1 sm:grid-cols-3 gap-3">
              <div className="sm:col-span-2 space-y-1.5">
                <Label htmlFor="semantic-table-desc" className="text-xs">
                  {t('ai.semanticContext.tableDescription')}
                </Label>
                <Input
                  id="semantic-table-desc"
                  value={description}
                  onChange={e => setDescription(e.target.value)}
                  placeholder={t('ai.semanticContext.tableDescriptionPlaceholder')}
                  className="h-8 text-xs"
                />
              </div>
              <div className="space-y-1.5">
                <Label htmlFor="semantic-table-domain" className="text-xs">
                  {t('ai.semanticContext.domain')}
                </Label>
                <Input
                  id="semantic-table-domain"
                  value={domain}
                  onChange={e => setDomain(e.target.value)}
                  placeholder={t('ai.semanticContext.domainPlaceholder')}
                  className="h-8 text-xs"
                />
              </div>
            </div>

            {tableColumns.length > 0 && (
              <div className="space-y-1.5">
                <Label className="text-xs">{t('ai.semanticContext.columnsTitle')}</Label>
                <div className="max-h-40 overflow-y-auto rounded-md border border-border divide-y divide-border/60">
                  {tableColumns.map(col => (
                    <div key={col.name} className="flex items-center gap-2 px-2.5 py-1.5">
                      <span
                        className="font-mono text-[11px] w-36 truncate shrink-0"
                        title={`${col.name} (${col.data_type})`}
                      >
                        {col.name}
                      </span>
                      <Input
                        value={columnNotes[col.name] ?? ''}
                        onChange={e =>
                          setColumnNotes(prev => ({
                            ...prev,
                            [col.name]: e.target.value,
                          }))
                        }
                        placeholder={t('ai.semanticContext.columnPlaceholder', {
                          type: col.data_type,
                        })}
                        className="h-7 text-xs flex-1"
                      />
                    </div>
                  ))}
                </div>
              </div>
            )}

            <div className="space-y-1.5">
              <Label htmlFor="semantic-implicit-rel" className="text-xs">
                {t('ai.semanticContext.implicitRelations')}
              </Label>
              <textarea
                id="semantic-implicit-rel"
                value={implicitRelationsText}
                onChange={e => setImplicitRelationsText(e.target.value)}
                placeholder={t('ai.semanticContext.implicitRelationsPlaceholder')}
                rows={2}
                className="w-full rounded-md border border-input bg-background px-2.5 py-1.5 text-xs font-mono focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-ring"
              />
            </div>

            <div className="space-y-1.5">
              <Label htmlFor="semantic-business-rules" className="text-xs">
                {t('ai.semanticContext.businessRules')}
              </Label>
              <textarea
                id="semantic-business-rules"
                value={businessRulesText}
                onChange={e => setBusinessRulesText(e.target.value)}
                placeholder={t('ai.semanticContext.businessRulesPlaceholder')}
                rows={3}
                className="w-full rounded-md border border-input bg-background px-2.5 py-1.5 text-xs focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-ring"
              />
            </div>
          </div>
        )}

        <DialogFooter className="pt-2">
          <Button
            type="button"
            variant="ghost"
            size="sm"
            onClick={() => onOpenChange(false)}
            disabled={saving}
          >
            {t('common.cancel')}
          </Button>
          <Button type="button" size="sm" onClick={handleSave} disabled={loading || saving}>
            {saving && <Loader2 className="mr-1.5 h-3.5 w-3.5 animate-spin" />}
            {t('common.save')}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
export default SemanticContextDialog;
