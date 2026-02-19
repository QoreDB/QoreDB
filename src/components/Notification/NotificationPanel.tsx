// SPDX-License-Identifier: Apache-2.0

import { Bell, Trash2 } from 'lucide-react';
import { useTranslation } from 'react-i18next';
import { Button } from '@/components/ui/button';
import { ScrollArea } from '@/components/ui/scroll-area';
import {
  clearAllNotifications,
  getNotificationsByCategory,
  type NotificationCategory,
  useNotifications,
} from '@/lib/notificationStore';
import { NotificationItem } from './NotificationItem';

const categoryOrder: NotificationCategory[] = ['system', 'query', 'security'];

export function NotificationPanel() {
  const { t } = useTranslation();
  const notifications = useNotifications();

  const grouped = getNotificationsByCategory();
  const hasNotifications = notifications.length > 0;

  return (
    <div className="w-80">
      {/* Header */}
      <div className="flex items-center justify-between pb-3 border-b border-border">
        <h3 className="text-sm font-semibold">{t('notifications.title')}</h3>
        {hasNotifications && (
          <Button
            variant="ghost"
            size="sm"
            onClick={clearAllNotifications}
            className="h-7 px-2 text-xs text-muted-foreground hover:text-foreground"
          >
            <Trash2 className="w-3 h-3 mr-1" />
            {t('notifications.clearAll')}
          </Button>
        )}
      </div>

      {/* Content */}
      {hasNotifications ? (
        <ScrollArea className="max-h-[400px] -mx-1 px-1">
          <div className="py-2 space-y-3">
            {categoryOrder.map(category => {
              const items = grouped[category];
              if (items.length === 0) return null;

              return (
                <div key={category}>
                  <div className="flex items-center gap-2 px-2 py-1">
                    <span className="text-[10px] font-medium uppercase tracking-wider text-muted-foreground">
                      {t(`notifications.categories.${category}`)}
                    </span>
                    <div className="flex-1 h-px bg-border/50" />
                  </div>
                  <div className="space-y-0.5">
                    {items.map(notification => (
                      <NotificationItem key={notification.id} notification={notification} />
                    ))}
                  </div>
                </div>
              );
            })}
          </div>
        </ScrollArea>
      ) : (
        <div className="py-8 flex flex-col items-center justify-center text-muted-foreground">
          <Bell className="w-8 h-8 mb-2 opacity-40" />
          <p className="text-sm">{t('notifications.empty')}</p>
        </div>
      )}
    </div>
  );
}
