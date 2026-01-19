
const ONBOARDING_KEY = 'qoredb_onboarding_completed';
const ANALYTICS_KEY = 'qoredb_analytics_enabled';

export const AnalyticsService = {
  isOnboardingCompleted: (): boolean => {
    return localStorage.getItem(ONBOARDING_KEY) === 'true';
  },

  completeOnboarding: () => {
    localStorage.setItem(ONBOARDING_KEY, 'true');
  },

  isAnalyticsEnabled: (): boolean => {
    return localStorage.getItem(ANALYTICS_KEY) === 'true';
  },

  setAnalyticsEnabled: (enabled: boolean) => {
    localStorage.setItem(ANALYTICS_KEY, String(enabled));
  },

  reset: () => {
    localStorage.removeItem(ONBOARDING_KEY);
    localStorage.removeItem(ANALYTICS_KEY);
  }
};
