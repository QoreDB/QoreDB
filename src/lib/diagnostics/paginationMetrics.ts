// SPDX-License-Identifier: Apache-2.0

// Session-scoped and in-memory only. Deliberately holds no table name, cell
// value or cursor, so a scope can be pasted into a bug report as-is.

const MAX_SAMPLES = 200;
const MAX_SCOPES = 32;

export interface PaginationScope {
  id: string;
  /** Opaque, ordinal label. Never the table name. */
  label: string;
  openedAt: number;
  pages: number;
  rows: number;
  firstPageMs: number | null;
  firstSearchMs: number | null;
  pageMs: number[];
  searchMs: number[];
  exactCounts: number;
  exactCountsCancelled: number;
  errors: number;
}

let scopes: PaginationScope[] = [];
let nextOrdinal = 1;
const listeners = new Set<() => void>();

function emit(): void {
  for (const listener of listeners) listener();
}

export function subscribePaginationMetrics(listener: () => void): () => void {
  listeners.add(listener);
  return () => {
    listeners.delete(listener);
  };
}

export function getPaginationScopes(): PaginationScope[] {
  return scopes;
}

export function openPaginationScope(): string {
  const ordinal = nextOrdinal++;
  const scope: PaginationScope = {
    id: `${ordinal}-${Math.random().toString(36).slice(2, 8)}`,
    label: `#${ordinal}`,
    openedAt: Date.now(),
    pages: 0,
    rows: 0,
    firstPageMs: null,
    firstSearchMs: null,
    pageMs: [],
    searchMs: [],
    exactCounts: 0,
    exactCountsCancelled: 0,
    errors: 0,
  };
  scopes = [...scopes, scope].slice(-MAX_SCOPES);
  emit();
  return scope.id;
}

function update(id: string, mutate: (scope: PaginationScope) => PaginationScope): void {
  const index = scopes.findIndex(scope => scope.id === id);
  if (index === -1) return;
  const next = scopes.slice();
  next[index] = mutate(scopes[index]);
  scopes = next;
  emit();
}

export function recordPaginationPage(
  id: string,
  durationMs: number,
  rows: number,
  searchActive: boolean
): void {
  update(id, scope => ({
    ...scope,
    pages: scope.pages + 1,
    rows: scope.rows + rows,
    firstPageMs: scope.firstPageMs ?? durationMs,
    firstSearchMs: searchActive ? (scope.firstSearchMs ?? durationMs) : scope.firstSearchMs,
    pageMs: [...scope.pageMs, durationMs].slice(-MAX_SAMPLES),
    searchMs: searchActive ? [...scope.searchMs, durationMs].slice(-MAX_SAMPLES) : scope.searchMs,
  }));
}

export function recordPaginationExactCount(id: string, cancelled: boolean): void {
  update(id, scope => ({
    ...scope,
    exactCounts: scope.exactCounts + 1,
    exactCountsCancelled: scope.exactCountsCancelled + (cancelled ? 1 : 0),
  }));
}

export function recordPaginationError(id: string): void {
  update(id, scope => ({ ...scope, errors: scope.errors + 1 }));
}

export function resetPaginationMetrics(): void {
  scopes = [];
  nextOrdinal = 1;
  emit();
}

export function percentile(values: number[], p: number): number | null {
  if (values.length === 0) return null;
  const sorted = [...values].sort((a, b) => a - b);
  const rank = Math.ceil((p / 100) * sorted.length) - 1;
  return sorted[Math.min(Math.max(rank, 0), sorted.length - 1)];
}

export function paginationReport(): string {
  return JSON.stringify(
    {
      generatedAt: new Date().toISOString(),
      scopes: scopes.map(scope => ({
        label: scope.label,
        pages: scope.pages,
        rows: scope.rows,
        firstPageMs: scope.firstPageMs,
        firstSearchMs: scope.firstSearchMs,
        pageP50Ms: percentile(scope.pageMs, 50),
        pageP95Ms: percentile(scope.pageMs, 95),
        searchP95Ms: percentile(scope.searchMs, 95),
        exactCounts: scope.exactCounts,
        exactCountsCancelled: scope.exactCountsCancelled,
        errors: scope.errors,
      })),
    },
    null,
    2
  );
}
