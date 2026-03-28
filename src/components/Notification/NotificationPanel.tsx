// SPDX-License-Identifier: Apache-2.0

import { Bell, History, Sparkles, Trash2, Wrench, Zap } from 'lucide-react';
import { useCallback, useState, useSyncExternalStore } from 'react';
import { useTranslation } from 'react-i18next';
import { Button } from '@/components/ui/button';
import { ScrollArea } from '@/components/ui/scroll-area';
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
  DialogTrigger,
} from '@/components/ui/dialog';
import { CHANGELOG, type ChangelogItem } from '@/data/changelog';
import {
  clearAllNotifications,
  getNotificationsByCategory,
  type NotificationCategory,
  useNotifications,
} from '@/lib/notificationStore';
import { APP_VERSION } from '@/lib/version';
import { NotificationItem } from './NotificationItem';

const categoryOrder: NotificationCategory[] = ['system', 'query', 'security'];
const LAST_SEEN_VERSION_KEY = 'qoredb_last_seen_version';

// Changelog seen state — reactive via useSyncExternalStore
const changelogListeners = new Set<() => void>();
let changelogSnapshot = computeChangelogSnapshot();

function computeChangelogSnapshot(): { version: string; items: ChangelogItem[] } | null {
  const lastSeen = localStorage.getItem(LAST_SEEN_VERSION_KEY);
  if (lastSeen === APP_VERSION) return null;
  const entry = CHANGELOG[0];
  if (!entry) return null;
  return entry;
}

function notifyChangelogListeners() {
  changelogSnapshot = computeChangelogSnapshot();
  changelogListeners.forEach(l => l());
}

function subscribeChangelog(listener: () => void): () => void {
  changelogListeners.add(listener);
  return () => changelogListeners.delete(listener);
}

function getChangelogSnapshot() {
  return changelogSnapshot;
}

function markChangelogSeen() {
  localStorage.setItem(LAST_SEEN_VERSION_KEY, APP_VERSION);
  notifyChangelogListeners();
}

export function useHasUnseenChangelog(): boolean {
  const snapshot = useSyncExternalStore(subscribeChangelog, getChangelogSnapshot, getChangelogSnapshot);
  return snapshot !== null;
}

function useUnseenChangelog() {
  return useSyncExternalStore(subscribeChangelog, getChangelogSnapshot, getChangelogSnapshot);
}

const typeIcons: Record<ChangelogItem['type'], typeof Sparkles> = {
  feature: Sparkles,
  improvement: Wrench,
  fix: Zap,
};

const typeColors: Record<ChangelogItem['type'], string> = {
  feature: 'bg-accent/15 text-accent border-accent/30',
  improvement: 'bg-primary/15 text-primary border-primary/30',
  fix: 'bg-success/15 text-success border-success/30',
};

export function NotificationPanel() {
  const { t } = useTranslation();
  const notifications = useNotifications();

  const grouped = getNotificationsByCategory();
  const hasNotifications = notifications.length > 0;

  const unseenChangelog = useUnseenChangelog();

  const handleDismissChangelog = useCallback(() => {
    markChangelogSeen();
  }, []);

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
      <ScrollArea className="max-h-100 -mx-1 px-1">
        <div className="py-2 space-y-3">
          {/* What's New section */}
          {unseenChangelog && (
            <div>
              <div className="flex items-center gap-2 px-2 py-1">
                <span className="text-[10px] font-medium uppercase tracking-wider text-accent">
                  {t('whatsNew.titleWithVersion', { version: unseenChangelog.version })}
                </span>
                <div className="flex-1 h-px bg-accent/20" />
              </div>
              <div className="space-y-1.5 px-1">
                {unseenChangelog.items.map(item => {
                  const Icon = typeIcons[item.type];
                  return (
                    <div
                      key={item.title}
                      className="flex items-start gap-2.5 px-2 py-1.5 rounded-md bg-muted/30"
                    >
                      <span
                        className={`mt-0.5 inline-flex items-center justify-center w-5 h-5 rounded-full border text-[10px] ${typeColors[item.type]}`}
                      >
                        <Icon size={10} />
                      </span>
                      <div className="flex-1 min-w-0">
                        <p className="text-xs font-medium">{item.title}</p>
                        <p className="text-[11px] text-muted-foreground leading-snug">
                          {item.description}
                        </p>
                      </div>
                    </div>
                  );
                })}
              </div>
              <div className="px-2 pt-2">
                <Button
                  variant="ghost"
                  size="sm"
                  onClick={handleDismissChangelog}
                  className="h-6 px-2 text-[11px] text-muted-foreground hover:text-foreground w-full"
                >
                  {t('whatsNew.markSeen')}
                </Button>
              </div>
            </div>
          )}

          {/* Regular notifications */}
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

          {/* Empty state — only if no changelog AND no notifications */}
          {!hasNotifications && !unseenChangelog && (
            <div className="py-8 flex flex-col items-center justify-center text-muted-foreground">
              <Bell className="w-8 h-8 mb-2 opacity-40" />
              <p className="text-sm">{t('notifications.empty')}</p>
            </div>
          )}
        </div>
      </ScrollArea>

      {/* Full changelog button */}
      <FullChangelogDialog />
    </div>
  );
}

function FullChangelogDialog() {
  const { t } = useTranslation();
  const [open, setOpen] = useState(false);

  return (
    <Dialog open={open} onOpenChange={setOpen}>
      <DialogTrigger asChild>
        <div className="border-t border-border pt-2 mt-1">
          <Button
            variant="ghost"
            size="sm"
            className="h-7 px-2 text-xs text-muted-foreground hover:text-foreground w-full"
          >
            <History className="w-3 h-3 mr-1.5" />
            {t('whatsNew.fullChangelog')}
          </Button>
        </div>
      </DialogTrigger>
      <DialogContent className="max-w-lg max-h-[80vh] overflow-hidden flex flex-col">
        <DialogHeader>
          <DialogTitle>{t('whatsNew.fullChangelog')}</DialogTitle>
        </DialogHeader>
        <ScrollArea className="flex-1 -mx-2 px-2">
          <div className="space-y-6 py-2">
            {CHANGELOG.map(entry => (
              <div key={entry.version}>
                <div className="flex items-center gap-2 mb-2">
                  <span className="text-sm font-semibold">v{entry.version}</span>
                  <span className="text-xs text-muted-foreground">{entry.date}</span>
                </div>
                <div className="space-y-1.5 pl-1">
                  {entry.items.map(item => {
                    const Icon = typeIcons[item.type];
                    return (
                      <div key={item.title} className="flex items-start gap-2.5 py-1">
                        <span
                          className={`mt-0.5 inline-flex items-center justify-center w-5 h-5 rounded-full border text-[10px] shrink-0 ${typeColors[item.type]}`}
                        >
                          <Icon size={10} />
                        </span>
                        <div className="flex-1 min-w-0">
                          <p className="text-xs font-medium">{item.title}</p>
                          <p className="text-[11px] text-muted-foreground leading-snug">
                            {item.description}
                          </p>
                        </div>
                      </div>
                    );
                  })}
                </div>
              </div>
            ))}
          </div>
        </ScrollArea>
      </DialogContent>
    </Dialog>
  );
}
