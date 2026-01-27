import { Shield, Lock, Link2Off, Link2 } from 'lucide-react';
import { useTranslation } from 'react-i18next';

import { SavedConnection } from '@/lib/tauri';
import { ENVIRONMENT_CONFIG } from '@/lib/environment';
import { getDriverMetadata } from '@/lib/drivers';
import { SandboxIndicator } from '@/components/Sandbox';

interface StatusBarProps {
  sessionId: string | null;
  connection: SavedConnection | null;
}

export function StatusBar({ sessionId, connection }: StatusBarProps) {
  const { t } = useTranslation();
  const isConnected = Boolean(sessionId && connection);

  const environment = connection?.environment || 'development';
  const envConfig = ENVIRONMENT_CONFIG[environment];
  const driverLabel = connection ? getDriverMetadata(connection.driver).label : '';
  const sessionShort = sessionId ? sessionId.slice(0, 8) : '';

  return (
    <div className="flex items-center justify-between h-8 px-3 border-t border-border bg-muted/30 text-xs text-muted-foreground">
      <div className="flex items-center gap-2 min-w-0">
        <span className="flex items-center gap-2">
          {isConnected ? (
            <>
              <span className="w-2 h-2 rounded-full bg-success shadow-sm shadow-success/40" />
              <span className="text-foreground">{t('status.connected')}</span>
            </>
          ) : (
            <>
              <Link2Off size={12} />
              <span>{t('status.disconnected')}</span>
            </>
          )}
        </span>

        {isConnected && connection && (
          <>
            <span className="text-border/60">•</span>
            <span className="font-medium text-foreground truncate">{connection.name}</span>
            <span className="text-border/60">•</span>
            <span className="truncate">{driverLabel}</span>
            {connection.database && (
              <>
                <span className="text-border/60">•</span>
                <span className="truncate">{connection.database}</span>
              </>
            )}
          </>
        )}
      </div>

      <div className="flex items-center gap-2">
        {isConnected ? (
          <>
            {/* Sandbox Indicator */}
            <SandboxIndicator
              sessionId={sessionId}
              environment={environment}
            />

            <span
              className="flex items-center gap-1.5 px-2 py-0.5 text-[10px] font-bold rounded-full border"
              style={{
                backgroundColor: envConfig.bgSoft,
                color: envConfig.color,
                borderColor: envConfig.color,
              }}
            >
              <Shield size={10} />
              {envConfig.labelShort}
            </span>
            {connection?.read_only && (
              <span className="flex items-center gap-1.5 px-2 py-0.5 text-[10px] font-bold rounded-full border border-warning/30 bg-warning/10 text-warning">
                <Lock size={10} />
                {t('environment.readOnly')}
              </span>
            )}
            {sessionShort && (
              <span className="flex items-center gap-1.5 font-mono text-[10px]">
                <Link2 size={10} />
                {t('status.session')} {sessionShort}
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
