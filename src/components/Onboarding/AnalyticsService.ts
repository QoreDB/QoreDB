// SPDX-License-Identifier: Apache-2.0

import posthog, { type Properties } from 'posthog-js';

const ONBOARDING_KEY = 'qoredb_onboarding_completed';
const ANALYTICS_KEY = 'qoredb_analytics_enabled';

const DEFAULT_POSTHOG_HOST = 'https://eu.i.posthog.com';

const DAILY_EVENT_PREFIX = 'qoredb_daily_event::';

function isEnabledFromStorage(): boolean {
  return localStorage.getItem(ANALYTICS_KEY) === 'true';
}

function getPosthogKey(): string | null {
  const key = (import.meta.env.VITE_POSTHOG_KEY ?? '').trim();
  return key ? key : null;
}

function getPosthogHost(): string {
  const host = (import.meta.env.VITE_POSTHOG_HOST ?? '').trim();
  return host ? host : DEFAULT_POSTHOG_HOST;
}

function shouldLoadSdk(): boolean {
  if (import.meta.env.PROD) return true;
  return (import.meta.env.VITE_POSTHOG_ENABLE_IN_DEV ?? '').trim() === 'true';
}

function todayKey(): string {
  return new Date().toISOString().slice(0, 10);
}

function hasFiredToday(event: string): boolean {
  try {
    return localStorage.getItem(`${DAILY_EVENT_PREFIX}${event}`) === todayKey();
  } catch {
    return false;
  }
}

function markFiredToday(event: string): void {
  try {
    localStorage.setItem(`${DAILY_EVENT_PREFIX}${event}`, todayKey());
  } catch {
    // ignore
  }
}

let sdkInitialized = false;

function ensureSdkInitialized(): boolean {
  if (sdkInitialized) return true;
  if (!shouldLoadSdk()) return false;

  const key = getPosthogKey();
  if (!key) return false;

  posthog.init(key, {
    api_host: getPosthogHost(),
    persistence: 'localStorage',
    autocapture: false,
    capture_pageview: false,
    capture_pageleave: false,
    disable_session_recording: true,
  });

  sdkInitialized = true;
  return true;
}

function captureAppOpenedOncePerDay(): void {
  if (!isEnabledFromStorage()) return;
  if (!ensureSdkInitialized()) return;
  const event = 'app_opened';
  if (hasFiredToday(event)) return;
  markFiredToday(event);
  posthog.capture(event);
}

export const AnalyticsService = {
  init: () => {
    captureAppOpenedOncePerDay();
  },

  capture: (event: string, properties?: Properties) => {
    if (!isEnabledFromStorage()) return;
    if (!ensureSdkInitialized()) return;
    posthog.capture(event, properties);
  },

  captureOncePerDay: (event: string, properties?: Properties) => {
    if (!isEnabledFromStorage()) return;
    if (!ensureSdkInitialized()) return;
    if (hasFiredToday(event)) return;
    markFiredToday(event);
    posthog.capture(event, properties);
  },

  isOnboardingCompleted: (): boolean => {
    return localStorage.getItem(ONBOARDING_KEY) === 'true';
  },

  completeOnboarding: () => {
    localStorage.setItem(ONBOARDING_KEY, 'true');
    AnalyticsService.capture('onboarding_completed');
  },

  isAnalyticsEnabled: (): boolean => {
    return isEnabledFromStorage();
  },

  setAnalyticsEnabled: (enabled: boolean) => {
    localStorage.setItem(ANALYTICS_KEY, String(enabled));
    if (enabled) {
      if (!ensureSdkInitialized()) return;
      posthog.opt_in_capturing();
      AnalyticsService.capture('analytics_opt_in');
    } else {
      if (sdkInitialized) {
        posthog.opt_out_capturing();
      }
      AnalyticsService.resetIdentity();
    }
  },

  resetIdentity: () => {
    if (sdkInitialized) {
      posthog.reset(true);
    }
  },

  reset: () => {
    localStorage.removeItem(ONBOARDING_KEY);
    localStorage.removeItem(ANALYTICS_KEY);
    AnalyticsService.resetIdentity();
  },
};
