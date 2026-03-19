// SPDX-License-Identifier: Apache-2.0

import { useCallback, useSyncExternalStore } from 'react';
import type { TourStepDef } from '@/components/Tour/tourDefinitions';
import { TOURS } from '@/components/Tour/tourDefinitions';

const STORAGE_KEY = 'qoredb_completed_tours';

let activeTourId: string | null = null;
let listeners: Array<() => void> = [];

function emitChange() {
  for (const listener of listeners) listener();
}

function getCompletedTours(): string[] {
  try {
    return JSON.parse(localStorage.getItem(STORAGE_KEY) || '[]');
  } catch {
    return [];
  }
}

function getSnapshot() {
  return { activeTourId, completed: getCompletedTours() };
}

let cachedSnapshot = getSnapshot();

function subscribe(listener: () => void) {
  listeners.push(listener);
  return () => {
    listeners = listeners.filter(l => l !== listener);
  };
}

export function useTourManager() {
  const snapshot = useSyncExternalStore(subscribe, () => {
    const next = getSnapshot();
    if (
      next.activeTourId !== cachedSnapshot.activeTourId ||
      next.completed.length !== cachedSnapshot.completed.length
    ) {
      cachedSnapshot = next;
    }
    return cachedSnapshot;
  });

  const shouldShowTour = useCallback(
    (tourId: string) => {
      return !snapshot.completed.includes(tourId) && snapshot.activeTourId === null;
    },
    [snapshot]
  );

  const startTour = useCallback((tourId: string) => {
    activeTourId = tourId;
    emitChange();
  }, []);

  const completeTour = useCallback((tourId: string) => {
    const completed = getCompletedTours();
    if (!completed.includes(tourId)) {
      localStorage.setItem(STORAGE_KEY, JSON.stringify([...completed, tourId]));
    }
    activeTourId = null;
    emitChange();
  }, []);

  const dismissTour = useCallback(() => {
    if (activeTourId) {
      const completed = getCompletedTours();
      if (!completed.includes(activeTourId)) {
        localStorage.setItem(STORAGE_KEY, JSON.stringify([...completed, activeTourId]));
      }
    }
    activeTourId = null;
    emitChange();
  }, []);

  const resetAllTours = useCallback(() => {
    localStorage.removeItem(STORAGE_KEY);
    activeTourId = null;
    emitChange();
  }, []);

  const activeTour = snapshot.activeTourId;
  const activeTourSteps: TourStepDef[] | null = activeTour ? (TOURS[activeTour] ?? null) : null;

  return {
    shouldShowTour,
    startTour,
    completeTour,
    dismissTour,
    resetAllTours,
    activeTour,
    activeTourSteps,
  };
}
