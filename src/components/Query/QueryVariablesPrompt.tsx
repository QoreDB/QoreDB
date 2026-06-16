// SPDX-License-Identifier: Apache-2.0

import { useEffect, useMemo, useState } from 'react';
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
import { Label } from '@/components/ui/label';
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select';
import { extractVariableReferences, substituteVariables } from '@/lib/notebook/notebookVariables';
import type { QueryVariable } from '@/lib/query/queryLibrary';

interface QueryVariablesPromptProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  title: string;
  query: string;
  variables?: Record<string, QueryVariable>;
  /** Receives the query with all placeholders substituted. */
  onSubmit: (resolvedQuery: string) => void;
}

/**
 * Resolves the variables actually referenced in the query, falling back to a
 * plain text definition for any placeholder without a stored schema.
 */
function resolveDefinitions(
  query: string,
  variables?: Record<string, QueryVariable>
): QueryVariable[] {
  return extractVariableReferences(query).map(name => {
    const def = variables?.[name];
    return def ?? { name, type: 'text' };
  });
}

export function QueryVariablesPrompt({
  open,
  onOpenChange,
  title,
  query,
  variables,
  onSubmit,
}: QueryVariablesPromptProps) {
  const { t } = useTranslation();
  const definitions = useMemo(() => resolveDefinitions(query, variables), [query, variables]);
  const [values, setValues] = useState<Record<string, string>>({});

  useEffect(() => {
    if (!open) return;
    const initial: Record<string, string> = {};
    for (const def of definitions) {
      initial[def.name] = def.currentValue ?? def.defaultValue ?? '';
    }
    setValues(initial);
  }, [open, definitions]);

  const allFilled = definitions.every(def => (values[def.name] ?? '').trim().length > 0);

  function handleSubmit() {
    const resolved: Record<string, QueryVariable> = {};
    for (const def of definitions) {
      resolved[def.name] = { ...def, currentValue: values[def.name] };
    }
    onSubmit(substituteVariables(query, resolved));
    onOpenChange(false);
  }

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="max-w-md">
        <DialogHeader>
          <DialogTitle>{t('library.variables.promptTitle', { title })}</DialogTitle>
        </DialogHeader>

        <div className="grid gap-4 py-2">
          {definitions.map(def => (
            <div key={def.name} className="grid gap-2">
              <Label htmlFor={`qv-${def.name}`}>
                {def.name}
                {def.description ? (
                  <span className="ml-2 font-normal text-xs text-muted-foreground">
                    {def.description}
                  </span>
                ) : null}
              </Label>
              {def.type === 'select' && def.options && def.options.length > 0 ? (
                <Select
                  value={values[def.name] ?? ''}
                  onValueChange={value => setValues(prev => ({ ...prev, [def.name]: value }))}
                >
                  <SelectTrigger id={`qv-${def.name}`}>
                    <SelectValue placeholder={t('library.variables.selectPlaceholder')} />
                  </SelectTrigger>
                  <SelectContent>
                    {def.options.map(option => (
                      <SelectItem key={option} value={option}>
                        {option}
                      </SelectItem>
                    ))}
                  </SelectContent>
                </Select>
              ) : (
                <Input
                  id={`qv-${def.name}`}
                  type={def.type === 'number' ? 'number' : def.type === 'date' ? 'date' : 'text'}
                  value={values[def.name] ?? ''}
                  onChange={e => setValues(prev => ({ ...prev, [def.name]: e.target.value }))}
                />
              )}
            </div>
          ))}
        </div>

        <DialogFooter>
          <Button variant="outline" onClick={() => onOpenChange(false)}>
            {t('common.cancel')}
          </Button>
          <Button onClick={handleSubmit} disabled={!allFilled}>
            {t('library.variables.apply')}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
