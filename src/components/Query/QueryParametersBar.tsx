// SPDX-License-Identifier: Apache-2.0

import { Braces, RotateCcw } from 'lucide-react';
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
import { Tooltip } from '@/components/ui/tooltip';
import {
  formatQueryParameter,
  type QueryParameterType,
  type QueryParameterValue,
} from '@/lib/query/queryParameters';
import { cn } from '@/lib/utils';

interface QueryParametersBarProps {
  parameters: string[];
  values: Record<string, QueryParameterValue>;
  onChange: (name: string, next: QueryParameterValue) => void;
  onReset: () => void;
  onExecute: () => void;
}

const PARAMETER_TYPES: QueryParameterType[] = ['text', 'number', 'date', 'null'];

export function QueryParametersBar({
  parameters,
  values,
  onChange,
  onReset,
  onExecute,
}: QueryParametersBarProps) {
  const { t } = useTranslation();

  if (parameters.length === 0) return null;

  const hasAnyValue = parameters.some(name => {
    const entry = values[name];
    return entry && (entry.type === 'null' || entry.value.length > 0);
  });

  return (
    <div
      data-testid="query-parameters-bar"
      className="flex flex-wrap items-center gap-2 border-b border-border bg-muted/20 px-3 py-1.5"
    >
      <div className="flex items-center gap-1.5 text-xs font-medium text-muted-foreground shrink-0">
        <Braces size={13} />
        <span>{t('query.parameters.title')}</span>
      </div>

      {parameters.map(name => {
        const current: QueryParameterValue = values[name] ?? { value: '', type: 'text' };
        const isNull = current.type === 'null';
        const isInvalid =
          !isNull && current.value.length > 0 && formatQueryParameter(current) === null;

        return (
          <div
            key={name}
            className={cn(
              'flex items-center rounded-md border bg-background overflow-hidden',
              isInvalid ? 'border-destructive' : 'border-border'
            )}
          >
            <span className="px-2 py-1 text-[11px] font-mono text-muted-foreground bg-muted/40 border-r border-border">
              {name}
            </span>
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
                className="h-7 w-[76px] border-0 rounded-none border-r border-border px-2 text-[11px] focus:ring-0"
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
              type={current.type === 'date' ? 'date' : 'text'}
              inputMode={current.type === 'number' ? 'decimal' : undefined}
              disabled={isNull}
              value={isNull ? 'NULL' : current.value}
              placeholder={t('query.parameters.valuePlaceholder')}
              onChange={e => onChange(name, { value: e.target.value, type: current.type })}
              onKeyDown={e => {
                if (e.key === 'Enter' && !e.shiftKey) {
                  e.preventDefault();
                  onExecute();
                }
              }}
              className="h-7 w-32 border-0 rounded-none px-2 text-xs font-mono focus-visible:ring-0"
            />
          </div>
        );
      })}

      {hasAnyValue && (
        <Tooltip content={t('query.parameters.clear')}>
          <Button
            type="button"
            variant="ghost"
            size="icon"
            className="h-7 w-7 text-muted-foreground"
            onClick={onReset}
            aria-label={t('query.parameters.clear')}
          >
            <RotateCcw size={13} />
          </Button>
        </Tooltip>
      )}
    </div>
  );
}

export default QueryParametersBar;
