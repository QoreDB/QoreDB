// SPDX-License-Identifier: Apache-2.0

import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import {
  clampPanelSize,
  computeKeyboardResizeDelta,
  loadPanelCollapsed,
  loadPanelSize,
  panelCollapsedStorageKey,
  panelStorageKey,
  savePanelCollapsed,
  savePanelSize,
} from './useResizableSidebar';

describe('resizable panel persistence & keyboard controls', () => {
  beforeEach(() => {
    const store = new Map<string, string>();
    vi.stubGlobal('localStorage', {
      getItem: (key: string) => store.get(key) ?? null,
      setItem: (key: string, value: string) => store.set(key, value),
      removeItem: (key: string) => store.delete(key),
      clear: () => store.clear(),
    });
  });

  afterEach(() => {
    vi.unstubAllGlobals();
  });

  it('clamps panel sizes safely within min and max bounds', () => {
    expect(clampPanelSize(150, 200, 500)).toBe(200);
    expect(clampPanelSize(650, 200, 500)).toBe(500);
    expect(clampPanelSize(312.6, 200, 500)).toBe(313);
    expect(clampPanelSize(Number.NaN, 200, 500)).toBe(200);
  });

  it('persists and loads panel dimensions isolated per workspace with legacy fallback', () => {
    localStorage.setItem('sidebar-width', '280');
    expect(loadPanelSize('sidebar-width', 256, 200, 500, 'ws-a')).toBe(280);

    savePanelSize('sidebar-width', 340, 'ws-a');
    savePanelSize('sidebar-width', 420, 'ws-b');

    expect(localStorage.getItem(panelStorageKey('sidebar-width', 'ws-a'))).toBe('340');
    expect(localStorage.getItem(panelStorageKey('sidebar-width', 'ws-b'))).toBe('420');
    expect(loadPanelSize('sidebar-width', 256, 200, 500, 'ws-a')).toBe(340);
    expect(loadPanelSize('sidebar-width', 256, 200, 500, 'ws-b')).toBe(420);
  });

  it('persists and loads collapsed state per workspace', () => {
    expect(loadPanelCollapsed('sidebar-width', false, 'ws-a')).toBe(false);
    expect(loadPanelCollapsed('ai-panel-width', true, 'ws-a')).toBe(true);

    savePanelCollapsed('sidebar-width', true, 'ws-a');
    savePanelCollapsed('sidebar-width', false, 'ws-b');

    expect(localStorage.getItem(panelCollapsedStorageKey('sidebar-width', 'ws-a'))).toBe('true');
    expect(loadPanelCollapsed('sidebar-width', false, 'ws-a')).toBe(true);
    expect(loadPanelCollapsed('sidebar-width', false, 'ws-b')).toBe(false);
  });

  it('computes keyboard resize deltas for horizontal, vertical, and inverted right-side panels', () => {
    expect(computeKeyboardResizeDelta('ArrowRight', 'horizontal')).toBe(16);
    expect(computeKeyboardResizeDelta('ArrowLeft', 'horizontal')).toBe(-16);
    expect(computeKeyboardResizeDelta('ArrowRight', 'horizontal', true)).toBe(48);
    // Right-hand panel (inverted): ArrowLeft expands, ArrowRight shrinks
    expect(computeKeyboardResizeDelta('ArrowLeft', 'horizontal', false, true)).toBe(16);
    expect(computeKeyboardResizeDelta('ArrowRight', 'horizontal', false, true)).toBe(-16);
    // Vertical panel (editor height): ArrowDown expands, ArrowUp shrinks
    expect(computeKeyboardResizeDelta('ArrowDown', 'vertical')).toBe(16);
    expect(computeKeyboardResizeDelta('ArrowUp', 'vertical', true)).toBe(-48);
    expect(computeKeyboardResizeDelta('Home', 'horizontal')).toBe('min');
    expect(computeKeyboardResizeDelta('End', 'vertical')).toBe('max');
    expect(computeKeyboardResizeDelta('Tab', 'horizontal')).toBeNull();
  });
});
