/**
 * Notification Store
 * 
 * Reactive store for managing persistent notifications in the Bell panel.
 * Follows the pattern of sandboxStore.ts for reactive updates.
 * 
 * Key distinction from toasts (Sonner):
 * - Notifications persist until dismissed
 * - Grouped by category
 * - Badge only for warnings/errors
 */

// ============================================
// TYPES
// ============================================

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

// ============================================
// CONSTANTS
// ============================================

const STORAGE_KEY = 'qoredb_notifications';
const MAX_NOTIFICATIONS = 50;
const RETENTION_MS = 24 * 60 * 60 * 1000; // 24 hours

// ============================================
// STATE
// ============================================

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

// ============================================
// HELPERS
// ============================================

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
      // Filter out expired notifications
      notifications = parsed.filter(n => n.timestamp > cutoff);
      // Trim if over max
      if (notifications.length > MAX_NOTIFICATIONS) {
        notifications = notifications.slice(0, MAX_NOTIFICATIONS);
      }
      persist(); // Save cleaned list
    }
  } catch {
    notifications = [];
  }
  updateSnapshots(); // Initialize snapshots after load
}

// Initialize on module load
load();

// ============================================
// PUBLIC API
// ============================================

/**
 * Get all notifications, sorted by timestamp (newest first)
 * Returns a stable reference for useSyncExternalStore
 */
export function getNotifications(): Notification[] {
  return cachedNotifications;
}

/**
 * Get notifications grouped by category
 */
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

/**
 * Get count of unread warnings and errors (for badge)
 * Returns a stable value for useSyncExternalStore
 */
export function getUnreadBadgeCount(): number {
  return cachedBadgeCount;
}

/**
 * Add a new notification
 */
export function addNotification(
  input: Omit<Notification, 'id' | 'timestamp' | 'read'>
): Notification {
  // Check for duplicate by dedupeKey
  if (input.dedupeKey) {
    const existing = notifications.find(n => n.dedupeKey === input.dedupeKey);
    if (existing) {
      // Update timestamp to bring it to top, but don't create duplicate
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

  // Add to beginning (newest first)
  notifications.unshift(notification);

  // Trim if over max
  if (notifications.length > MAX_NOTIFICATIONS) {
    notifications = notifications.slice(0, MAX_NOTIFICATIONS);
  }

  persist();
  notifyListeners();

  return notification;
}

/**
 * Mark a notification as read
 */
export function markAsRead(id: string): void {
  const notification = notifications.find(n => n.id === id);
  if (notification && !notification.read) {
    notification.read = true;
    persist();
    notifyListeners();
  }
}

/**
 * Mark all notifications as read
 */
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

/**
 * Dismiss (remove) a notification
 */
export function dismissNotification(id: string): void {
  const index = notifications.findIndex(n => n.id === id);
  if (index !== -1) {
    notifications.splice(index, 1);
    persist();
    notifyListeners();
  }
}

/**
 * Resolve notifications by dedupeKey (for auto-resolving notifications)
 */
export function resolveByKey(dedupeKey: string): void {
  const index = notifications.findIndex(n => n.dedupeKey === dedupeKey);
  if (index !== -1) {
    notifications.splice(index, 1);
    persist();
    notifyListeners();
  }
}

/**
 * Clear all notifications
 */
export function clearAllNotifications(): void {
  if (notifications.length > 0) {
    notifications = [];
    persist();
    notifyListeners();
  }
}

/**
 * Subscribe to notification changes
 */
export function subscribeNotifications(listener: NotificationListener): () => void {
  listeners.add(listener);
  return () => {
    listeners.delete(listener);
  };
}

// ============================================
// REACT HOOK
// ============================================

import { useSyncExternalStore } from 'react';

/**
 * React hook for subscribing to notifications
 */
export function useNotifications(): Notification[] {
  return useSyncExternalStore(
    subscribeNotifications,
    getNotifications,
    getNotifications
  );
}

/**
 * React hook for badge count
 */
export function useNotificationBadge(): number {
  return useSyncExternalStore(
    subscribeNotifications,
    getUnreadBadgeCount,
    getUnreadBadgeCount
  );
}

// ============================================
// DEV HELPER (removed in production build)
// ============================================

if (import.meta.env.DEV) {
  (window as unknown as Record<string, unknown>).__addTestNotification = (
    input: Omit<Notification, 'id' | 'timestamp' | 'read'>
  ) => addNotification(input);
  
  (window as unknown as Record<string, unknown>).__clearNotifications = clearAllNotifications;
}

