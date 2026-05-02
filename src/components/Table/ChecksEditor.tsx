// SPDX-License-Identifier: Apache-2.0

import { Plus, Trash2 } from 'lucide-react';
import { useTranslation } from 'react-i18next';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { Textarea } from '@/components/ui/textarea';
import type { CheckConstraintDef } from '@/lib/ddl';
import type { EditableCheck } from './createTableTypes';

interface ChecksEditorProps {
  checks: EditableCheck[];
  onAdd: () => void;
  onUpdate: (index: number, updates: Partial<CheckConstraintDef>) => void;
  onRemove: (index: number) => void;
  disabled?: boolean;
}

export function ChecksEditor({ checks, onAdd, onUpdate, onRemove, disabled }: ChecksEditorProps) {
  const { t } = useTranslation();

  if (disabled) {
    return <p className="text-sm text-muted-foreground">{t('createTable.checksUnsupported')}</p>;
  }

  return (
    <div className="space-y-3">
      <div className="flex items-center justify-between">
        <p className="text-sm font-medium">{t('createTable.checks')}</p>
        <Button variant="outline" size="sm" onClick={onAdd}>
          <Plus size={14} className="mr-1" />
          {t('createTable.addCheck')}
        </Button>
      </div>

      {checks.length === 0 && (
        <p className="text-xs text-muted-foreground">{t('createTable.noChecks')}</p>
      )}

      <div className="space-y-3">
        {checks.map((check, index) => (
          <div key={check._id} className="bg-muted/30 rounded-md p-3 space-y-2">
            <div className="grid grid-cols-[1fr_2fr_auto] gap-3 items-start">
              <div className="space-y-1">
                <label htmlFor={`chk-name-${check._id}`} className="text-xs text-muted-foreground">
                  {t('createTable.constraintName')}
                </label>
                <Input
                  id={`chk-name-${check._id}`}
                  value={check.name ?? ''}
                  onChange={e => onUpdate(index, { name: e.target.value || undefined })}
                  placeholder="chk_positive_amount"
                  className="h-8 text-sm"
                />
              </div>
              <div className="space-y-1">
                <label htmlFor={`chk-expr-${check._id}`} className="text-xs text-muted-foreground">
                  {t('createTable.checkExpression')}
                </label>
                <Textarea
                  id={`chk-expr-${check._id}`}
                  value={check.expression}
                  onChange={e => onUpdate(index, { expression: e.target.value })}
                  rows={2}
                  placeholder={t('createTable.checkExpressionPlaceholder')}
                  className="text-sm font-mono"
                />
              </div>
              <Button
                variant="ghost"
                size="icon"
                className="h-8 w-8 text-destructive hover:text-destructive mt-5"
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
