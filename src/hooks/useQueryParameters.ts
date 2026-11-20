// SPDX-License-Identifier: Apache-2.0

import { useState } from 'react';
import type { Driver } from '@/lib/connection/drivers';
import {
  areAllQueryParametersFilled,
  extractQueryParameters,
  type QueryParameterValue,
  substituteQueryParameters,
} from '@/lib/query/queryParameters';

export type QueryParameterAction = 'query' | 'explain';

interface PendingAction {
  mode: QueryParameterAction;
  sql: string;
}

interface UseQueryParametersOptions {
  query: string;
  driver?: Driver;
  enabled: boolean;
}

export function useQueryParameters({ query, driver, enabled }: UseQueryParametersOptions) {
  const [values, setValues] = useState<Record<string, QueryParameterValue>>({});
  const [dialogOpen, setDialogOpen] = useState(false);
  const [pending, setPending] = useState<PendingAction | null>(null);

  const detected = enabled ? extractQueryParameters(query, driver) : [];
  const dialogSql = pending?.sql ?? query;
  const dialogParameters = pending ? extractQueryParameters(pending.sql, driver) : detected;

  /** Returns the SQL to run, or `null` after opening the dialog for missing values. */
  const resolve = (sql: string, mode: QueryParameterAction): string | null => {
    if (!enabled) return sql;
    if (!areAllQueryParametersFilled(sql, values, driver)) {
      setPending({ mode, sql });
      setDialogOpen(true);
      return null;
    }
    return substituteQueryParameters(sql, values, driver);
  };

  const confirm = (): PendingAction | null => {
    const action = pending ?? { mode: 'query' as const, sql: query };
    setPending(null);
    setDialogOpen(false);
    if (!action.sql.trim() || !areAllQueryParametersFilled(action.sql, values, driver)) {
      return null;
    }
    return { mode: action.mode, sql: substituteQueryParameters(action.sql, values, driver) };
  };

  const openDialog = () => {
    setPending(null);
    setDialogOpen(true);
  };

  const setDialogVisibility = (open: boolean) => {
    setDialogOpen(open);
    if (!open) setPending(null);
  };

  const setValue = (name: string, next: QueryParameterValue) =>
    setValues(prev => ({ ...prev, [name]: next }));

  return {
    values,
    detected,
    dialogOpen,
    dialogSql,
    dialogParameters,
    resolve,
    confirm,
    openDialog,
    setDialogVisibility,
    setValue,
    reset: () => setValues({}),
  };
}
