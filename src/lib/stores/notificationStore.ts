// SPDX-License-Identifier: Apache-2.0

/**
 * Persistent notifications for the Bell panel.
 *
 * Unlike toasts (Sonner): these persist until dismissed, are grouped by
 * category, and only warnings/errors contribute to the badge.
 */

export type NotificationLevel = 'info' | 'warning' | 'error' | 'success';
export type NotificationCategory = 'system' | 'query' | 'security';

export interface Notification {
  id: string;
  level: NotificationLevel;
  category: NotificationCategory;
  title: string;
  message?: string;
  timestamp: number;
  read: boolean;
  /** Optional action button label (translation key) */
  actionLabel?: string;
  /** Event name to dispatch when action is clicked */
  actionEvent?: string;
  /** Link to context (connection, query, table) */
  contextLink?: {
    type: 'connection' | 'query' | 'table';
    id: string;
  };
  /** If true, notification auto-resolves when condition clears */
  autoResolve?: boolean;
  /** Unique key to prevent duplicates (e.g., "connection_lost:session_123") */
  dedupeKey?: string;
}

export type NotificationListener = () => void;

const STORAGE_KEY = 'qoredb_notifications';
const MAX_NOTIFICATIONS = 50;
const RETENTION_MS = 24 * 60 * 60 * 1000; // 24 hours

let notifications: Notification[] = [];
const listeners: Set<NotificationListener> = new Set();

// Cached snapshots for useSyncExternalStore (must be stable references)
let cachedNotifications: Notification[] = [];
let cachedBadgeCount: number = 0;

function updateSnapshots(): void {
  cachedNotifications = [...notifications];
  cachedBadgeCount = notifications.filter(
    n => !n.read && (n.level === 'warning' || n.level === 'error')
  ).length;
}

function generateId(): string {
  return `${Date.now()}-${Math.random().toString(36).slice(2, 9)}`;
}

function notifyListeners(): void {
  updateSnapshots();
  listeners.forEach(listener => {
    try {
      listener();
    } catch {
      // Ignore listener errors
    }
  });
}

function persist(): void {
  try {
    localStorage.setItem(STORAGE_KEY, JSON.stringify(notifications));
  } catch {
    // Storage full or unavailable, ignore
  }
}

function load(): void {
  try {
    const data = localStorage.getItem(STORAGE_KEY);
    if (data) {
      const parsed = JSON.parse(data) as Notification[];
      const cutoff = Date.now() - RETENTION_MS;
      notifications = parsed.filter(n => n.timestamp > cutoff);
      if (notifications.length > MAX_NOTIFICATIONS) {
        notifications = notifications.slice(0, MAX_NOTIFICATIONS);
      }
      persist();
    }
  } catch {
    notifications = [];
  }
  updateSnapshots();
}

load();

/** Returns a stable reference for useSyncExternalStore. */
export function getNotifications(): Notification[] {
  return cachedNotifications;
}

export function getNotificationsByCategory(): Record<NotificationCategory, Notification[]> {
  const grouped: Record<NotificationCategory, Notification[]> = {
    system: [],
    query: [],
    security: [],
  };

  for (const n of cachedNotifications) {
    grouped[n.category].push(n);
  }

  return grouped;
}

/** Returns a stable value for useSyncExternalStore. */
export function getUnreadBadgeCount(): number {
  return cachedBadgeCount;
}

export function addNotification(
  input: Omit<Notification, 'id' | 'timestamp' | 'read'>
): Notification {
  if (input.dedupeKey) {
    const existing = notifications.find(n => n.dedupeKey === input.dedupeKey);
    if (existing) {
      // Bump to top instead of creating a duplicate
      existing.timestamp = Date.now();
      existing.read = false;
      persist();
      notifyListeners();
      return existing;
    }
  }

  const notification: Notification = {
    ...input,
    id: generateId(),
    timestamp: Date.now(),
    read: false,
  };

  notifications.unshift(notification);

  if (notifications.length > MAX_NOTIFICATIONS) {
    notifications = notifications.slice(0, MAX_NOTIFICATIONS);
  }

  persist();
  notifyListeners();

  return notification;
}

export function markAsRead(id: string): void {
  const notification = notifications.find(n => n.id === id);
  if (notification && !notification.read) {
    notification.read = true;
    persist();
    notifyListeners();
  }
}

export function markAllAsRead(): void {
  let changed = false;
  for (const n of notifications) {
    if (!n.read) {
      n.read = true;
      changed = true;
    }
  }
  if (changed) {
    persist();
    notifyListeners();
  }
}

export function dismissNotification(id: string): void {
  const index = notifications.findIndex(n => n.id === id);
  if (index !== -1) {
    notifications.splice(index, 1);
    persist();
    notifyListeners();
  }
}

/** Removes a notification by dedupeKey, used when its condition clears. */
export function resolveByKey(dedupeKey: string): void {
  const index = notifications.findIndex(n => n.dedupeKey === dedupeKey);
  if (index !== -1) {
    notifications.splice(index, 1);
    persist();
    notifyListeners();
  }
}

export function clearAllNotifications(): void {
  if (notifications.length > 0) {
    notifications = [];
    persist();
    notifyListeners();
  }
}

export function subscribeNotifications(listener: NotificationListener): () => void {
  listeners.add(listener);
  return () => {
    listeners.delete(listener);
  };
}

import { useSyncExternalStore } from 'react';

export function useNotifications(): Notification[] {
  return useSyncExternalStore(subscribeNotifications, getNotifications, getNotifications);
}

export function useNotificationBadge(): number {
  return useSyncExternalStore(subscribeNotifications, getUnreadBadgeCount, getUnreadBadgeCount);
}

if (import.meta.env.DEV) {
  (window as unknown as Record<string, unknown>).__addTestNotification = (
    input: Omit<Notification, 'id' | 'timestamp' | 'read'>
  ) => addNotification(input);

  (window as unknown as Record<string, unknown>).__clearNotifications = clearAllNotifications;
}
