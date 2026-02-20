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
  const isDragging = useRef(false);
  const startX = useRef(0);
  const startWidth = useRef(0);

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
      document.body.style.userSelect = 'none';
      document.body.style.cursor = 'col-resize';
    },
    [width],
  );

  useEffect(() => {
    const handleMouseMove = (e: MouseEvent) => {
      if (!isDragging.current) return;
      const delta = e.clientX - startX.current;
      const newWidth = clampWidth(startWidth.current + delta);
      setWidth(newWidth);
    };

    const handleMouseUp = () => {
      if (!isDragging.current) return;
      isDragging.current = false;
      document.body.style.userSelect = '';
      document.body.style.cursor = '';
      setWidth((w) => {
        saveWidth(w);
        return w;
      });
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
      setWidth((w) => clampWidth(w));
    };
    window.addEventListener('resize', handleResize);
    return () => window.removeEventListener('resize', handleResize);
  }, []);

  return { width, handleMouseDown };
}
