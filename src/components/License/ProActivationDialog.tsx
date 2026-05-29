// SPDX-License-Identifier: Apache-2.0

import type { LucideIcon } from 'lucide-react';
import {
  Bot,
  CheckCircle2,
  ChevronLeft,
  ChevronRight,
  FileSpreadsheet,
  History,
  Sparkles,
} from 'lucide-react';
import { useEffect, useMemo, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { Button } from '@/components/ui/button';
import { Dialog, DialogContent, DialogDescription, DialogTitle } from '@/components/ui/dialog';
import type { ProFeature } from '@/lib/license';
import { useLicense } from '@/providers/LicenseProvider';

interface ProActivationItem {
  icon: LucideIcon;
  titleKey: string;
  descriptionKey: string;
}

const ACTIVATION_ITEMS: ProActivationItem[] = [
  {
    icon: Bot,
    titleKey: 'proActivation.benefits.ai.title',
    descriptionKey: 'proActivation.benefits.ai.description',
  },
  {
    icon: History,
    titleKey: 'proActivation.benefits.history.title',
    descriptionKey: 'proActivation.benefits.history.description',
  },
  {
    icon: FileSpreadsheet,
    titleKey: 'proActivation.benefits.exports.title',
    descriptionKey: 'proActivation.benefits.exports.description',
  },
];

const PRO_FEATURES: ProFeature[] = [
  'sandbox',
  'visual_diff',
  'ai',
  'data_time_travel',
  'data_contracts',
  'instant_api',
  'audit_advanced',
  'profiling',
  'export_xlsx',
  'export_parquet',
  'custom_safety_rules',
  'query_library_advanced',
  'virtual_relations_auto_suggest',
  'bulk_edit_unlimited',
];

export function ProActivationDialog() {
  const { t } = useTranslation();
  const { proActivation, dismissProActivation } = useLicense();
  const [view, setView] = useState<'summary' | 'tour'>('summary');
  const [step, setStep] = useState(0);

  useEffect(() => {
    if (!proActivation) return;
    setView('summary');
    setStep(0);
  }, [proActivation]);

  const activeItem = ACTIVATION_ITEMS[step];
  const ActiveIcon = activeItem.icon;
  const progressLabel = useMemo(
    () => t('proActivation.tour.progress', { current: step + 1, total: ACTIVATION_ITEMS.length }),
    [step, t]
  );

  const handleClose = () => {
    dismissProActivation();
  };

  return (
    <>
      {proActivation && <div className="pro-activation-sweep" aria-hidden="true" />}
      <Dialog open={!!proActivation} onOpenChange={open => !open && handleClose()}>
        <DialogContent className="max-h-[86vh] max-w-2xl gap-0 overflow-hidden p-0">
          {view === 'summary' ? (
            <>
              <div className="border-b bg-muted/20 px-6 py-5">
                <div className="mb-4 flex items-center gap-3">
                  <div className="pro-activation-mark flex h-10 w-10 items-center justify-center rounded-md border border-accent/20 bg-accent/10 text-accent">
                    <CheckCircle2 size={22} aria-hidden="true" />
                  </div>
                  <div>
                    <DialogTitle className="text-lg font-semibold">
                      {t('proActivation.title')}
                    </DialogTitle>
                    <DialogDescription className="mt-1">
                      {t('proActivation.description')}
                    </DialogDescription>
                  </div>
                </div>
                <div className="flex items-center gap-2 rounded-md border border-accent/20 bg-accent/5 px-3 py-2 text-xs text-muted-foreground">
                  <Sparkles size={14} className="text-accent" aria-hidden="true" />
                  <span>{t('proActivation.statusLine')}</span>
                </div>
              </div>

              <div className="max-h-[56vh] overflow-y-auto p-4">
                <section>
                  <div className="mb-2">
                    <h3 className="text-sm font-semibold text-foreground">
                      {t('proActivation.starterTitle')}
                    </h3>
                    <p className="mt-0.5 text-xs text-muted-foreground">
                      {t('proActivation.starterDescription')}
                    </p>
                  </div>
                  <div className="grid gap-2">
                    {ACTIVATION_ITEMS.map(({ icon: Icon, titleKey, descriptionKey }) => (
                      <div
                        key={titleKey}
                        className="flex items-start gap-3 rounded-md border bg-background px-3 py-3"
                      >
                        <Icon
                          size={16}
                          className="mt-0.5 shrink-0 text-accent"
                          aria-hidden="true"
                        />
                        <div className="min-w-0">
                          <div className="text-sm font-medium text-foreground">{t(titleKey)}</div>
                          <div className="mt-0.5 text-xs leading-relaxed text-muted-foreground">
                            {t(descriptionKey)}
                          </div>
                        </div>
                      </div>
                    ))}
                  </div>
                </section>

                <section className="mt-4 rounded-md border bg-muted/10 p-3">
                  <div className="mb-3">
                    <h3 className="text-sm font-semibold text-foreground">
                      {t('proActivation.allFeaturesTitle')}
                    </h3>
                    <p className="mt-0.5 text-xs text-muted-foreground">
                      {t('proActivation.allFeaturesDescription')}
                    </p>
                  </div>
                  <div className="grid gap-1.5 sm:grid-cols-2">
                    {PRO_FEATURES.map(feature => (
                      <div key={feature} className="flex items-center gap-2 text-xs">
                        <CheckCircle2 size={13} className="shrink-0 text-accent" aria-hidden />
                        <span className="min-w-0 truncate text-foreground">
                          {t(`license.upgrade.features.${feature}.title`)}
                        </span>
                      </div>
                    ))}
                  </div>
                </section>
              </div>

              <div className="flex flex-col-reverse gap-2 border-t bg-muted/20 px-6 py-4 sm:flex-row sm:justify-end">
                <Button variant="ghost" size="sm" onClick={handleClose}>
                  {t('proActivation.continue')}
                </Button>
                <Button size="sm" className="gap-1.5" onClick={() => setView('tour')}>
                  {t('proActivation.viewTour')}
                  <ChevronRight size={14} aria-hidden="true" />
                </Button>
              </div>
            </>
          ) : (
            <>
              <div className="border-b bg-muted/20 px-6 py-5">
                <div className="mb-1 text-xs font-medium uppercase tracking-wide text-accent">
                  {progressLabel}
                </div>
                <DialogTitle className="text-lg font-semibold">
                  {t('proActivation.tour.title')}
                </DialogTitle>
                <DialogDescription className="mt-1">
                  {t('proActivation.tour.description')}
                </DialogDescription>
              </div>

              <div className="p-6">
                <div className="mb-5 flex gap-1.5" aria-hidden="true">
                  {ACTIVATION_ITEMS.map(item => (
                    <div
                      key={item.titleKey}
                      className={`h-1.5 flex-1 rounded-full ${item === activeItem ? 'bg-accent' : 'bg-muted'}`}
                    />
                  ))}
                </div>

                <div className="flex items-start gap-4">
                  <div className="flex h-11 w-11 shrink-0 items-center justify-center rounded-md border border-accent/20 bg-accent/10 text-accent">
                    <ActiveIcon size={22} aria-hidden="true" />
                  </div>
                  <div className="min-w-0">
                    <h3 className="text-base font-semibold text-foreground">
                      {t(activeItem.titleKey)}
                    </h3>
                    <p className="mt-2 text-sm leading-relaxed text-muted-foreground">
                      {t(activeItem.descriptionKey)}
                    </p>
                    <p className="mt-3 text-xs leading-relaxed text-muted-foreground">
                      {t(`proActivation.tour.steps.${step}.hint`)}
                    </p>
                  </div>
                </div>
              </div>

              <div className="flex flex-col-reverse gap-2 border-t bg-muted/20 px-6 py-4 sm:flex-row sm:items-center sm:justify-between">
                <Button variant="ghost" size="sm" onClick={() => setView('summary')}>
                  {t('proActivation.backToSummary')}
                </Button>
                <div className="flex justify-end gap-2">
                  <Button
                    variant="outline"
                    size="sm"
                    onClick={() => setStep(value => Math.max(0, value - 1))}
                    disabled={step === 0}
                    className="gap-1.5"
                  >
                    <ChevronLeft size={14} aria-hidden="true" />
                    {t('proActivation.previous')}
                  </Button>
                  {step < ACTIVATION_ITEMS.length - 1 ? (
                    <Button
                      size="sm"
                      onClick={() =>
                        setStep(value => Math.min(ACTIVATION_ITEMS.length - 1, value + 1))
                      }
                      className="gap-1.5"
                    >
                      {t('proActivation.next')}
                      <ChevronRight size={14} aria-hidden="true" />
                    </Button>
                  ) : (
                    <Button size="sm" onClick={handleClose}>
                      {t('proActivation.finish')}
                    </Button>
                  )}
                </div>
              </div>
            </>
          )}
        </DialogContent>
      </Dialog>
    </>
  );
}
