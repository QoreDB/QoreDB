// SPDX-License-Identifier: Apache-2.0

import { useCallback, useEffect, useRef, useState } from 'react';

const STORAGE_KEY = 'sidebar-width';
const MIN_WIDTH = 200;
const DEFAULT_WIDTH = 256;

function getMaxWidth() {
  return Math.floor(window.innerWidth * 0.5);
}

function clampWidth(width: number): number {
  return Math.min(Math.max(width, MIN_WIDTH), getMaxWidth());
}

function loadWidth(): number {
  try {
    const stored = localStorage.getItem(STORAGE_KEY);
    if (stored) {
      const parsed = Number(stored);
      if (Number.isFinite(parsed)) return clampWidth(parsed);
    }
  } catch {
    // ignore
  }
  return DEFAULT_WIDTH;
}

export function useResizableSidebar() {
  const [width, setWidth] = useState(loadWidth);
  const sidebarRef = useRef<HTMLElement>(null);
  const isDragging = useRef(false);
  const startX = useRef(0);
  const startWidth = useRef(0);
  const latestWidth = useRef(width);

  const saveWidth = useCallback((w: number) => {
    try {
      localStorage.setItem(STORAGE_KEY, String(w));
    } catch {
      // ignore
    }
  }, []);

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
    setWidth(DEFAULT_WIDTH);
    saveWidth(DEFAULT_WIDTH);
  }, [saveWidth]);

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

  return { width, sidebarRef, handleMouseDown, resetWidth };
}
