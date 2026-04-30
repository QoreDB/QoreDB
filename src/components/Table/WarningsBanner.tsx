// SPDX-License-Identifier: Apache-2.0

import { AlertTriangle, ChevronDown, ChevronRight } from 'lucide-react';
import { useState } from 'react';
import { useTranslation } from 'react-i18next';
import type { DdlWarning } from '@/lib/ddl';
import { translateDdlWarning } from './translateDdlWarning';

interface WarningsBannerProps {
  warnings: DdlWarning[];
  defaultOpen?: boolean;
}

export function WarningsBanner({ warnings, defaultOpen = false }: WarningsBannerProps) {
  const { t } = useTranslation();
  const [open, setOpen] = useState(defaultOpen);

  if (warnings.length === 0) return null;

  return (
    <div className="rounded-md border border-amber-500/40 bg-amber-500/10 px-3 py-2">
      <button
        type="button"
        onClick={() => setOpen(o => !o)}
        className="flex w-full items-center gap-2 text-amber-700 dark:text-amber-300 text-sm font-medium"
      >
        {open ? <ChevronDown size={14} /> : <ChevronRight size={14} />}
        <AlertTriangle size={14} />
        <span>{t('createTable.warningsCount', { count: warnings.length })}</span>
      </button>
      {open && (
        <ul className="mt-2 list-disc list-inside text-xs text-amber-700 dark:text-amber-300 space-y-0.5 pl-1">
          {warnings.map((w, i) => (
            // biome-ignore lint/suspicious/noArrayIndexKey: warnings are regenerated deterministically; positional index is stable
            <li key={`${w.code}-${i}`}>{translateDdlWarning(t, w)}</li>
          ))}
        </ul>
      )}
    </div>
  );
}
