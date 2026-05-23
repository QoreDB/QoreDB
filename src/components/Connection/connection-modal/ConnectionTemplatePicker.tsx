// SPDX-License-Identifier: Apache-2.0

import { Sparkles } from 'lucide-react';
import { useMemo } from 'react';
import { useTranslation } from 'react-i18next';
import { Button } from '@/components/ui/button';
import { Tooltip, TooltipProvider } from '@/components/ui/tooltip';
import { Driver } from '@/lib/connection/drivers';
import type { ConnectionTemplateContribution } from '@/lib/plugins';
import { usePlugins } from '@/providers/PluginProvider';
import type { ConnectionFormData } from './types';

interface ConnectionTemplatePickerProps {
  driver: Driver;
  onApply: (field: keyof ConnectionFormData, value: string | number | boolean) => void;
}

const APPLIABLE_KEYS: ReadonlyArray<keyof ConnectionFormData> = [
  'name',
  'host',
  'port',
  'username',
  'database',
  'ssl',
  'sslMode',
  'environment',
  'readOnly',
];

/** Maps the driver string a template declares onto our Driver enum. Common
 *  aliases (`postgresql` → `postgres`) are accepted so a generic Postgres
 *  template matches our specialised drivers' base case. */
function templateMatchesDriver(templateDriver: string, current: Driver): boolean {
  const normalised = templateDriver.trim().toLowerCase();
  if (normalised === current) return true;
  if ((normalised === 'postgresql' || normalised === 'postgres') && current === Driver.Postgres) {
    return true;
  }
  if (normalised === 'sqlite3' && current === Driver.Sqlite) return true;
  return false;
}

function applyDefaults(
  defaults: ConnectionTemplateContribution['defaults'],
  onApply: ConnectionTemplatePickerProps['onApply']
): void {
  for (const key of APPLIABLE_KEYS) {
    if (!(key in defaults)) continue;
    const raw = defaults[key];
    if (typeof raw === 'string' || typeof raw === 'number' || typeof raw === 'boolean') {
      onApply(key, raw);
    }
  }
}

export function ConnectionTemplatePicker({ driver, onApply }: ConnectionTemplatePickerProps) {
  const { t } = useTranslation();
  const { contributions } = usePlugins();
  const templates = useMemo(
    () => contributions.connectionTemplates.filter(tpl => templateMatchesDriver(tpl.driver, driver)),
    [contributions.connectionTemplates, driver]
  );

  if (templates.length === 0) return null;

  return (
    <div className="rounded-md border border-border bg-muted/20 p-3 space-y-2">
      <div className="flex items-center gap-2 text-xs font-semibold text-foreground/80">
        <Sparkles size={12} className="text-primary" />
        {t('plugins.connectionTemplatePicker.title')}
      </div>
      <div className="flex flex-wrap gap-2">
        <TooltipProvider delayDuration={200}>
          {templates.map(tpl => (
            <Tooltip
              key={tpl.id}
              content={tpl.description ?? ''}
              side="bottom"
              className="max-w-xs text-xs"
            >
              <Button
                type="button"
                size="sm"
                variant="outline"
                className="h-7 px-2.5 text-xs"
                onClick={() => applyDefaults(tpl.defaults, onApply)}
              >
                {tpl.name}
              </Button>
            </Tooltip>
          ))}
        </TooltipProvider>
      </div>
    </div>
  );
}
