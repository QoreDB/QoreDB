// SPDX-License-Identifier: Apache-2.0

import { useCallback, useEffect, useRef, useState } from 'react';
import { setSidebarVisible, toggleSidebar, useModalStore } from '@/lib/stores/modalStore';
import { useWorkspaceStore } from '@/lib/stores/workspaceStore';

export const SIDEBAR_STORAGE_KEY = 'sidebar-width';
export const MIN_SIDEBAR_WIDTH = 200;
export const DEFAULT_SIDEBAR_WIDTH = 256;

const KEYBOARD_STEP = 16;
const KEYBOARD_LARGE_STEP = 48;

export function panelStorageKey(baseKey: string, projectId = 'default'): string {
  const scope = projectId.trim() || 'default';
  return `${baseKey}:${scope}`;
}

export function panelCollapsedStorageKey(baseKey: string, projectId = 'default'): string {
  const scope = projectId.trim() || 'default';
  return `${baseKey}-collapsed:${scope}`;
}

export function clampPanelSize(size: number, min: number, max: number): number {
  const safeMax = Math.max(min, max);
  if (!Number.isFinite(size)) return min;
  return Math.min(Math.max(Math.round(size), min), safeMax);
}

export function loadPanelSize(
  baseKey: string,
  defaultSize: number,
  min: number,
  max: number,
  projectId = 'default'
): number {
  try {
    const scoped = localStorage.getItem(panelStorageKey(baseKey, projectId));
    const raw = scoped ?? localStorage.getItem(baseKey);
    if (raw !== null) {
      const parsed = Number(raw);
      if (Number.isFinite(parsed)) {
        return clampPanelSize(parsed, min, max);
      }
    }
  } catch {
    // ignore storage errors
  }
  return clampPanelSize(defaultSize, min, max);
}

export function savePanelSize(baseKey: string, size: number, projectId = 'default'): void {
  const serialized = String(Math.round(size));
  try {
    localStorage.setItem(panelStorageKey(baseKey, projectId), serialized);
    localStorage.setItem(baseKey, serialized);
  } catch {
    // ignore storage errors
  }
}

export function loadPanelCollapsed(
  baseKey: string,
  defaultCollapsed = false,
  projectId = 'default'
): boolean {
  try {
    const raw = localStorage.getItem(panelCollapsedStorageKey(baseKey, projectId));
    if (raw === 'true') return true;
    if (raw === 'false') return false;
  } catch {
    // ignore storage errors
  }
  return defaultCollapsed;
}

export function savePanelCollapsed(
  baseKey: string,
  collapsed: boolean,
  projectId = 'default'
): void {
  try {
    localStorage.setItem(panelCollapsedStorageKey(baseKey, projectId), String(collapsed));
  } catch {
    // ignore storage errors
  }
}

export function computeKeyboardResizeDelta(
  key: string,
  orientation: 'horizontal' | 'vertical',
  shiftKey = false,
  invert = false
): number | 'min' | 'max' | null {
  const step = (shiftKey ? KEYBOARD_LARGE_STEP : KEYBOARD_STEP) * (invert ? -1 : 1);
  if (orientation === 'horizontal') {
    if (key === 'ArrowRight') return step;
    if (key === 'ArrowLeft') return -step;
  } else {
    if (key === 'ArrowDown') return step;
    if (key === 'ArrowUp') return -step;
  }
  if (key === 'Home') return 'min';
  if (key === 'End') return 'max';
  return null;
}

function getMaxWidth() {
  if (typeof window === 'undefined' || !window.innerWidth) return 600;
  return Math.max(MIN_SIDEBAR_WIDTH, Math.floor(window.innerWidth * 0.5));
}

function clampWidth(width: number): number {
  return clampPanelSize(width, MIN_SIDEBAR_WIDTH, getMaxWidth());
}

