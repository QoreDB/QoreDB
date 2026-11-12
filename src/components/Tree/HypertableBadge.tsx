// SPDX-License-Identifier: Apache-2.0

import { Clock } from 'lucide-react';
import { useTranslation } from 'react-i18next';

export function HypertableBadge() {
  const { t } = useTranslation();
  return (
    <span className="ml-auto inline-flex shrink-0 items-center gap-0.5 rounded border border-accent/30 bg-accent/10 px-1 text-[10px] text-accent">
      <Clock size={10} aria-hidden="true" />
      {t('timescale.hypertable')}
    </span>
  );
}
