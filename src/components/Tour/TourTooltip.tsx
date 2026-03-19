// SPDX-License-Identifier: Apache-2.0

import { useTranslation } from 'react-i18next';
import { Button } from '@/components/ui/button';

interface TourTooltipProps {
  title: string;
  description: string;
  stepIndex: number;
  totalSteps: number;
  onNext: () => void;
  onDismiss: () => void;
}

export function TourTooltip({
  title,
  description,
  stepIndex,
  totalSteps,
  onNext,
  onDismiss,
}: TourTooltipProps) {
  const { t } = useTranslation();
  const isLast = stepIndex === totalSteps - 1;

  return (
    <div className="w-72 rounded-lg border border-border bg-popover p-4 shadow-xl text-popover-foreground">
      <h4 className="text-sm font-semibold mb-1">{title}</h4>
      <p className="text-xs text-muted-foreground leading-relaxed mb-3">{description}</p>
      <div className="flex items-center justify-between">
        <span className="text-[11px] text-muted-foreground">
          {t('tour.stepOf', { current: stepIndex + 1, total: totalSteps })}
        </span>
        <div className="flex items-center gap-2">
          <Button
            variant="ghost"
            size="sm"
            className="h-7 px-2 text-xs text-muted-foreground"
            onClick={onDismiss}
          >
            {t('tour.dismiss')}
          </Button>
          <Button size="sm" className="h-7 px-3 text-xs" onClick={onNext}>
            {isLast ? t('tour.gotIt') : t('tour.next')}
          </Button>
        </div>
      </div>
    </div>
  );
}
