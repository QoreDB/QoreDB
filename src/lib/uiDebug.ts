// SPDX-License-Identifier: Apache-2.0

import { invoke } from '@tauri-apps/api/core';
import { useCallback, useEffect, useRef, useState } from 'react';

declare global {
  interface Window {
    __QOREDB_UI_DEBUG__?: boolean;
    __QOREDB_UI_DEBUG_FILE__?: boolean;
    __QOREDB_UI_DEBUG_FILE_PATH__?: string;
  }
}

let debugInstanceCounter = 0;
let uiDebugFileFlushTimer: ReturnType<typeof setTimeout> | null = null;
let uiDebugFileFlushPromise: Promise<void> | null = null;
let uiDebugFileLoggingDisabled = false;
let uiDebugFileSessionHeaderWritten = false;
let uiDebugFilePathAnnounced = false;
let uiDebugFileErrorAnnounced = false;
const uiDebugFileChunks: string[] = [];

const UI_DEBUG_FILE_FLUSH_DELAY_MS = 250;
const UI_DEBUG_FILE_FLUSH_EAGER_THRESHOLD = 40;
const UI_DEBUG_LOG_COMMAND = 'append_ui_debug_log';

function isUiDebugEnabled(): boolean {
  if (!import.meta.env.DEV) return false;
  if (typeof window === 'undefined') return true;
  return window.__QOREDB_UI_DEBUG__ !== false;
}

function isUiDebugFileEnabled(): boolean {
  if (!isUiDebugEnabled()) return false;
  if (typeof window === 'undefined') return true;
  return window.__QOREDB_UI_DEBUG_FILE__ !== false;
}

function getDebugTimestamp(): string {
  if (typeof performance !== 'undefined') {
    return performance.now().toFixed(1);
  }
  return String(Date.now());
}

function getDebugIsoTimestamp(): string {
  return new Date().toISOString();
}

function safeSerialize(value: unknown): string {
  try {
    return JSON.stringify(value, (_key, currentValue) => {
      if (typeof currentValue === 'function') {
        return `[function ${currentValue.name || 'anonymous'}]`;
      }
      if (typeof HTMLElement !== 'undefined' && currentValue instanceof HTMLElement) {
        return `[HTMLElement ${currentValue.tagName}]`;
      }
      if (typeof Event !== 'undefined' && currentValue instanceof Event) {
        return `[Event ${currentValue.type}]`;
      }
      return currentValue;
    });
  } catch (error) {
    return `[unserializable:${error instanceof Error ? error.message : 'unknown'}]`;
  }
}

function getUiDebugSessionHeader(): string {
  const location =
    typeof window !== 'undefined' ? ` ${window.location.pathname}${window.location.search}` : '';
  return `\n===== UI DEBUG SESSION ${getDebugIsoTimestamp()}${location} =====\n`;
}

function scheduleUiDebugFileFlush(): void {
  if (!isUiDebugFileEnabled() || uiDebugFileLoggingDisabled) return;
  if (uiDebugFileFlushTimer !== null) return;

  uiDebugFileFlushTimer = setTimeout(() => {
    uiDebugFileFlushTimer = null;
    void flushUiDebugFile();
  }, UI_DEBUG_FILE_FLUSH_DELAY_MS);
}

async function flushUiDebugFile(): Promise<void> {
  if (!isUiDebugFileEnabled() || uiDebugFileLoggingDisabled) return;
  if (uiDebugFileFlushPromise) {
    await uiDebugFileFlushPromise;
    return;
  }

  const chunks = uiDebugFileChunks.splice(0);
  if (chunks.length === 0) return;

  const content = `${uiDebugFileSessionHeaderWritten ? '' : getUiDebugSessionHeader()}${chunks.join('')}`;

  uiDebugFileFlushPromise = (async () => {
    try {
      const filePath = await invoke<string>(UI_DEBUG_LOG_COMMAND, { content });
      uiDebugFileSessionHeaderWritten = true;

      if (!uiDebugFilePathAnnounced) {
        uiDebugFilePathAnnounced = true;
        if (typeof window !== 'undefined') {
          window.__QOREDB_UI_DEBUG_FILE_PATH__ = filePath;
        }
        console.info(`[ui-debug] file logging enabled: ${filePath}`);
      }

      uiDebugFileErrorAnnounced = false;
    } catch (error) {
      uiDebugFileChunks.unshift(content);

      if (!uiDebugFileErrorAnnounced) {
        uiDebugFileErrorAnnounced = true;
        console.warn('[ui-debug] failed to append logs to ui-debug.txt', error);
      }

      uiDebugFileLoggingDisabled = true;
    } finally {
      uiDebugFileFlushPromise = null;
      if (uiDebugFileFlushTimer !== null) {
        clearTimeout(uiDebugFileFlushTimer);
        uiDebugFileFlushTimer = null;
      }
      if (uiDebugFileChunks.length > 0 && !uiDebugFileLoggingDisabled) {
        scheduleUiDebugFileFlush();
      }
    }
  })();

  await uiDebugFileFlushPromise;
}

