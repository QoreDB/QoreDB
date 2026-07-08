// SPDX-License-Identifier: Apache-2.0

import { useEffect, useRef, useState } from 'react';
import { useTranslation } from 'react-i18next';
import type { SearchResult } from '@/components/Search/GlobalSearch';
import { type SemanticSearchState, semanticSearch } from '@/lib/semantic';
import { useLicense } from '@/providers/LicenseProvider';
import { useSessionContext } from '@/providers/SessionProvider';

const DEBOUNCE_MS = 300;
const MIN_QUERY_LENGTH = 3;
const MAX_RESULTS = 5;

export const SEMANTIC_SETUP_ID = 'sem_setup';

const GUIDANCE_STATES: SemanticSearchState[] = [
  'ollama_missing',
  'model_missing',
  'index_empty',
  'building',
];

export function useSemanticSearch(query: string, active: boolean): SearchResult[] {
  const { t } = useTranslation();
  const { isFeatureEnabled } = useLicense();
  const { sessionId } = useSessionContext();
  const [results, setResults] = useState<SearchResult[]>([]);
  const requestCounter = useRef(0);

  const enabled = isFeatureEnabled('semantic_search');
  const trimmed = query.trim();

  useEffect(() => {
    if (!active || !enabled || !sessionId || trimmed.length < MIN_QUERY_LENGTH) {
      setResults([]);
      return;
    }

    const requestId = ++requestCounter.current;
    const timer = window.setTimeout(() => {
      semanticSearch(sessionId, trimmed, MAX_RESULTS)
        .then(response => {
          if (requestCounter.current !== requestId) return;
          if (response.status === 'ready') {
            setResults(
              response.results.slice(0, MAX_RESULTS).map(hit => ({
                type: 'schema' as const,
                id: `sem-${hit.object_id}`,
                label: hit.column ? `${hit.table}.${hit.column}` : hit.table,
                sublabel: hit.document,
                data: hit,
              }))
            );
          } else if (response.status === 'disabled') {
            setResults([]);
          } else if (GUIDANCE_STATES.includes(response.status)) {
            setResults([
              {
                type: 'schema' as const,
                id: SEMANTIC_SETUP_ID,
                label: t(`semantic.states.${response.status}`),
                sublabel: t('semantic.states.openSettings'),
              },
            ]);
          }
        })
        .catch(() => {
          if (requestCounter.current === requestId) setResults([]);
        });
    }, DEBOUNCE_MS);

    return () => window.clearTimeout(timer);
  }, [active, enabled, sessionId, trimmed, t]);

  useEffect(() => {
    if (!active) {
      requestCounter.current += 1;
      setResults([]);
    }
  }, [active]);

  return results;
}
