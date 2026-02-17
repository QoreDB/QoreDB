// SPDX-License-Identifier: Apache-2.0

import { useMemo } from 'react';
import { useTranslation } from 'react-i18next';
import { QueryResult } from '../../lib/tauri';

interface ExplainPlanViewProps {
  result: QueryResult;
}

function normalizePlan(result: QueryResult): string {
  const value = result.rows[0]?.values?.[0];
  if (value === undefined || value === null) return '';
  if (typeof value === 'string') {
    try {
      const parsed = JSON.parse(value);
      return JSON.stringify(parsed, null, 2);
    } catch {
      return value;
    }
  }
  if (typeof value === 'object') {
    return JSON.stringify(value, null, 2);
  }
  return String(value);
}

export function ExplainPlanView({ result }: ExplainPlanViewProps) {
  const { t } = useTranslation();
  const planText = useMemo(() => normalizePlan(result), [result]);

  if (!planText) {
    return (
      <div className="flex items-center justify-center h-full text-muted-foreground text-sm">
        {t('query.explainNoPlan')}
      </div>
    );
  }

  return (
    <div className="flex-1 overflow-auto p-3">
      <pre className="text-xs font-mono whitespace-pre-wrap wrap-break-word">{planText}</pre>
    </div>
  );
}
