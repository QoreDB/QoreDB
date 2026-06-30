// SPDX-License-Identifier: Apache-2.0

import {
  Bug,
  Database,
  FileText,
  GitBranch,
  Link2Off,
  Lock,
  RefreshCw,
  Server,
  WifiOff,
} from 'lucide-react';
import { useTranslation } from 'react-i18next';
import { AnalyticsService } from '@/components/Onboarding/AnalyticsService';
import { SandboxIndicator } from '@/components/Sandbox';
import { Tooltip } from '@/components/ui/tooltip';
import { getDriverMetadata } from '@/lib/connection/drivers';
import { ENVIRONMENT_CONFIG } from '@/lib/environment';
import { setAuditLogOpen, setLogsOpen } from '@/lib/stores/modalStore';
import { useTransactionStore } from '@/lib/stores/transactionStore';
import type { ConnectionHealth, SavedConnection } from '@/lib/tauri';
import { APP_VERSION } from '@/lib/version';

interface StatusBarProps {
  sessionId: string | null;
  connection: SavedConnection | null;
  connectionHealth?: ConnectionHealth;
}

export function StatusBar({ sessionId, connection, connectionHealth = 'healthy' }: StatusBarProps) {
  const { t } = useTranslation();
  const isConnected = Boolean(sessionId && connection);
  const transactionState = useTransactionStore();

  const environment = connection?.environment || 'development';
  const envConfig = ENVIRONMENT_CONFIG[environment];
  const driverMeta = connection ? getDriverMetadata(connection.driver) : null;

  const healthIndicator = () => {
    if (!isConnected) {
      return <Link2Off size={12} className="text-muted-foreground" />;
    }
    if (connectionHealth === 'reconnecting') {
      return <RefreshCw size={12} className="text-warning animate-spin" />;
    }
    if (connectionHealth === 'unhealthy') {
      return <WifiOff size={12} className="text-destructive" />;
    }
    return <span className="w-2 h-2 rounded-full bg-success shadow-sm shadow-success/40" />;
  };

  const healthLabel = () => {
    if (!isConnected) return t('status.disconnected');
    if (connectionHealth === 'reconnecting') return t('status.reconnecting');
    if (connectionHealth === 'unhealthy') return t('status.connectionLost');
    return t('status.connected');
  };

  return (
    <output
      aria-live="polite"
      aria-label={t('a11y.statusBar')}
      className="flex items-center justify-between h-8 px-3 border-t border-border bg-muted/30 text-xs text-muted-foreground"
    >
      <div className="flex items-center gap-3 min-w-0">
        <span className="flex items-center gap-1.5">
          {healthIndicator()}
          <span
            className={
              isConnected && connectionHealth === 'healthy'
                ? 'text-foreground'
                : connectionHealth !== 'healthy'
                  ? 'text-warning'
                  : ''
            }
          >
            {healthLabel()}
          </span>
        </span>

        {isConnected && connection && (
          <>
            <div className="h-4 w-px bg-border/50" />

            <span className="flex items-center gap-1.5 font-medium text-foreground truncate max-w-40">
              <Server size={11} className="text-muted-foreground shrink-0" />
              {connection.name}
            </span>

            {connection.database && (
              <span className="flex items-center gap-1.5 truncate max-w-32">
                <Database size={11} className="text-muted-foreground shrink-0" />
                <span className="truncate">{connection.database}</span>
              </span>
            )}

            {driverMeta && (
              <span className="text-muted-foreground/70 truncate">{driverMeta.label}</span>
            )}
          </>
        )}
      </div>

      <div className="flex items-center gap-2">
        {isConnected ? (
          <>
            {transactionState.active && (
              <span className="flex items-center gap-1.5 px-2.5 py-1 text-[10px] font-bold uppercase tracking-wide rounded-full border border-accent/30 bg-accent/10 text-accent animate-pulse">
                <GitBranch size={11} />
                {t('status.transactionActive')}
                {transactionState.statementCount > 0 && (
                  <span className="font-normal">
                    ({t('status.transactionStatements', { count: transactionState.statementCount })}
                    )
                  </span>
                )}
              </span>
            )}

            <SandboxIndicator sessionId={sessionId} environment={environment} />

            {environment !== 'development' && (
              <span
                className="px-1.5 py-0.5 text-[10px] font-bold rounded"
                style={{
                  backgroundColor: envConfig.bgSoft,
                  color: envConfig.color,
                }}
              >
                {envConfig.labelShort}
              </span>
            )}

            {connection?.read_only && (
              <span className="flex items-center gap-1.5 px-2.5 py-1 text-[10px] font-bold uppercase tracking-wide rounded-full border-2 border-warning bg-warning/20 text-warning">
                <Lock size={11} />
                {t('environment.readOnly')}
              </span>
            )}
          </>
        ) : (
          <span className="text-muted-foreground">{t('status.noSession')}</span>
        )}
        <div className="h-4 w-px bg-border/50" />
        <Tooltip content={t('sidebar.errorLogs')}>
          <button
            type="button"
            aria-label={t('sidebar.errorLogs')}
            className="flex items-center justify-center h-5 w-5 rounded text-muted-foreground/70 hover:text-foreground hover:bg-muted/50 transition-colors"
            onClick={() => {
              AnalyticsService.capture('error_view_opened', { source: 'statusbar' });
              setLogsOpen(true);
            }}
          >
            <Bug size={12} />
          </button>
        </Tooltip>
        <Tooltip content={t('sidebar.auditLog')}>
          <button
            type="button"
            aria-label={t('sidebar.auditLog')}
            className="flex items-center justify-center h-5 w-5 rounded text-muted-foreground/70 hover:text-foreground hover:bg-muted/50 transition-colors"
            onClick={() => setAuditLogOpen(true)}
          >
            <FileText size={12} />
          </button>
        </Tooltip>
        <div className="h-4 w-px bg-border/50" />
        <span className="text-muted-foreground/60">v{APP_VERSION}</span>
      </div>
    </output>
  );
}
