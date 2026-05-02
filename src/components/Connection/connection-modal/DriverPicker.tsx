// SPDX-License-Identifier: Apache-2.0

import { ScrollArea } from '@/components/ui/scroll-area';
import type { Driver } from '@/lib/connection/drivers';
import { DRIVER_ICONS, DRIVER_LABELS } from '@/lib/connection/drivers';
import { cn } from '@/lib/utils';

export function DriverPicker(props: {
  driver: Driver;
  isEditMode: boolean;
  onChange: (driver: Driver) => void;
}) {
  const { driver, isEditMode, onChange } = props;

  return (
    <ScrollArea className="max-h-[60vh]">
      <div className="grid grid-cols-2 sm:grid-cols-3 lg:grid-cols-4 gap-3 px-1 py-1 pr-3">
        {(Object.keys(DRIVER_LABELS) as Driver[]).map(d => (
          <button
            key={d}
            type="button"
            className={cn(
              'flex flex-col items-center gap-2 p-3 rounded-xl border-2 transition-all hover:scale-[1.02] active:scale-[0.98]',
              driver === d
                ? 'border-accent bg-accent/5'
                : 'border-border bg-background hover:border-foreground/20 hover:bg-muted/50'
            )}
            onClick={() => onChange(d)}
            disabled={isEditMode}
          >
            <div
              className={cn(
                'flex items-center justify-center w-12 h-12 rounded-xl p-2 transition-colors shadow-sm',
                driver === d ? 'bg-accent/10' : 'bg-muted'
              )}
            >
              <img
                src={`/databases/${DRIVER_ICONS[d]}`}
                alt={DRIVER_LABELS[d]}
                className="w-full h-full object-contain"
              />
            </div>
            <span
              className={cn(
                'text-xs font-semibold text-center',
                driver === d ? 'text-accent' : 'text-foreground'
              )}
            >
              {DRIVER_LABELS[d]}
            </span>
          </button>
        ))}
      </div>
    </ScrollArea>
  );
}