function queueUiDebugFileLine(line: string): void {
  if (!isUiDebugFileEnabled() || uiDebugFileLoggingDisabled) return;

  uiDebugFileChunks.push(`${line}\n`);

  if (uiDebugFileChunks.length >= UI_DEBUG_FILE_FLUSH_EAGER_THRESHOLD) {
    if (uiDebugFileFlushTimer !== null) {
      clearTimeout(uiDebugFileFlushTimer);
      uiDebugFileFlushTimer = null;
    }
    void flushUiDebugFile();
    return;
  }

  scheduleUiDebugFileFlush();
}

export function uiDebugLog(scope: string, event: string, details?: Record<string, unknown>): void {
  if (!isUiDebugEnabled()) return;

  const prefix = `[ui-debug ${getDebugTimestamp()}] ${scope} ${event}`;
  const filePrefix = `[${getDebugIsoTimestamp()}] ${prefix}`;
  if (details && Object.keys(details).length > 0) {
    console.log(prefix, details);
    queueUiDebugFileLine(`${filePrefix} ${safeSerialize(details)}`);
    return;
  }
  console.log(prefix);
  queueUiDebugFileLine(filePrefix);
}

export function useUiDebugSnapshot(
  name: string | undefined,
  snapshot: Record<string, unknown>
): string | null {
  const instanceIdRef = useRef<number>(0);
  if (instanceIdRef.current === 0) {
    instanceIdRef.current = ++debugInstanceCounter;
  }

  const scope = name ? `${name}#${instanceIdRef.current}` : null;
  const snapshotString = safeSerialize(snapshot);
  const previousSnapshotRef = useRef<string | null>(null);

  useEffect(() => {
    if (!scope) return;
    uiDebugLog(scope, 'mount');
    return () => uiDebugLog(scope, 'unmount');
  }, [scope]);

  useEffect(() => {
    if (!scope) return;
    if (previousSnapshotRef.current === snapshotString) return;

    previousSnapshotRef.current = snapshotString;
    uiDebugLog(scope, 'snapshot', snapshot);
  }, [scope, snapshot, snapshotString]);

  return scope;
}

export function useUiDebugElement<T extends HTMLElement>(
  name: string | undefined,
  snapshot: Record<string, unknown> = {}
): (node: T | null) => void {
  const scope = useUiDebugSnapshot(name, snapshot);
  const [element, setElement] = useState<T | null>(null);

  const setElementRef = useCallback((node: T | null) => {
    setElement(previous => (previous === node ? previous : node));
  }, []);

  useEffect(() => {
    if (!scope) return;

    if (!element) {
      uiDebugLog(scope, 'missing-element');
      return;
    }

    const getElementSnapshot = () => ({
      tagName: element.tagName,
      dataState: element.getAttribute('data-state'),
      dataSide: element.getAttribute('data-side'),
      dataAlign: element.getAttribute('data-align'),
      dataSlot: element.getAttribute('data-slot'),
      style: element.getAttribute('style'),
      sameSlotCount:
        typeof document !== 'undefined' && element.getAttribute('data-slot')
          ? document.querySelectorAll(`[data-slot="${element.getAttribute('data-slot')}"]`).length
          : null,
      sameSlotOpenCount:
        typeof document !== 'undefined' && element.getAttribute('data-slot')
          ? Array.from(
              document.querySelectorAll<HTMLElement>(
                `[data-slot="${element.getAttribute('data-slot')}"]`
              )
            ).filter(candidate => candidate.getAttribute('data-state') === 'open').length
          : null,
    });

    uiDebugLog(scope, 'element-mount', getElementSnapshot());

    const mutationObserver = new MutationObserver(mutations => {
      for (const mutation of mutations) {
        uiDebugLog(scope, 'attribute-change', {
          attributeName: mutation.attributeName,
          ...getElementSnapshot(),
        });
      }
    });

    mutationObserver.observe(element, {
      attributes: true,
      attributeFilter: ['data-state', 'data-side', 'data-align', 'style'],
    });

    const handleAnimationStart = (event: AnimationEvent) => {
      uiDebugLog(scope, 'animationstart', {
        animationName: event.animationName,
        elapsedTime: event.elapsedTime,
        ...getElementSnapshot(),
      });
    };

    const handleAnimationEnd = (event: AnimationEvent) => {
      uiDebugLog(scope, 'animationend', {
        animationName: event.animationName,
        elapsedTime: event.elapsedTime,
        ...getElementSnapshot(),
      });
    };

    const handleTransitionEnd = (event: TransitionEvent) => {
      uiDebugLog(scope, 'transitionend', {
        propertyName: event.propertyName,
        elapsedTime: event.elapsedTime,
        ...getElementSnapshot(),
      });
    };

    element.addEventListener('animationstart', handleAnimationStart);
    element.addEventListener('animationend', handleAnimationEnd);
    element.addEventListener('transitionend', handleTransitionEnd);

    return () => {
      mutationObserver.disconnect();
      element.removeEventListener('animationstart', handleAnimationStart);
      element.removeEventListener('animationend', handleAnimationEnd);
      element.removeEventListener('transitionend', handleTransitionEnd);
      uiDebugLog(scope, 'element-unmount', getElementSnapshot());
    };
  }, [scope, element]);

  return setElementRef;
}
