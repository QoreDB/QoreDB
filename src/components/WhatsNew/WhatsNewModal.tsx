// SPDX-License-Identifier: Apache-2.0

import type { LucideIcon } from 'lucide-react';
import { Sparkles, TrendingUp, Wrench } from 'lucide-react';
import { useEffect } from 'react';
import { useTranslation } from 'react-i18next';
import { LicenseBadge } from '@/components/License/LicenseBadge';
import { AnalyticsService } from '@/components/Onboarding/AnalyticsService';
import { Button } from '@/components/ui/button';
import { Dialog, DialogContent, DialogDescription, DialogTitle } from '@/components/ui/dialog';
import type { ChangelogEntry, ChangelogItem } from '@/data/changelog';

const TYPE_ICON: Record<ChangelogItem['type'], LucideIcon> = {
  feature: Sparkles,
  improvement: TrendingUp,
  fix: Wrench,
};

const TYPE_COLOR: Record<ChangelogItem['type'], string> = {
  feature: '#6B5CFF',
  improvement: '#10B981',
  fix: '#F59E0B',
};

const MAX_ITEMS = 5;

interface WhatsNewModalProps {
  open: boolean;
  entry: ChangelogEntry | null;
  onClose: () => void;
}

export function WhatsNewModal({ open, entry, onClose }: WhatsNewModalProps) {
  const { t } = useTranslation();

  useEffect(() => {
    if (open && entry) {
      AnalyticsService.capture('whats_new_seen', { version: entry.version });
    }
  }, [open, entry]);

  if (!entry) return null;

  const items = entry.items.slice(0, MAX_ITEMS);

  return (
    <Dialog open={open} onOpenChange={value => !value && onClose()}>
      <DialogContent className="max-w-xl gap-0 p-0">
        <div
          className="border-b px-6 py-5"
          style={{
            background:
              'linear-gradient(180deg, rgba(107, 92, 255, 0.06) 0%, rgba(107, 92, 255, 0.01) 100%)',
            borderColor: 'rgba(107, 92, 255, 0.18)',
          }}
        >
          <DialogTitle className="text-lg font-semibold">
            {t('whatsNew.title', "What's new in QoreDB {{version}}", { version: entry.version })}
          </DialogTitle>
          <DialogDescription className="mt-1 text-xs">
            {t('whatsNew.subtitle', 'A quick look at what changed since your last session.')}
          </DialogDescription>
        </div>

        <ul className="flex flex-col divide-y">
          {items.map(item => {
            const Icon = TYPE_ICON[item.type];
            return (
              <li key={item.title} className="flex items-start gap-3 px-6 py-4">
                <div
                  className="flex h-8 w-8 shrink-0 items-center justify-center rounded-md"
                  style={{
                    background: `${TYPE_COLOR[item.type]}1F`,
                    color: TYPE_COLOR[item.type],
                  }}
                >
                  <Icon size={15} aria-hidden />
                </div>
                <div className="flex min-w-0 flex-col gap-1">
                  <div className="flex flex-wrap items-center gap-2">
                    <span className="text-sm font-medium text-(--color-text-primary)">
                      {item.title}
                    </span>
                    {item.proOnly && <LicenseBadge tier="pro" />}
                  </div>
                  <p className="text-xs text-(--color-text-secondary)">{item.description}</p>
                </div>
              </li>
            );
          })}
        </ul>

        <div className="flex items-center justify-end gap-2 border-t bg-muted/20 px-6 py-3">
          <Button size="sm" onClick={onClose} className="text-xs">
            {t('whatsNew.dismiss', 'Got it')}
          </Button>
        </div>
      </DialogContent>
    </Dialog>
  );
}
