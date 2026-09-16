// SPDX-License-Identifier: Apache-2.0

import { useTranslation } from 'react-i18next';
import type { AuditLogEntry } from '../../lib/tauri/interceptor';

const STYLES = {
  blocked: 'bg-warning/15 text-warning',
  success: 'bg-success/15 text-success',
  failed: 'bg-error/15 text-error',
} as const;

export function auditStatus(entry: AuditLogEntry): keyof typeof STYLES {
  return entry.blocked ? 'blocked' : entry.success ? 'success' : 'failed';
}

export function AuditStatusChip({ entry }: { entry: AuditLogEntry }) {
  const { t } = useTranslation();
  const status = auditStatus(entry);
  return (
    <span
      className={`inline-flex items-center rounded-sm px-1.5 py-px text-[10px] font-semibold tracking-wide ${STYLES[status]}`}
    >
      {t(`interceptor.audit.status.${status}`)}
    </span>
  );
}
