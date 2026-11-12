// SPDX-License-Identifier: Apache-2.0

import { useEffect, useState } from 'react';
import { Driver } from '@/lib/connection/drivers';
import { type Hypertable, listHypertables, type Namespace } from '@/lib/tauri';

const EMPTY: Hypertable[] = [];

export function useHypertables(
  sessionId: string | undefined,
  namespace: Namespace | null,
  driver: string,
  refreshKey?: unknown
): Hypertable[] {
  const [hypertables, setHypertables] = useState<Hypertable[]>(EMPTY);
  const enabled = driver === Driver.Timescaledb && Boolean(sessionId) && namespace !== null;
  const database = namespace?.database;
  const schema = namespace?.schema;

  // biome-ignore lint/correctness/useExhaustiveDependencies: refreshKey reloads when the caller's listing changes
  useEffect(() => {
    if (!enabled || !sessionId || database === undefined) {
      setHypertables(EMPTY);
      return;
    }
    let cancelled = false;
    listHypertables(sessionId, { database, schema })
      .then(response => {
        if (!cancelled) setHypertables(response.success ? (response.data ?? EMPTY) : EMPTY);
      })
      .catch(() => {
        if (!cancelled) setHypertables(EMPTY);
      });
    return () => {
      cancelled = true;
    };
  }, [enabled, sessionId, database, schema, refreshKey]);

  return hypertables;
}
