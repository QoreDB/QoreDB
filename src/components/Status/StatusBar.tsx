// SPDX-License-Identifier: Apache-2.0

import { Database, GitBranch, Link2Off, Lock, RefreshCw, Server, Shield, WifiOff } from 'lucide-react';
import { useTranslation } from 'react-i18next';
import { SandboxIndicator } from '@/components/Sandbox';
import { getDriverMetadata } from '@/lib/drivers';
import { ENVIRONMENT_CONFIG } from '@/lib/environment';
import type { ConnectionHealth, SavedConnection } from '@/lib/tauri';
import { useTransactionStore } from '@/lib/transactionStore';

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
    <div className="flex items-center justify-between h-8 px-3 border-t border-border bg-muted/30 text-xs text-muted-foreground">
      <div className="flex items-center gap-3 min-w-0">
        <span className="flex items-center gap-1.5">
          {healthIndicator()}
          <span className={isConnected && connectionHealth === 'healthy' ? 'text-foreground' : connectionHealth !== 'healthy' ? 'text-warning' : ''}>
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
                    ({t('status.transactionStatements', { count: transactionState.statementCount })})
                  </span>
                )}
              </span>
            )}

            <SandboxIndicator sessionId={sessionId} environment={environment} />

            <span
              className={`flex items-center gap-1.5 px-2.5 py-1 text-[10px] font-bold uppercase tracking-wide rounded-full border ${
                environment === 'production' ? 'animate-pulse shadow-sm' : ''
              }`}
              style={{
                backgroundColor: environment === 'production' ? envConfig.color : envConfig.bgSoft,
                color: environment === 'production' ? '#ffffff' : envConfig.color,
                borderColor: envConfig.color,
              }}
            >
              <Shield size={11} />
              {envConfig.labelShort}
            </span>

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
      </div>
    </div>
  );
}
