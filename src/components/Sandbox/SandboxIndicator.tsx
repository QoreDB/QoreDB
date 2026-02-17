// SPDX-License-Identifier: Apache-2.0

import { useState, useEffect } from 'react';
import { useTranslation } from 'react-i18next';
import { FlaskConical } from 'lucide-react';
import { isSandboxActive, getChangesCount, subscribeSandbox } from '@/lib/sandboxStore';
import { Environment } from '@/lib/tauri';
import { cn } from '@/lib/utils';

interface SandboxIndicatorProps {
  sessionId: string | null;
  environment?: Environment;
  onClick?: () => void;
}

const ENVIRONMENT_COLORS: Record<Environment, { bg: string; border: string; text: string }> = {
  development: {
    bg: 'bg-blue-500/10',
    border: 'border-blue-500/30',
    text: 'text-blue-500',
  },
  staging: {
    bg: 'bg-orange-500/10',
    border: 'border-orange-500/30',
    text: 'text-orange-500',
  },
  production: {
    bg: 'bg-red-500/10',
    border: 'border-red-500/30',
    text: 'text-red-500',
  },
};

export function SandboxIndicator({
  sessionId,
  environment = 'development',
  onClick,
}: SandboxIndicatorProps) {
  const { t } = useTranslation();
  const [isActive, setIsActive] = useState(false);
  const [changesCount, setChangesCount] = useState(0);

  useEffect(() => {
    if (!sessionId) {
      setIsActive(false);
      setChangesCount(0);
      return;
    }

    setIsActive(isSandboxActive(sessionId));
    setChangesCount(getChangesCount(sessionId));

    const unsubscribe = subscribeSandbox(changedSessionId => {
      if (changedSessionId === sessionId) {
        setIsActive(isSandboxActive(sessionId));
        setChangesCount(getChangesCount(sessionId));
      }
    });

    return unsubscribe;
  }, [sessionId]);

  if (!isActive || !sessionId) {
    return null;
  }

  const colors = ENVIRONMENT_COLORS[environment];

  return (
    <button
      type="button"
      onClick={onClick}
      className={cn(
        'flex items-center gap-1.5 px-2 py-0.5 text-[10px] font-bold rounded-full border transition-colors',
        colors.bg,
        colors.border,
        colors.text,
        onClick && 'hover:opacity-80 cursor-pointer'
      )}
      title={t('sandbox.indicator.tooltip', { count: changesCount })}
    >
      <FlaskConical size={10} className="animate-pulse" />
      <span>{t('sandbox.indicator.label')}</span>
      {changesCount > 0 && (
        <span className="px-1 py-0.5 rounded-full bg-current/20 text-[9px] min-w-[16px] text-center">
          {changesCount > 99 ? '99+' : changesCount}
        </span>
      )}
    </button>
  );
}

/**
 * A border indicator wrapper that adds a colored border when sandbox is active
 */
interface SandboxBorderProps {
  sessionId: string | null;
  environment?: Environment;
  children: React.ReactNode;
  className?: string;
}

export function SandboxBorder({
  sessionId,
  environment = 'development',
  children,
  className,
}: SandboxBorderProps) {
  const [isActive, setIsActive] = useState(false);

  useEffect(() => {
    if (!sessionId) {
      setIsActive(false);
      return;
    }

    setIsActive(isSandboxActive(sessionId));

    const unsubscribe = subscribeSandbox(changedSessionId => {
      if (changedSessionId === sessionId) {
        setIsActive(isSandboxActive(sessionId));
      }
    });

    return unsubscribe;
  }, [sessionId]);

  const colors = ENVIRONMENT_COLORS[environment];

  return (
    <div
      className={cn(
        'transition-all duration-300',
        isActive && `ring-2 ring-inset ${colors.border.replace('border-', 'ring-')}`,
        className
      )}
    >
      {children}
    </div>
  );
}
