// SPDX-License-Identifier: Apache-2.0

import { Search } from 'lucide-react';
import { useState } from 'react';
import { useTranslation } from 'react-i18next';
import { Input } from '@/components/ui/input';
import type { DataModel, Driver } from '@/lib/connection/drivers';
import { DATA_MODEL_ORDER, DRIVERS } from '@/lib/connection/drivers';
import { cn } from '@/lib/utils';

const MODEL_LABEL_KEYS: Record<DataModel, string> = {
  relational: 'connection.driverGroups.relational',
  document: 'connection.driverGroups.document',
  'key-value': 'connection.driverGroups.keyValue',
  'time-series': 'connection.driverGroups.timeSeries',
  search: 'connection.driverGroups.search',
  'wide-column': 'connection.driverGroups.wideColumn',
  graph: 'connection.driverGroups.graph',
};

export function DriverPicker(props: {
  driver: Driver;
  isEditMode: boolean;
  onChange: (driver: Driver) => void;
}) {
  const { driver, isEditMode, onChange } = props;
  const { t } = useTranslation();
  const [query, setQuery] = useState('');
  const [model, setModel] = useState<DataModel | null>(null);

  const all = Object.values(DRIVERS);
  const filters = DATA_MODEL_ORDER.filter(m => all.some(meta => meta.dataModel === m)).map(m => ({
    model: m,
    labelKey: MODEL_LABEL_KEYS[m],
  }));

  const needle = query.trim().toLowerCase();
  const visible = all.filter(
    meta =>
      (model === null || meta.dataModel === model) && meta.label.toLowerCase().includes(needle)
  );

  return (
    <div className="space-y-3">
      <div className="relative px-1">
        <Search
          size={14}
          className="absolute left-3 top-1/2 -translate-y-1/2 text-muted-foreground"
        />
        <Input
          value={query}
          onChange={e => setQuery(e.target.value)}
          placeholder={t('connection.driverSearch')}
          disabled={isEditMode}
          autoFocus
          className="pl-9"
        />
      </div>

      <div className="flex flex-wrap gap-2 px-1">
        {[{ model: null, labelKey: 'connection.driverGroups.all' }, ...filters].map(filter => (
          <button
            key={filter.model ?? 'all'}
            type="button"
            onClick={() => setModel(filter.model)}
            disabled={isEditMode}
            className={cn(
              'inline-flex items-center gap-1.5 px-2.5 py-1 text-xs rounded-full border transition-colors',
              model === filter.model
                ? 'bg-primary text-primary-foreground border-primary'
                : 'bg-muted/50 text-muted-foreground border-border hover:bg-muted hover:text-foreground'
            )}
          >
            {t(filter.labelKey)}
          </button>
        ))}
      </div>

      {/* Native scroll: Radix ScrollArea sizes its viewport with `h-full`, which
          resolves to `auto` under a `max-h` root — the content is clipped and
          nothing scrolls. */}
      <div className="max-h-[55vh] overflow-y-auto">
        <div className="grid grid-cols-2 sm:grid-cols-3 lg:grid-cols-4 gap-3 px-1 py-1 pr-3">
          {visible.map(meta => (
            <button
              key={meta.id}
              type="button"
              className={cn(
                'flex flex-col items-center gap-2 p-3 rounded-xl border-2 transition-all hover:scale-[1.02] active:scale-[0.98]',
                driver === meta.id
                  ? 'border-accent bg-accent/5'
                  : 'border-border bg-background hover:border-foreground/20 hover:bg-muted/50'
              )}
              onClick={() => onChange(meta.id)}
              disabled={isEditMode}
            >
              <div
                className={cn(
                  'flex items-center justify-center w-12 h-12 rounded-xl p-2 transition-colors shadow-sm',
                  driver === meta.id ? 'bg-accent/10' : 'bg-muted'
                )}
              >
                <img
                  src={`/databases/${meta.icon}`}
                  alt={meta.label}
                  className="w-full h-full object-contain"
                />
              </div>
              <span
                className={cn(
                  'text-xs font-semibold text-center',
                  driver === meta.id ? 'text-accent' : 'text-foreground'
                )}
              >
                {meta.label}
              </span>
            </button>
          ))}
        </div>

        {visible.length === 0 && (
          <p className="py-8 text-center text-sm text-muted-foreground">
            {t('connection.driverSearchEmpty', { query: query.trim() })}
          </p>
        )}
      </div>
    </div>
  );
}
