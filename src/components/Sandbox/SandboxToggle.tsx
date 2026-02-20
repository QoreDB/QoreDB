// SPDX-License-Identifier: Apache-2.0

import { FlaskConical } from 'lucide-react';
import { useCallback, useEffect, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { toast } from 'sonner';
import { Button } from '@/components/ui/button';
import {
  Dialog,
  DialogContent,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog';
import { TooltipContent, TooltipRoot, TooltipTrigger } from '@/components/ui/tooltip';
import {
  activateSandbox,
  deactivateSandbox,
  getSandboxPreferences,
  hasPendingChanges,
  isSandboxActive,
  subscribeSandbox,
} from '@/lib/sandboxStore';
import type { Environment } from '@/lib/tauri';
import { cn } from '@/lib/utils';
import { getShortcut } from '@/utils/platform';

interface SandboxToggleProps {
  sessionId: string;
  onToggle?: (active: boolean) => void;
  className?: string;
  disabled?: boolean;
  environment?: Environment;
}

export function SandboxToggle({
  sessionId,
  onToggle,
  className,
  disabled = false,
  environment = 'development',
}: SandboxToggleProps) {
  const { t } = useTranslation();
  const [isActive, setIsActive] = useState(() => isSandboxActive(sessionId));
  const [showConfirm, setShowConfirm] = useState(false);

  // Subscribe to sandbox changes
  useEffect(() => {
    const unsubscribe = subscribeSandbox(changedSessionId => {
      if (changedSessionId === sessionId) {
        setIsActive(isSandboxActive(sessionId));
      }
    });
    return unsubscribe;
  }, [sessionId]);

  const handleToggle = useCallback(() => {
    if (isActive) {
      // Deactivating - check if there are pending changes
      const prefs = getSandboxPreferences();
      if (prefs.confirmOnDiscard && hasPendingChanges(sessionId)) {
        setShowConfirm(true);
        return;
      }
      deactivateSandbox(sessionId);
      onToggle?.(false);
    } else {
      activateSandbox(sessionId);
      onToggle?.(true);
      if (environment === 'staging') {
        toast.warning(t('sandbox.envWarningStaging'));
      }
      if (environment === 'production') {
        toast.warning(t('sandbox.envWarningProduction'));
      }
    }
  }, [isActive, sessionId, onToggle, environment, t]);

  // Keyboard shortcut: Ctrl+Shift+S
  useEffect(() => {
    const handleKeyDown = (e: KeyboardEvent) => {
      if ((e.ctrlKey || e.metaKey) && e.shiftKey && e.key.toLowerCase() === 's') {
        e.preventDefault();
        if (!disabled) {
          handleToggle();
        }
      }
    };

    window.addEventListener('keydown', handleKeyDown);
    return () => window.removeEventListener('keydown', handleKeyDown);
  }, [disabled, handleToggle]);

  const handleConfirmDeactivate = useCallback(
    (clearChanges: boolean) => {
      setShowConfirm(false);
      deactivateSandbox(sessionId, clearChanges);
      onToggle?.(false);
    },
    [sessionId, onToggle]
  );

  return (
    <>
      <TooltipRoot>
        <TooltipTrigger asChild>
          <Button
            variant={isActive ? 'default' : 'outline'}
            size="sm"
            className={cn(
              'h-8 gap-1.5 transition-all',
              isActive && 'bg-sandbox text-sandbox-foreground hover:bg-sandbox/90',
              className
            )}
            onClick={handleToggle}
            disabled={disabled}
          >
            <FlaskConical size={14} className={cn(isActive && 'animate-pulse')} />
            <span className="hidden sm:inline">
              {isActive ? t('sandbox.active') : t('sandbox.activate')}
            </span>
          </Button>
        </TooltipTrigger>
        <TooltipContent side="bottom">
          <p>{isActive ? t('sandbox.deactivateHint') : t('sandbox.activateHint')}</p>
          <p className="text-xs text-muted-foreground mt-1">{getShortcut('S', { shift: true })}</p>
        </TooltipContent>
      </TooltipRoot>

      <Dialog open={showConfirm} onOpenChange={setShowConfirm}>
        <DialogContent className="">
          <DialogHeader>
            <DialogTitle>{t('sandbox.confirmDeactivate.title')}</DialogTitle>
          </DialogHeader>
          <p className="text-sm text-muted-foreground">{t('sandbox.confirmDeactivate.message')}</p>
          <DialogFooter className="gap-2">
            <Button variant="outline" size="sm" onClick={() => setShowConfirm(false)}>
              {t('common.cancel')}
            </Button>
            <Button variant="outline" size="sm" onClick={() => handleConfirmDeactivate(false)}>
              {t('sandbox.confirmDeactivate.keepChanges')}
            </Button>
            <Button variant="destructive" size="sm" onClick={() => handleConfirmDeactivate(true)}>
              {t('sandbox.confirmDeactivate.discardChanges')}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </>
  );
}
