// SPDX-License-Identifier: Apache-2.0

import { Keyboard, PanelLeft, Plus, RotateCcw, Search } from 'lucide-react';
import { useTranslation } from 'react-i18next';
import { Button } from '@/components/ui/button';
import { getShortcutSymbol } from '@/utils/platform';

interface RecoveryState {
  snapshot: { connectionId: string } | null;
  connectionName: string | null;
  isMissing: boolean;
  isLoading: boolean;
  error: string | null;
}

interface WelcomeScreenProps {
  hasConnections: boolean;
  recovery: RecoveryState;
  onNewConnection: () => void;
  onRestoreSession: () => void;
  onDiscardRecovery: () => void;
  onOpenSearch: () => void;
}

/**
 * Écran d'accueil avec deux états distincts :
 * - État A : Aucune connexion (first run)
 * - État B : Connexions existantes
 */
export function WelcomeScreen({
  hasConnections,
  recovery,
  onNewConnection,
  onRestoreSession,
  onDiscardRecovery,
  onOpenSearch,
}: WelcomeScreenProps) {
  const { t } = useTranslation();

  // État A : Aucune connexion configurée (first run)
  if (!hasConnections) {
    return (
      <div className="flex flex-col items-center justify-center h-full text-center px-4">
        <div className="p-4 rounded-full bg-accent/10 mb-6">
          <img src="/logo.png" alt="QoreDB" width={48} height={48} className="opacity-80" />
        </div>

        <h2 className="text-xl font-semibold text-foreground mb-2">
          {t('home.noConnections.title')}
        </h2>

        <p className="text-sm text-muted-foreground max-w-sm mb-6">
          {t('home.noConnections.description')}
        </p>

        <Button onClick={onNewConnection} size="lg">
          <Plus className="mr-2 h-4 w-4" />
          {t('home.noConnections.action')}
        </Button>
      </div>
    );
  }

  // État B : Connexions existantes
  return (
    <div className="flex flex-col items-center justify-center h-full px-4">
      <div className="w-full max-w-lg space-y-6">
        {/* Bloc principal : Reprise de session (si applicable) */}
        {recovery.snapshot && (
          <div className="rounded-xl border-2 border-accent/50 bg-accent/5 p-6 shadow-md">
            <div className="flex items-start gap-4">
              <div className="p-2.5 rounded-lg bg-accent/10 text-accent shrink-0">
                <RotateCcw size={20} />
              </div>
              <div className="flex-1 min-w-0">
                <h3 className="text-base font-semibold text-foreground mb-1">
                  {t('home.recovery.title')}
                </h3>
                <p className="text-sm text-muted-foreground">
                  {recovery.connectionName
                    ? t('home.recovery.description', { name: recovery.connectionName })
                    : t('home.recovery.descriptionUnknown')}
                </p>

                {recovery.isMissing && (
                  <p className="text-xs text-error mt-2">{t('home.recovery.missingConnection')}</p>
                )}

                {recovery.error && !recovery.isMissing && (
                  <p className="text-xs text-error mt-2">{recovery.error}</p>
                )}
              </div>
            </div>

            <div className="flex items-center justify-end gap-2 mt-4 pt-4 border-t border-border/50">
              <Button
                variant="ghost"
                size="sm"
                onClick={onDiscardRecovery}
                disabled={recovery.isLoading}
              >
                {t('home.recovery.discard')}
              </Button>
              <Button
                onClick={onRestoreSession}
                disabled={recovery.isLoading || recovery.isMissing}
              >
                {recovery.isLoading ? t('home.recovery.restoring') : t('home.recovery.restore')}
              </Button>
            </div>
          </div>
        )}

        {/* Contenu secondaire : Aucune session active */}
        <div className="text-center space-y-4">
          {!recovery.snapshot && (
            <>
              <div className="flex justify-center mb-2">
                <div className="p-3 rounded-full bg-muted/50">
                  <img src="/logo.png" alt="QoreDB" width={36} height={36} className="opacity-50" />
                </div>
              </div>

              <div>
                <h2 className="text-lg font-medium text-foreground">{t('home.noSession.title')}</h2>
                <p className="text-sm text-muted-foreground mt-1">
                  {t('home.noSession.description')}
                </p>
              </div>
            </>
          )}

          {/* Indication vers la sidebar */}
          <div className="flex items-center justify-center gap-2 py-3 px-4 rounded-lg bg-muted/30 border border-border/50 text-sm text-muted-foreground animate-pulse">
            <PanelLeft size={16} className="shrink-0" />
            <span>{t('home.noSession.sidebarHint')}</span>
          </div>

          {/* Séparateur */}
          <div className="flex items-center gap-3 py-2">
            <div className="flex-1 h-px bg-border/50" />
            <span className="text-xs text-muted-foreground/60 uppercase tracking-wide">
              {t('home.noSession.or')}
            </span>
            <div className="flex-1 h-px bg-border/50" />
          </div>

          {/* Actions secondaires */}
          <div className="flex flex-col sm:flex-row items-center justify-center gap-2">
            {/* Recherche / Actions rapides */}
            <Button
              variant="outline"
              onClick={onOpenSearch}
              className="w-full sm:w-auto text-muted-foreground"
            >
              <Search className="mr-2 h-4 w-4" />
              {t('home.noSession.quickActions')}
              <kbd className="ml-3 pointer-events-none inline-flex h-5 select-none items-center gap-1 rounded border bg-muted px-1.5 font-mono text-[10px] font-medium text-muted-foreground">
                <span className="text-xs">{getShortcutSymbol()}</span>K
              </kbd>
            </Button>

            {/* Nouvelle connexion */}
            <Button
              variant="ghost"
              size="sm"
              onClick={onNewConnection}
              className="text-muted-foreground"
            >
              <Plus className="mr-1.5 h-3.5 w-3.5" />
              {t('home.noSession.newConnection')}
            </Button>
          </div>

          {/* Raccourcis clavier */}
          {!recovery.snapshot && (
            <div className="pt-4 mt-2">
              <div className="inline-flex items-center gap-1.5 text-xs text-muted-foreground/50">
                <Keyboard size={12} />
                <span>{t('home.noSession.keyboardHint')}</span>
              </div>
            </div>
          )}
        </div>
      </div>
    </div>
  );
}
