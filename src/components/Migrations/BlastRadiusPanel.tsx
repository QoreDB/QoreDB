// SPDX-License-Identifier: Apache-2.0

import {
  Activity,
  AlertTriangle,
  CheckCircle2,
  FileText,
  Loader2,
  ShieldAlert,
} from 'lucide-react';
import { useCallback, useEffect, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { Badge } from '@/components/ui/badge';
import { Button } from '@/components/ui/button';
import { analyzeMigrationBlastRadius, type MigrationBlastRadiusReport } from '@/lib/tauri';
import { cn } from '@/lib/utils';

const PREFLIGHT_CAPABLE_KINDS = new Set([
  'add_not_null',
  'add_not_null_column',
  'add_unique',
  'add_foreign_key',
  'add_check',
  'shrink_varchar',
]);
const RENAME_KINDS = new Set(['rename_table', 'rename_column']);

interface BlastRadiusPanelProps {
  sql: string;
  sessionId?: string;
  database?: string;
  driver?: string;
}

export function BlastRadiusPanel({ sql, sessionId, database, driver }: BlastRadiusPanelProps) {
  const { t } = useTranslation();
  const [analysis, setAnalysis] = useState<{
    sql: string;
    report: MigrationBlastRadiusReport;
  } | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [analyzing, setAnalyzing] = useState(false);
  const [runningPreflight, setRunningPreflight] = useState(false);

  const trimmedSql = sql.trim();

  useEffect(() => {
    setError(null);
    if (!trimmedSql) {
      setAnalysis(null);
      return;
    }

    let cancelled = false;
    const timer = window.setTimeout(() => {
      setAnalyzing(true);
      void analyzeMigrationBlastRadius({
        sessionId,
        database,
        driver,
        sql: trimmedSql,
        runPreflight: false,
      })
        .then(res => {
          if (!cancelled) setAnalysis({ sql: trimmedSql, report: res });
        })
        .catch(err => {
          if (!cancelled) setError(err instanceof Error ? err.message : String(err));
        })
        .finally(() => {
          if (!cancelled) setAnalyzing(false);
        });
    }, 350);

    return () => {
      cancelled = true;
      window.clearTimeout(timer);
    };
  }, [trimmedSql, sessionId, database, driver]);

  const handleRunPreflight = useCallback(async () => {
    if (!trimmedSql || !sessionId) return;
    setRunningPreflight(true);
    try {
      const res = await analyzeMigrationBlastRadius({
        sessionId,
        database,
        driver,
        sql: trimmedSql,
        runPreflight: true,
      });
      setAnalysis({ sql: trimmedSql, report: res });
      setError(null);
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setRunningPreflight(false);
    }
  }, [trimmedSql, sessionId, database, driver]);

  if (!trimmedSql) {
    return null;
  }

  const report = analysis?.report ?? null;
  const isCurrent = analysis?.sql === trimmedSql;
  const hazards = report?.hazards ?? [];
  const auditMatches = report?.audit_matches ?? [];
  const workspaceAssets = report?.workspace_assets ?? [];
  const preflightChecks = report?.preflight_checks ?? [];
  const hasFindings = hazards.length > 0 || auditMatches.length > 0 || workspaceAssets.length > 0;
  const hasPreflightCandidate = hazards.some(h => PREFLIGHT_CAPABLE_KINDS.has(h.kind));
  const parseError = report?.parse_error ?? null;

  if (error && !report) {
    return (
      <div
        role="alert"
        className="flex items-start gap-2 rounded-lg border border-destructive/30 bg-destructive/10 px-3 py-2 text-xs text-destructive"
      >
        <AlertTriangle className="mt-0.5 h-3.5 w-3.5 shrink-0" />
        <span>{t('migrations.blastRadius.analysisFailed', { error })}</span>
      </div>
    );
  }

  if (!report) {
    return (
      <div className="flex items-center gap-2 rounded-lg border border-border/60 bg-muted/20 px-3 py-2 text-xs text-muted-foreground">
        <Loader2 className="h-3.5 w-3.5 shrink-0 animate-spin" />
        <span>{t('migrations.blastRadius.analyzing')}</span>
      </div>
    );
  }

  if (isCurrent && !analyzing && !hasFindings && !parseError && !error) {
    return (
      <div className="flex items-center gap-2 rounded-lg border border-border/60 bg-muted/20 px-3 py-2 text-xs text-muted-foreground">
        <CheckCircle2 className="h-3.5 w-3.5 shrink-0 text-success" />
        <span>{t('migrations.blastRadius.none')}</span>
      </div>
    );
  }

  return (
    <div className="rounded-lg border border-border bg-muted/20 p-3 text-xs space-y-2.5">
      <div className="flex flex-wrap items-center justify-between gap-2">
        <div className="flex items-center gap-2 font-medium text-foreground">
          <ShieldAlert className="h-3.5 w-3.5 text-warning" />
          <span>{t('migrations.blastRadius.title')}</span>
          {(analyzing || (!isCurrent && !error)) && (
            <Loader2 className="h-3 w-3 animate-spin text-muted-foreground" />
          )}
        </div>
        {hasPreflightCandidate && (
          <Button
            type="button"
            size="sm"
            variant="outline"
            className="h-6 px-2 text-[11px]"
            disabled={!sessionId || runningPreflight}
            onClick={() => void handleRunPreflight()}
            title={!sessionId ? t('migrations.connectHint') : undefined}
          >
            {runningPreflight && <Loader2 className="mr-1 h-3 w-3 animate-spin" />}
            {t('migrations.blastRadius.runPreflight')}
          </Button>
        )}
      </div>

      {parseError && (
        <div
          role="alert"
          className="flex items-start gap-2 rounded border border-warning/30 bg-warning/10 px-2 py-1.5 text-warning"
        >
          <AlertTriangle className="mt-0.5 h-3 w-3 shrink-0" />
          <div className="min-w-0">
            <div className="font-medium">{t('migrations.blastRadius.partialAnalysis')}</div>
            <div className="break-words font-mono text-[11px] opacity-80">{parseError}</div>
          </div>
        </div>
      )}

      {error && (
        <div role="alert" className="text-destructive">
          {t('migrations.blastRadius.analysisFailed', { error })}
        </div>
      )}

      {hazards.length > 0 && (
        <div className="space-y-1">
          <div className="text-[11px] font-medium text-muted-foreground">
            {t('migrations.blastRadius.hazardsSection')}
          </div>
          <div className="flex flex-wrap gap-1.5">
            {hazards.map(hazard => (
              <div
                key={`${hazard.kind}-${hazard.table}-${hazard.columns.join(',')}-${hazard.detail ?? ''}`}
                className="flex items-center gap-1.5 rounded border border-border/80 bg-background px-2 py-1"
              >
                <Badge
                  variant={hazard.severity === 'high' ? 'destructive' : 'outline'}
                  className={cn(
                    'text-[10px] uppercase',
                    hazard.severity !== 'high' && 'border-warning/40 text-warning'
                  )}
                >
                  {t(`migrations.blastRadius.severity.${hazard.severity}`)}
                </Badge>
                <AlertTriangle className="h-3 w-3 shrink-0 text-warning" />
                <span className="font-medium">
                  {t(`migrations.blastRadius.kinds.${hazard.kind}`)}
                </span>
                <span className="font-mono text-muted-foreground">
                  {hazard.table}
                  {hazard.columns.length > 0 ? `.${hazard.columns.join(', ')}` : ''}
                </span>
                {hazard.detail && (
                  <span className="font-mono text-[11px] text-muted-foreground">
                    {RENAME_KINDS.has(hazard.kind) ? `→ ${hazard.detail}` : `(${hazard.detail})`}
                  </span>
                )}
              </div>
            ))}
          </div>
        </div>
      )}

      {auditMatches.length > 0 && (
        <div className="space-y-1">
          <div className="flex items-center gap-1.5 text-[11px] font-medium text-muted-foreground">
            <Activity className="h-3 w-3" />
            <span>{t('migrations.blastRadius.auditSection')}</span>
          </div>
          <div className="flex flex-wrap gap-1.5">
            {auditMatches.map(match => (
              <div
                key={match.table}
                className="rounded border border-border/70 bg-background px-2 py-1 font-mono text-[11px]"
              >
                {match.table}:{' '}
                {t('migrations.blastRadius.auditStats', {
                  count: match.execution_count,
                  fingerprints: match.unique_fingerprints,
                })}
              </div>
            ))}
          </div>
        </div>
      )}

      {workspaceAssets.length > 0 && (
        <div className="space-y-1">
          <div className="flex items-center gap-1.5 text-[11px] font-medium text-muted-foreground">
            <FileText className="h-3 w-3" />
            <span>{t('migrations.blastRadius.assetsSection')}</span>
          </div>
          <div className="flex flex-wrap gap-1.5">
            {workspaceAssets.map(asset => (
              <div
                key={`${asset.kind}-${asset.path}-${asset.name}-${asset.matched_tables.join(',')}`}
                className="flex items-center gap-1 rounded border border-border/70 bg-background px-2 py-1"
              >
                <Badge variant="secondary" className="text-[10px]">
                  {t(`migrations.blastRadius.assetKinds.${asset.kind}`)}
                </Badge>
                <span>{asset.name}</span>
                <span className="font-mono text-[11px] text-muted-foreground">
                  ({asset.matched_tables.join(', ')})
                </span>
              </div>
            ))}
          </div>
        </div>
      )}

      {preflightChecks.length > 0 && (
        <div className="space-y-1 border-t border-border/60 pt-2">
          <div className="text-[11px] font-medium text-muted-foreground">
            {t('migrations.blastRadius.preflightSection')}
          </div>
          <div className="space-y-1">
            {preflightChecks.map(check => (
              <div
                key={`${check.kind}-${check.table}-${check.columns.join(',')}`}
                className="flex items-center justify-between gap-2 rounded border border-border/70 bg-background px-2 py-1"
              >
                <span className="font-mono">
                  {check.table}
                  {check.columns.length > 0 ? `.${check.columns.join(', ')}` : ''} (
                  {t(`migrations.blastRadius.kinds.${check.kind}`)})
                </span>
                {check.error ? (
                  <span className="text-warning">{check.error}</span>
                ) : (
                  <span
                    className={cn(
                      'font-medium',
                      (check.violations ?? 0) > 0 ? 'text-destructive' : 'text-success'
                    )}
                  >
                    {t('migrations.blastRadius.violationsCount', {
                      count: check.violations ?? 0,
                    })}
                  </span>
                )}
              </div>
            ))}
          </div>
        </div>
      )}
    </div>
  );
}
