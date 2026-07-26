// SPDX-License-Identifier: Apache-2.0

import { useEffect, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { toast } from 'sonner';
import { isOnboardingCompleted } from '@/lib/onboardingState';
import { acknowledgeCrashReports, type CrashReport, getPendingCrashReports } from '@/lib/tauri';
import { CrashReportDialog } from './CrashReportDialog';

export function CrashReportOverlay() {
  const { t } = useTranslation();
  const [reports, setReports] = useState<CrashReport[]>([]);
  const [open, setOpen] = useState(false);

  useEffect(() => {
    if (!isOnboardingCompleted()) return;

    let cancelled = false;
    getPendingCrashReports()
      .then(pending => {
        if (cancelled || pending.length === 0) return;
        setReports(pending);
        toast.error(t('crashReport.toastTitle'), {
          description: t('crashReport.toastDescription'),
          duration: Number.POSITIVE_INFINITY,
          // Dismissing without opening still counts as acknowledged; leaving it
          // pending would nag on every launch.
          onDismiss: () => acknowledge(),
          action: {
            label: t('crashReport.toastAction'),
            onClick: () => setOpen(true),
          },
        });
      })
      .catch(() => {});

    return () => {
      cancelled = true;
    };
  }, [t]);

  function acknowledge() {
    acknowledgeCrashReports().catch(() => {});
  }

  const handleDismiss = () => {
    setOpen(false);
    acknowledge();
  };

  if (reports.length === 0) return null;

  return (
    <CrashReportDialog
      open={open}
      report={reports[0]}
      otherCount={reports.length - 1}
      onDismiss={handleDismiss}
    />
  );
}