export function useResizableSidebar() {
  const projectId = useWorkspaceStore(s => s.projectId) || 'default';
  const sidebarVisible = useModalStore(s => s.sidebarVisible);
  const [width, setWidth] = useState(() =>
    loadPanelSize(
      SIDEBAR_STORAGE_KEY,
      DEFAULT_SIDEBAR_WIDTH,
      MIN_SIDEBAR_WIDTH,
      getMaxWidth(),
      projectId
    )
  );
  const sidebarRef = useRef<HTMLElement>(null);
  const isDragging = useRef(false);
  const startX = useRef(0);
  const startWidth = useRef(0);
  const latestWidth = useRef(width);
  const hydratedScopeRef = useRef<string | null>(null);

  const saveWidth = useCallback(
    (w: number) => {
      savePanelSize(SIDEBAR_STORAGE_KEY, w, projectId);
    },
    [projectId]
  );

  // Hydrate width & collapsed state when switching workspaces
  useEffect(() => {
    const nextWidth = loadPanelSize(
      SIDEBAR_STORAGE_KEY,
      DEFAULT_SIDEBAR_WIDTH,
      MIN_SIDEBAR_WIDTH,
      getMaxWidth(),
      projectId
    );
    latestWidth.current = nextWidth;
    setWidth(nextWidth);
    const collapsed = loadPanelCollapsed(SIDEBAR_STORAGE_KEY, false, projectId);
    hydratedScopeRef.current = projectId;
    setSidebarVisible(!collapsed);
  }, [projectId]);

  // Persist collapsed state per workspace when toggled
  useEffect(() => {
    if (hydratedScopeRef.current !== projectId) return;
    savePanelCollapsed(SIDEBAR_STORAGE_KEY, !sidebarVisible, projectId);
  }, [projectId, sidebarVisible]);

  const handleMouseDown = useCallback(
    (e: React.MouseEvent) => {
      e.preventDefault();
      isDragging.current = true;
      startX.current = e.clientX;
      startWidth.current = width;
      latestWidth.current = width;
      document.body.style.userSelect = 'none';
      document.body.style.cursor = 'col-resize';
    },
    [width]
  );

  const resetWidth = useCallback(() => {
    const next = clampWidth(DEFAULT_SIDEBAR_WIDTH);
    latestWidth.current = next;
    setWidth(next);
    saveWidth(next);
    if (sidebarRef.current) {
      sidebarRef.current.style.width = `${next}px`;
      sidebarRef.current.style.minWidth = `${next}px`;
    }
  }, [saveWidth]);

  const handleKeyDown = useCallback(
    (e: React.KeyboardEvent) => {
      if (e.key === 'Enter') {
        e.preventDefault();
        toggleSidebar();
        return;
      }
      const action = computeKeyboardResizeDelta(e.key, 'horizontal', e.shiftKey);
      if (action === null) return;
      e.preventDefault();
      const max = getMaxWidth();
      const nextWidth =
        action === 'min'
          ? MIN_SIDEBAR_WIDTH
          : action === 'max'
            ? max
            : clampPanelSize(width + action, MIN_SIDEBAR_WIDTH, max);
      latestWidth.current = nextWidth;
      setWidth(nextWidth);
      saveWidth(nextWidth);
    },
    [saveWidth, width]
  );

  useEffect(() => {
    const handleMouseMove = (e: MouseEvent) => {
      if (!isDragging.current) return;
      const delta = e.clientX - startX.current;
      const newWidth = clampWidth(startWidth.current + delta);
      latestWidth.current = newWidth;
      // Write the DOM directly during the drag so we don't re-render the whole
      // sidebar tree (and the app root) on every mousemove; commit on mouseup.
      const el = sidebarRef.current;
      if (el) {
        el.style.width = `${newWidth}px`;
        el.style.minWidth = `${newWidth}px`;
      }
    };

    const handleMouseUp = () => {
      if (!isDragging.current) return;
      isDragging.current = false;
      document.body.style.userSelect = '';
      document.body.style.cursor = '';
      setWidth(latestWidth.current);
      saveWidth(latestWidth.current);
    };

    document.addEventListener('mousemove', handleMouseMove);
    document.addEventListener('mouseup', handleMouseUp);
    return () => {
      document.removeEventListener('mousemove', handleMouseMove);
      document.removeEventListener('mouseup', handleMouseUp);
    };
  }, [saveWidth]);

  // Re-clamp on window resize
  useEffect(() => {
    const handleResize = () => {
      setWidth(w => clampWidth(w));
    };
    window.addEventListener('resize', handleResize);
    return () => window.removeEventListener('resize', handleResize);
  }, []);

  return { width, sidebarRef, handleMouseDown, handleKeyDown, resetWidth };
}
