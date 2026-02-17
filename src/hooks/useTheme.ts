// SPDX-License-Identifier: Apache-2.0

import { useEffect, useMemo, useState, useCallback } from 'react';

export type ThemePreference = 'light' | 'dark' | 'auto';
export type ResolvedTheme = 'light' | 'dark';

const STORAGE_KEY = 'qoredb-theme';
const THEME_EVENT = 'qoredb:theme-changed';

function isThemePreference(value: unknown): value is ThemePreference {
  return value === 'light' || value === 'dark' || value === 'auto';
}

export function useTheme() {
  const [theme, setThemeState] = useState<ThemePreference>(() => {
    const stored = localStorage.getItem(STORAGE_KEY);
    if (isThemePreference(stored)) return stored;
    return 'auto';
  });

  const [systemTheme, setSystemTheme] = useState<ResolvedTheme>(() => {
    return window.matchMedia('(prefers-color-scheme: dark)').matches ? 'dark' : 'light';
  });

  useEffect(() => {
    const media = window.matchMedia('(prefers-color-scheme: dark)');

    const update = () => {
      setSystemTheme(media.matches ? 'dark' : 'light');
    };

    update();

    if (typeof media.addEventListener === 'function') {
      media.addEventListener('change', update);
      return () => media.removeEventListener('change', update);
    }

    // Safari < 14
    media.addListener(update);
    return () => media.removeListener(update);
  }, []);

  useEffect(() => {
    const onThemeChanged = (event: Event) => {
      const next = (event as CustomEvent<{ theme?: unknown }>).detail?.theme;
      if (!isThemePreference(next)) return;
      setThemeState(prev => (prev === next ? prev : next));
    };

    const onStorage = (event: StorageEvent) => {
      if (event.key !== STORAGE_KEY) return;
      const next = event.newValue;
      if (!isThemePreference(next)) return;
      setThemeState(prev => (prev === next ? prev : next));
    };

    window.addEventListener(THEME_EVENT, onThemeChanged);
    window.addEventListener('storage', onStorage);
    return () => {
      window.removeEventListener(THEME_EVENT, onThemeChanged);
      window.removeEventListener('storage', onStorage);
    };
  }, []);

  const resolvedTheme = useMemo<ResolvedTheme>(() => {
    return theme === 'auto' ? systemTheme : theme;
  }, [theme, systemTheme]);

  useEffect(() => {
    const root = document.documentElement;

    root.setAttribute('data-theme', resolvedTheme);
    if (resolvedTheme === 'dark') {
      root.classList.add('dark');
    } else {
      root.classList.remove('dark');
    }

    localStorage.setItem(STORAGE_KEY, theme);
  }, [resolvedTheme, theme]);

  const setTheme = useCallback(
    (next: ThemePreference | ((prev: ThemePreference) => ThemePreference)) => {
      setThemeState(prev => {
        const computed = typeof next === 'function' ? next(prev) : next;
        if (computed === prev) return prev;

        localStorage.setItem(STORAGE_KEY, computed);
        window.dispatchEvent(new CustomEvent(THEME_EVENT, { detail: { theme: computed } }));
        return computed;
      });
    },
    []
  );

  const toggleTheme = useCallback(() => {
    setTheme(prev => {
      const active: ResolvedTheme = prev === 'auto' ? systemTheme : prev;
      return active === 'light' ? 'dark' : 'light';
    });
  }, [setTheme, systemTheme]);

  const isDark = resolvedTheme === 'dark';

  return { theme, resolvedTheme, systemTheme, setTheme, toggleTheme, isDark };
}
