// SPDX-License-Identifier: Apache-2.0

import { Archive, Clock, Layers, Timer } from 'lucide-react';
import { useTranslation } from 'react-i18next';
import { Badge } from '@/components/ui/badge';
import type { Hypertable, HypertableDimension } from '@/lib/tauri';

function dimensionPartitioning(dimension: HypertableDimension): string | null {
  if (dimension.time_interval) return dimension.time_interval;
  if (dimension.integer_interval != null) return dimension.integer_interval.toLocaleString();
  if (dimension.num_partitions != null) return String(dimension.num_partitions);
  return null;
}

export function HypertableInfoSection({ hypertable }: { hypertable: Hypertable }) {
  const { t } = useTranslation();
  return (
    <section
      aria-labelledby="hypertable-info-title"
      className="border border-border rounded-md overflow-hidden"
    >
      <div className="px-3 py-2 bg-muted/50 border-b border-border flex items-center justify-between">
        <h3
          id="hypertable-info-title"
          className="text-xs font-semibold text-muted-foreground uppercase"
        >
          {t('timescale.hypertable')}
        </h3>
        {hypertable.num_chunks != null && (
          <span className="text-xs text-muted-foreground">
            {t('timescale.chunks', { count: hypertable.num_chunks })}
          </span>
        )}
      </div>
      <div className="divide-y divide-border">
        {hypertable.dimensions.map(dimension => {
          const partitioning = dimensionPartitioning(dimension);
          const isTime = dimension.dimension_type.toLowerCase() === 'time';
          return (
            <div
              key={dimension.column_name}
              className="flex items-center justify-between gap-3 px-3 py-2 text-sm"
            >
              <span className="flex items-center gap-2 min-w-0">
                {isTime ? (
                  <Clock size={14} className="shrink-0 text-muted-foreground" />
                ) : (
                  <Layers size={14} className="shrink-0 text-muted-foreground" />
                )}
                <span className="font-mono font-medium truncate">{dimension.column_name}</span>
                <Badge variant="outline">
                  {isTime ? t('timescale.timeDimension') : t('timescale.spaceDimension')}
                </Badge>
              </span>
              {partitioning && (
                <span className="shrink-0 font-mono text-xs text-muted-foreground">
                  {isTime || dimension.integer_interval != null
                    ? t('timescale.chunkInterval', { value: partitioning })
                    : t('timescale.partitions', { value: partitioning })}
                </span>
              )}
            </div>
          );
        })}
        <div className="flex items-center gap-2 px-3 py-2 text-sm">
          <Archive size={14} className="text-muted-foreground" />
          <span className="text-muted-foreground">{t('timescale.compression')}:</span>
          <span className="font-mono text-xs">
            {!hypertable.compression_enabled
              ? t('timescale.disabled')
              : hypertable.compress_after
                ? t('timescale.after', { value: hypertable.compress_after })
                : t('timescale.enabledNoPolicy')}
          </span>
        </div>
        <div className="flex items-center gap-2 px-3 py-2 text-sm">
          <Timer size={14} className="text-muted-foreground" />
          <span className="text-muted-foreground">{t('timescale.retention')}:</span>
          <span className="font-mono text-xs">
            {hypertable.drop_after
              ? t('timescale.dropAfter', { value: hypertable.drop_after })
              : t('timescale.noPolicy')}
          </span>
        </div>
      </div>
    </section>
  );
}
