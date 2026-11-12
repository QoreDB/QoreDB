// SPDX-License-Identifier: Apache-2.0

import { useTranslation } from 'react-i18next';
import type { QueryResultEntry } from './QueryPanelResults';
import { describeQueryStatus } from './queryAnnouncement';

interface QueryStatusAnnouncerProps {
  loading: boolean;
  results: QueryResultEntry[];
  panelError: string | null;
}

export function QueryStatusAnnouncer({ loading, results, panelError }: QueryStatusAnnouncerProps) {
  const { t } = useTranslation();
  const announcement = describeQueryStatus(loading, results, panelError);
  return (
    <div className="sr-only" role="status" aria-live="polite" aria-atomic="true">
      {announcement ? t(announcement.key, announcement) : ''}
    </div>
  );
}
