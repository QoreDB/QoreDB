// SPDX-License-Identifier: Apache-2.0

import { useTranslation } from 'react-i18next';
import { X, AlertTriangle, AlertCircle, CheckCircle2, Info, ExternalLink } from 'lucide-react';
import { cn } from '@/lib/utils';
import {
  Notification,
  NotificationLevel,
  dismissNotification,
  markAsRead,
} from '@/lib/notificationStore';
import { emitUiEvent } from '@/lib/uiEvents';

interface NotificationItemProps {
  notification: Notification;
}

const levelIcons: Record<NotificationLevel, React.ComponentType<{ className?: string }>> = {
  info: Info,
  warning: AlertTriangle,
  error: AlertCircle,
  success: CheckCircle2,
};

const levelColors: Record<NotificationLevel, string> = {
  info: 'text-blue-500',
  warning: 'text-amber-500',
  error: 'text-red-500',
  success: 'text-green-500',
};

function formatRelativeTime(timestamp: number): string {
  const now = Date.now();
  const diff = now - timestamp;

  const seconds = Math.floor(diff / 1000);
  const minutes = Math.floor(seconds / 60);
  const hours = Math.floor(minutes / 60);
  const days = Math.floor(hours / 24);

  if (days > 0) return `${days}d`;
  if (hours > 0) return `${hours}h`;
  if (minutes > 0) return `${minutes}m`;
  return 'now';
}

export function NotificationItem({ notification }: NotificationItemProps) {
  const { t } = useTranslation();
  const Icon = levelIcons[notification.level];
  const colorClass = levelColors[notification.level];

  const handleClick = () => {
    if (!notification.read) {
      markAsRead(notification.id);
    }
  };

  const handleAction = (e: React.MouseEvent) => {
    e.stopPropagation();
    if (notification.actionEvent) {
      emitUiEvent(notification.actionEvent);
    }
    dismissNotification(notification.id);
  };

  const handleDismiss = (e: React.MouseEvent) => {
    e.stopPropagation();
    dismissNotification(notification.id);
  };

  return (
    <div
      className={cn(
        'group relative flex gap-2.5 p-2.5 rounded-md cursor-pointer transition-colors',
        'hover:bg-muted/50',
        !notification.read && 'bg-muted/30'
      )}
      onClick={handleClick}
      role="button"
      tabIndex={0}
      onKeyDown={e => {
        if (e.key === 'Enter' || e.key === ' ') {
          handleClick();
        }
      }}
    >
      {/* Level indicator */}
      <div className={cn('mt-0.5 shrink-0', colorClass)}>
        <Icon className="w-4 h-4" />
      </div>

      {/* Content */}
      <div className="flex-1 min-w-0">
        <div className="flex items-start justify-between gap-2">
          <p
            className={cn(
              'text-sm font-medium leading-tight',
              !notification.read && 'text-foreground',
              notification.read && 'text-muted-foreground'
            )}
          >
            {notification.title}
          </p>
          <span className="text-[10px] text-muted-foreground shrink-0 mt-0.5">
            {formatRelativeTime(notification.timestamp)}
          </span>
        </div>

        {notification.message && (
          <p className="text-xs text-muted-foreground mt-0.5 line-clamp-2">
            {notification.message}
          </p>
        )}

        {notification.actionLabel && (
          <button
            onClick={handleAction}
            className="text-xs text-primary hover:underline mt-1 inline-flex items-center gap-1"
          >
            {t(notification.actionLabel)}
            <ExternalLink className="w-3 h-3" />
          </button>
        )}
      </div>

      {/* Dismiss button */}
      <button
        onClick={handleDismiss}
        className={cn(
          'absolute top-1.5 right-1.5 p-0.5 rounded opacity-0 group-hover:opacity-100',
          'hover:bg-muted transition-opacity'
        )}
        aria-label={t('notifications.actions.dismiss')}
      >
        <X className="w-3 h-3 text-muted-foreground" />
      </button>

      {/* Unread indicator */}
      {!notification.read && (
        <div className="absolute left-1 top-1/2 -translate-y-1/2 w-1.5 h-1.5 rounded-full bg-primary" />
      )}
    </div>
  );
}
