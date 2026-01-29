import { Shield, Lock, Link2Off, Database, Server } from 'lucide-react';
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
  const driverMeta = connection ? getDriverMetadata(connection.driver) : null;

  return (
    <div className="flex items-center justify-between h-8 px-3 border-t border-border bg-muted/30 text-xs text-muted-foreground">
      {/* GAUCHE : Contexte actif (connexion, DB, driver) - zone dédiée */}
      <div className="flex items-center gap-3 min-w-0">
        {/* Indicateur de connexion */}
        <span className="flex items-center gap-1.5">
          {isConnected ? (
            <span className="w-2 h-2 rounded-full bg-success shadow-sm shadow-success/40" />
          ) : (
            <Link2Off size={12} className="text-muted-foreground" />
          )}
          <span className={isConnected ? 'text-foreground' : ''}>
            {isConnected ? t('status.connected') : t('status.disconnected')}
          </span>
        </span>

        {/* Contexte de connexion - hiérarchie claire */}
        {isConnected && connection && (
          <>
            <div className="h-4 w-px bg-border/50" />

            {/* Connexion (primaire) */}
            <span className="flex items-center gap-1.5 font-medium text-foreground truncate max-w-40">
              <Server size={11} className="text-muted-foreground shrink-0" />
              {connection.name}
            </span>

            {/* Database (secondaire) */}
            {connection.database && (
              <span className="flex items-center gap-1.5 truncate max-w-32">
                <Database size={11} className="text-muted-foreground shrink-0" />
                <span className="truncate">{connection.database}</span>
              </span>
            )}

            {/* Driver (tertiaire) */}
            {driverMeta && (
              <span className="text-muted-foreground/70 truncate">
                {driverMeta.label}
              </span>
            )}
          </>
        )}
      </div>

      {/* DROITE : États critiques (env, sandbox, read-only) */}
      <div className="flex items-center gap-2">
        {isConnected ? (
          <>
            {/* Sandbox Indicator */}
            <SandboxIndicator
              sessionId={sessionId}
              environment={environment}
            />

            {/* Badge environnement - visibilité accentuée pour PROD */}
            <span
              className={`flex items-center gap-1.5 px-2.5 py-1 text-[10px] font-bold uppercase tracking-wide rounded-full border ${
                environment === 'production'
                  ? 'animate-pulse shadow-sm'
                  : ''
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

            {/* Badge read-only si actif - très visible */}
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
