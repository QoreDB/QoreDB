// SPDX-License-Identifier: Apache-2.0

import type { LucideIcon } from 'lucide-react';
import {
  ArrowLeft,
  Box,
  CheckCircle2,
  Database,
  GitBranch,
  KeyRound,
  Layers,
  Lock,
  ShieldCheck,
  Sparkles,
  X,
  Zap,
} from 'lucide-react';
import { useEffect, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { Button } from '@/components/ui/button';
import { Checkbox } from '@/components/ui/checkbox';
import { AnalyticsService } from './AnalyticsService';

const ACCENT = 'var(--color-accent)';
const ACCENT_BG_SUBTLE = 'color-mix(in srgb, var(--color-accent) 8%, transparent)';
const ACCENT_BG_SOFT = 'color-mix(in srgb, var(--color-accent) 10%, transparent)';
const ACCENT_BORDER = 'color-mix(in srgb, var(--color-accent) 25%, transparent)';
const ACCENT_GRADIENT =
  'linear-gradient(180deg, color-mix(in srgb, var(--color-accent) 4%, transparent) 0%, transparent 100%)';
const TOTAL_STEPS = 5;
const EXIT_DURATION_MS = 160;

interface OnboardingModalProps {
  onComplete: () => void;
}

interface CapabilityCard {
  icon: LucideIcon;
  titleKey: string;
  descKey: string;
}

const CAPABILITIES: CapabilityCard[] = [
  {
    icon: Database,
    titleKey: 'onboarding.capabilities.driversTitle',
    descKey: 'onboarding.capabilities.driversDesc',
  },
  {
    icon: Sparkles,
    titleKey: 'onboarding.capabilities.aiTitle',
    descKey: 'onboarding.capabilities.aiDesc',
  },
  {
    icon: Layers,
    titleKey: 'onboarding.capabilities.notebooksTitle',
    descKey: 'onboarding.capabilities.notebooksDesc',
  },
  {
    icon: GitBranch,
    titleKey: 'onboarding.capabilities.federationTitle',
    descKey: 'onboarding.capabilities.federationDesc',
  },
];

interface SafetyPoint {
  icon: LucideIcon;
  titleKey: string;
  descKey: string;
}

const SAFETY_POINTS: SafetyPoint[] = [
  {
    icon: KeyRound,
    titleKey: 'onboarding.privacy.vaultTitle',
    descKey: 'onboarding.privacy.vaultDesc',
  },
  {
    icon: ShieldCheck,
    titleKey: 'onboarding.privacy.guardsTitle',
    descKey: 'onboarding.privacy.guardsDesc',
  },
  {
    icon: Box,
    titleKey: 'onboarding.privacy.sandboxTitle',
    descKey: 'onboarding.privacy.sandboxDesc',
  },
  {
    icon: Lock,
    titleKey: 'onboarding.privacy.localTitle',
    descKey: 'onboarding.privacy.localDesc',
  },
];

export function OnboardingModal({ onComplete }: OnboardingModalProps) {
  const { t } = useTranslation();
  const [step, setStep] = useState(0);
  const [analyticsEnabled, setAnalyticsEnabled] = useState(false);
  const [isExiting, setIsExiting] = useState(false);

  useEffect(() => {
    if (!isExiting) return;
    const timer = window.setTimeout(onComplete, EXIT_DURATION_MS);
    return () => window.clearTimeout(timer);
  }, [isExiting, onComplete]);

  const handlePrev = () => {
    if (step > 0) setStep(prev => prev - 1);
  };

  const finish = () => {
    AnalyticsService.setAnalyticsEnabled(analyticsEnabled);
    AnalyticsService.completeOnboarding();
    setIsExiting(true);
  };

  const handleNext = () => {
    if (step < TOTAL_STEPS - 1) {
      setStep(prev => prev + 1);
    } else {
      finish();
    }
  };

  const handleSkip = () => {
    AnalyticsService.completeOnboarding();
    setIsExiting(true);
  };

  return (
    <div
      className={`fixed inset-0 z-50 flex items-center justify-center bg-background/95 p-4 backdrop-blur-sm transition-opacity duration-200 ${
        isExiting ? 'opacity-0' : 'opacity-100'
      }`}
      role="dialog"
      aria-modal="true"
      aria-labelledby="onboarding-title"
    >
      <div
        className={`relative w-full max-w-2xl overflow-hidden rounded-xl border bg-card shadow-2xl transition-transform duration-200 ${
          isExiting ? 'scale-[0.98]' : 'scale-100'
        }`}
      >
        <div className="h-1 w-full bg-muted">
          <div
            className="h-full transition-[width] duration-300 ease-out"
            style={{
              background: ACCENT,
              width: `${((step + 1) / TOTAL_STEPS) * 100}%`,
            }}
          />
        </div>

        {step < TOTAL_STEPS - 1 && (
          <button
            type="button"
            onClick={handleSkip}
            className="absolute right-4 top-5 z-10 inline-flex items-center gap-1 text-xs text-muted-foreground transition-colors hover:text-foreground"
            aria-label={t('onboarding.skip', 'Skip')}
          >
            {t('onboarding.skip', 'Skip')}
            <X size={12} />
          </button>
        )}

        <div
          // biome-ignore lint/suspicious/noArrayIndexKey: step index drives the fade-in remount
          key={step}
          className="relative min-h-[380px] animate-in fade-in p-8 duration-200"
        >
          {step === 0 && (
            <div className="flex min-h-[380px] flex-col items-center justify-center gap-4 text-center">
              <div
                className="flex h-20 w-20 items-center justify-center rounded-2xl"
                style={{ background: ACCENT_BG_SUBTLE }}
              >
                <img src="/logo.png" alt="QoreDB" width={56} height={56} />
              </div>
              <h1 id="onboarding-title" className="text-3xl font-semibold tracking-tight">
                {t('onboarding.welcome.title')}
              </h1>
              <p className="max-w-md text-base text-muted-foreground">
                {t('onboarding.welcome.subtitle')}
              </p>
              <div className="mt-2 flex items-center gap-1.5 text-xs text-muted-foreground/70">
                <Zap size={12} style={{ color: ACCENT }} aria-hidden />
                <span>
                  {t(
                    'onboarding.welcome.tagline',
                    '12 drivers · SQL + NoSQL · Rust-fast · Local-first'
                  )}
                </span>
              </div>
            </div>
          )}

          {step === 1 && (
            <div className="flex flex-col gap-5">
              <div className="flex flex-col gap-1">
                <h2 className="text-xl font-semibold">{t('onboarding.capabilities.title')}</h2>
                <p className="text-sm text-muted-foreground">
                  {t('onboarding.capabilities.subtitle')}
                </p>
              </div>
              <div className="grid grid-cols-2 gap-3">
                {CAPABILITIES.map(({ icon: Icon, titleKey, descKey }) => (
                  <div
                    key={titleKey}
                    className="flex flex-col gap-2 rounded-lg border p-4 transition-colors hover:bg-muted/40"
                  >
                    <div
                      className="flex h-9 w-9 items-center justify-center rounded-md"
                      style={{ background: ACCENT_BG_SOFT, color: ACCENT }}
                    >
                      <Icon size={18} aria-hidden />
                    </div>
                    <h3 className="text-sm font-semibold">{t(titleKey)}</h3>
                    <p className="text-xs leading-relaxed text-muted-foreground">{t(descKey)}</p>
                  </div>
                ))}
              </div>
            </div>
          )}

          {step === 2 && (
            <div className="flex flex-col gap-4">
              <div className="flex flex-col gap-1">
                <h2 className="text-xl font-semibold">{t('onboarding.privacy.title')}</h2>
                <p className="text-sm text-muted-foreground">{t('onboarding.privacy.subtitle')}</p>
              </div>
              <ul className="flex flex-col gap-3">
                {SAFETY_POINTS.map(({ icon: Icon, titleKey, descKey }) => (
                  <li key={titleKey} className="flex items-start gap-3">
                    <div
                      className="flex h-8 w-8 shrink-0 items-center justify-center rounded-md"
                      style={{ background: 'rgba(16, 185, 129, 0.1)', color: '#10B981' }}
                    >
                      <Icon size={15} aria-hidden />
                    </div>
                    <div className="flex flex-col gap-0.5">
                      <span className="text-sm font-medium">{t(titleKey)}</span>
                      <span className="text-xs text-muted-foreground">{t(descKey)}</span>
                    </div>
                  </li>
                ))}
              </ul>
            </div>
          )}

          {step === 3 && (
            <div className="flex flex-col gap-4">
              <div className="flex flex-col gap-1">
                <h2 className="text-xl font-semibold">{t('onboarding.tiers.title')}</h2>
                <p className="text-sm text-muted-foreground">{t('onboarding.tiers.subtitle')}</p>
              </div>
              <div className="grid grid-cols-2 gap-3">
                <div className="flex flex-col gap-3 rounded-lg border p-4">
                  <span className="text-xs font-semibold uppercase tracking-wider text-muted-foreground">
                    {t('onboarding.tiers.freeLabel', 'Free')}
                  </span>
                  <span className="text-sm font-medium">{t('onboarding.tiers.freeFor')}</span>
                  <ul className="flex flex-col gap-1.5 text-xs text-muted-foreground">
                    <li className="flex gap-2">
                      <CheckCircle2
                        size={12}
                        className="mt-0.5 shrink-0 text-green-500"
                        aria-hidden
                      />
                      {t('onboarding.tiers.freeBullet1')}
                    </li>
                    <li className="flex gap-2">
                      <CheckCircle2
                        size={12}
                        className="mt-0.5 shrink-0 text-green-500"
                        aria-hidden
                      />
                      {t('onboarding.tiers.freeBullet2')}
                    </li>
                    <li className="flex gap-2">
                      <CheckCircle2
                        size={12}
                        className="mt-0.5 shrink-0 text-green-500"
                        aria-hidden
                      />
                      {t('onboarding.tiers.freeBullet3')}
                    </li>
                  </ul>
                </div>
                <div
                  className="flex flex-col gap-3 rounded-lg border p-4"
                  style={{
                    borderColor: ACCENT_BORDER,
                    background: ACCENT_GRADIENT,
                  }}
                >
                  <span
                    className="text-xs font-semibold uppercase tracking-wider"
                    style={{ color: ACCENT }}
                  >
                    {t('onboarding.tiers.proLabel', 'Pro')}
                  </span>
                  <span className="text-sm font-medium">{t('onboarding.tiers.proFor')}</span>
                  <ul className="flex flex-col gap-1.5 text-xs text-muted-foreground">
                    <li className="flex gap-2">
                      <CheckCircle2
                        size={12}
                        className="mt-0.5 shrink-0"
                        style={{ color: ACCENT }}
                        aria-hidden
                      />
                      {t('onboarding.tiers.proBullet1')}
                    </li>
                    <li className="flex gap-2">
                      <CheckCircle2
                        size={12}
                        className="mt-0.5 shrink-0"
                        style={{ color: ACCENT }}
                        aria-hidden
                      />
                      {t('onboarding.tiers.proBullet2')}
                    </li>
                    <li className="flex gap-2">
                      <CheckCircle2
                        size={12}
                        className="mt-0.5 shrink-0"
                        style={{ color: ACCENT }}
                        aria-hidden
                      />
                      {t('onboarding.tiers.proBullet3')}
                    </li>
                  </ul>
                </div>
              </div>
              <p className="text-center text-xs text-muted-foreground/70">
                {t('onboarding.tiers.footnote')}
              </p>
            </div>
          )}

          {step === 4 && (
            <div className="flex flex-col gap-4">
              <div className="flex flex-col gap-1">
                <h2 className="text-xl font-semibold">{t('onboarding.analytics.title')}</h2>
                <p className="text-sm text-muted-foreground">
                  {t('onboarding.analytics.subtitle')}
                </p>
              </div>
              <button
                type="button"
                className="flex w-full items-start gap-3 rounded-lg border p-4 text-left transition-colors hover:bg-muted/40"
                onClick={() => setAnalyticsEnabled(!analyticsEnabled)}
              >
                <Checkbox
                  id="analytics"
                  checked={analyticsEnabled}
                  onCheckedChange={c => setAnalyticsEnabled(c === true)}
                  className="mt-0.5"
                />
                <div className="flex flex-col gap-1">
                  <span className="text-sm font-medium leading-tight">
                    {t('onboarding.analytics.checkbox')}
                  </span>
                  <span className="text-xs leading-relaxed text-muted-foreground">
                    {t('onboarding.analytics.privacyHint')}
                  </span>
                </div>
              </button>
              <div className="flex items-center gap-2 rounded-md border border-dashed bg-muted/20 p-3">
                <Lock size={14} className="shrink-0 text-muted-foreground" aria-hidden />
                <span className="text-xs text-muted-foreground">
                  {t(
                    'onboarding.analytics.guarantee',
                    'Your queries, results, and credentials never leave your machine — regardless of this setting.'
                  )}
                </span>
              </div>
            </div>
          )}
        </div>

        <div className="flex items-center justify-between border-t bg-muted/20 px-8 py-4">
          {step > 0 ? (
            <button
              type="button"
              onClick={handlePrev}
              className="inline-flex items-center gap-1.5 text-xs text-muted-foreground transition-colors hover:text-foreground"
            >
              <ArrowLeft size={12} aria-hidden />
              {t('common.back', 'Back')}
            </button>
          ) : (
            <span />
          )}

          <div className="flex items-center gap-1.5">
            {Array.from({ length: TOTAL_STEPS }).map((_, i) => (
              <div
                // biome-ignore lint/suspicious/noArrayIndexKey: static dots indexed by step
                key={i}
                className="h-1.5 w-1.5 rounded-full transition-colors"
                style={{
                  background: i === step ? ACCENT : 'var(--color-border)',
                }}
              />
            ))}
          </div>

          <Button onClick={handleNext} size="sm" style={{ background: ACCENT, color: 'white' }}>
            {step === 0
              ? t('onboarding.welcome.next')
              : step === TOTAL_STEPS - 1
                ? t('onboarding.analytics.finish')
                : t('common.next', 'Next')}
          </Button>
        </div>
      </div>
    </div>
  );
}
