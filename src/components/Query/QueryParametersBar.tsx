// SPDX-License-Identifier: Apache-2.0

import { Braces, Play, RotateCcw } from 'lucide-react';
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
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select';
import {
  formatQueryParameter,
  type QueryParameterType,
  type QueryParameterValue,
} from '@/lib/query/queryParameters';
import { cn } from '@/lib/utils';

interface QueryParametersDialogProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  parameters: string[];
  values: Record<string, QueryParameterValue>;
  onChange: (name: string, next: QueryParameterValue) => void;
  onReset: () => void;
  onConfirmExecute: () => void;
}

const PARAMETER_TYPES: QueryParameterType[] = ['auto', 'text', 'number', 'date', 'null'];

export function QueryParametersDialog({
  open,
  onOpenChange,
  parameters,
  values,
  onChange,
  onReset,
  onConfirmExecute,
}: QueryParametersDialogProps) {
  const { t } = useTranslation();

  const formattedByName = Object.fromEntries(
    parameters.map(name => {
      const current: QueryParameterValue = values[name] ?? { value: '', type: 'auto' };
      return [name, formatQueryParameter(current)];
    })
  );

  const allValid =
    parameters.length > 0 && parameters.every(name => formattedByName[name] !== null);
  const firstUnfilled = parameters.find(name => formattedByName[name] === null) ?? parameters[0];
  const hasAnyValue = parameters.some(name => {
    const entry = values[name];
    return entry && (entry.type === 'null' || entry.value.length > 0);
  });

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="max-w-lg" data-testid="query-parameters-dialog">
        <DialogHeader>
          <DialogTitle className="flex items-center gap-2 text-base">
            <Braces size={16} className="text-accent" />
            {t('query.parameters.dialogTitle')}
          </DialogTitle>
          <DialogDescription>{t('query.parameters.dialogDescription')}</DialogDescription>
        </DialogHeader>

        <div className="space-y-3 py-1 max-h-[60vh] overflow-y-auto pr-1">
          {parameters.map(name => {
            const current: QueryParameterValue = values[name] ?? { value: '', type: 'auto' };
            const isNull = current.type === 'null';
            const preview = formattedByName[name];
            const isInvalid = !isNull && current.value.length > 0 && preview === null;

            return (
              <div key={name} className="space-y-1">
                <div className="flex items-center justify-between gap-2 text-xs">
                  <span className="font-mono font-medium text-foreground">{name}</span>
                  <span
                    className={cn(
                      'font-mono text-[11px] truncate max-w-56',
                      preview !== null
                        ? 'text-muted-foreground'
                        : 'text-amber-600 dark:text-amber-400'
                    )}
                  >
                    {preview !== null ? `→ ${preview}` : t('query.parameters.required')}
                  </span>
                </div>

                <div
                  className={cn(
                    'flex items-center rounded-md border bg-background overflow-hidden',
                    isInvalid ? 'border-destructive' : 'border-border'
                  )}
                >
                  <Select
                    value={current.type}
                    onValueChange={nextType =>
                      onChange(name, {
                        value: nextType === 'null' ? '' : current.value,
                        type: nextType as QueryParameterType,
                      })
                    }
                  >
                    <SelectTrigger
                      aria-label={`${name} ${t('notebook.variableType')}`}
                      className="h-8 w-24 border-0 rounded-none border-r border-border px-2.5 text-xs focus:ring-0"
                    >
                      <SelectValue />
                    </SelectTrigger>
                    <SelectContent>
                      {PARAMETER_TYPES.map(type => (
                        <SelectItem key={type} value={type} className="text-xs">
                          {t(`query.parameters.types.${type}`)}
                        </SelectItem>
                      ))}
                    </SelectContent>
                  </Select>
                  <Input
                    aria-label={name}
                    autoFocus={name === firstUnfilled}
                    type={current.type === 'date' ? 'date' : 'text'}
                    inputMode={current.type === 'number' ? 'decimal' : undefined}
                    disabled={isNull}
                    value={isNull ? 'NULL' : current.value}
                    placeholder={t('query.parameters.valuePlaceholder')}
                    onChange={e => onChange(name, { value: e.target.value, type: current.type })}
                    onKeyDown={e => {
                      if (e.key === 'Enter' && !e.shiftKey) {
                        e.preventDefault();
                        if (allValid) {
                          onConfirmExecute();
                        }
                      }
                    }}
                    className="h-8 flex-1 border-0 rounded-none px-2.5 text-xs font-mono focus-visible:ring-0"
                  />
                </div>
              </div>
            );
          })}
        </div>

        <DialogFooter className="gap-2 sm:justify-between">
          <div>
            {hasAnyValue && (
              <Button type="button" variant="ghost" size="sm" onClick={onReset} className="gap-1.5">
                <RotateCcw size={13} />
                {t('query.parameters.clear')}
              </Button>
            )}
          </div>
          <div className="flex items-center gap-2">
            <Button type="button" variant="outline" size="sm" onClick={() => onOpenChange(false)}>
              {t('common.cancel')}
            </Button>
            <Button
              type="button"
              size="sm"
              disabled={!allValid}
              onClick={onConfirmExecute}
              className="gap-1.5"
            >
              <Play size={13} className="fill-current" />
              {t('query.parameters.runWithParams')}
            </Button>
          </div>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}

export default QueryParametersDialog;
