// SPDX-License-Identifier: BUSL-1.1

import type { TFunction } from 'i18next';
import { type ReactNode, useEffect } from 'react';
import { useTranslation } from 'react-i18next';
import { toast } from 'sonner';
import { pushInterceptorAlert, setRegressionCount } from '@/lib/stores/interceptorAlertStore';
import { setAuditLogOpen } from '@/lib/stores/modalStore';
import { getQueryTrends, type InterceptorAlert } from '@/lib/tauri/interceptor';
import { listen } from '@/lib/transport';
import { useLicense } from './LicenseProvider';

const REGRESSION_POLL_MS = 5 * 60 * 1000;
/** 24 h of recent executions plus the 7-day baseline. */
const REGRESSION_WINDOW_DAYS = 8;

/** Surfaces backend performance alerts (N+1, thresholds) as toasts and keeps
 *  the regression count fresh for the status bar. Pro only: the backend
 *  detects, this side decides whether the user sees it. */
export function InterceptorAlertsProvider({ children }: { children: ReactNode }) {
  const { t } = useTranslation();
  const { isFeatureEnabled } = useLicense();
  const enabled = isFeatureEnabled('profiling');

  useEffect(() => {
    if (!enabled) return;
    const unlisten = listen<InterceptorAlert>('interceptor-alert', event => {
      const alert = event.payload;
      pushInterceptorAlert(alert);
      toast.warning(describe(alert, t), {
        description: alert.kind === 'n_plus_one' ? alert.query_preview : undefined,
        action: {
          label: t('interceptor.alerts.open'),
          onClick: () => setAuditLogOpen(true, 'trends'),
        },
      });
    });
    return () => {
      unlisten.then(fn => fn()).catch(() => {});
    };
  }, [enabled, t]);

  useEffect(() => {
    if (!enabled) return;
    let active = true;
    const poll = () =>
      getQueryTrends({ days: REGRESSION_WINDOW_DAYS })
        .then(trends => {
          if (active) setRegressionCount(trends.filter(trend => trend.regression).length);
        })
        .catch(() => {});
    poll();
    const timer = setInterval(poll, REGRESSION_POLL_MS);
    return () => {
      active = false;
      clearInterval(timer);
    };
  }, [enabled]);

  return <>{children}</>;
}

function describe(alert: InterceptorAlert, t: TFunction): string {
  switch (alert.kind) {
    case 'n_plus_one':
      return t('interceptor.alerts.nPlusOne', { count: alert.count });
    case 'error_rate':
      return t('interceptor.alerts.errorRate', {
        percent: Math.round(alert.percent),
        total: alert.total,
      });
    case 'slow_queries':
      return t('interceptor.alerts.slowQueries', { count: alert.count });
  }
}
