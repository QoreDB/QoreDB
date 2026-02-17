// SPDX-License-Identifier: Apache-2.0

import { useState } from 'react';
import { motion, AnimatePresence } from 'framer-motion';
import { useTranslation } from 'react-i18next';
import { Button } from '@/components/ui/button';
import { Checkbox } from '@/components/ui/checkbox';
import { AnalyticsService } from './AnalyticsService';
import { ArrowLeft, Database, Laptop, ShieldCheck } from 'lucide-react';

interface OnboardingModalProps {
  onComplete: () => void;
}

export function OnboardingModal({ onComplete }: OnboardingModalProps) {
  const { t } = useTranslation();
  const [step, setStep] = useState(0);
  const [analyticsEnabled, setAnalyticsEnabled] = useState(false);
  const [isExiting, setIsExiting] = useState(false);

  const totalSteps = 3;

  const handlePrev = () => {
    if (step > 0) {
      setStep(prev => prev - 1);
    }
  };

  const handleNext = () => {
    if (step < totalSteps - 1) {
      setStep(prev => prev + 1);
    } else {
      AnalyticsService.setAnalyticsEnabled(analyticsEnabled);
      AnalyticsService.completeOnboarding();
      setIsExiting(true);
    }
  };

  const variants = {
    enter: (direction: number) => ({
      x: direction > 0 ? 100 : -100,
      opacity: 0,
    }),
    center: {
      zIndex: 1,
      x: 0,
      opacity: 1,
    },
    exit: (direction: number) => ({
      zIndex: 0,
      x: direction < 0 ? 100 : -100,
      opacity: 0,
    }),
  };

  return (
    <motion.div
      className="fixed inset-0 z-50 flex items-center justify-center bg-background/95 backdrop-blur-sm p-4"
      initial={{ opacity: 0 }}
      animate={isExiting ? { opacity: 0 } : { opacity: 1 }}
      onAnimationComplete={() => {
        if (isExiting) onComplete();
      }}
    >
      <motion.div
        className="relative w-full max-w-2xl bg-card border rounded-xl shadow-2xl p-8 overflow-hidden h-125"
        initial={{ scale: 0.9, opacity: 0 }}
        animate={isExiting ? { scale: 1.05, opacity: 0 } : { scale: 1, opacity: 1 }}
        transition={{ duration: 0.3 }}
      >
        {/* Progress Bar */}
        <div className="absolute top-0 left-0 right-0 h-1 bg-muted">
          <motion.div
            className="h-full bg-primary"
            initial={{ width: '0%' }}
            animate={{ width: `${((step + 1) / totalSteps) * 100}%` }}
            transition={{ duration: 0.5 }}
          />
        </div>

        <AnimatePresence mode="wait" custom={step}>
          {step === 0 && (
            <motion.div
              key="step0"
              variants={variants}
              initial="enter"
              animate="center"
              exit="exit"
              custom={step}
              className="flex flex-col items-center justify-center h-full text-center space-y-6"
            >
              <motion.div
                initial={{ scale: 0.8 }}
                animate={{ scale: 1 }}
                transition={{ type: 'spring', stiffness: 200, damping: 20 }}
                className="p-6 bg-primary/10 rounded-full mb-4"
              >
                <img src="/logo.png" alt="QoreDB" width={80} height={80} />
              </motion.div>
              <h1 className="text-4xl font-bold bg-linear-to-r from-primary to-accent bg-clip-text text-transparent">
                {t('onboarding.welcome.title')}
              </h1>
              <p className="text-xl text-muted-foreground max-w-md">
                {t('onboarding.welcome.subtitle')}
              </p>
            </motion.div>
          )}

          {step === 1 && (
            <motion.div
              key="step1"
              variants={variants}
              initial="enter"
              animate="center"
              exit="exit"
              custom={step}
              className="flex flex-col h-full pt-8 space-y-8"
            >
              <div className="text-center">
                <h2 className="text-3xl font-bold mb-2">{t('onboarding.concepts.title')}</h2>
              </div>

              <div className="grid grid-cols-2 gap-6 mt-4">
                <div className="p-6 border rounded-lg bg-card/50 hover:bg-muted/50 transition-colors">
                  <Database className="w-10 h-10 text-primary mb-4" />
                  <h3 className="text-xl font-semibold mb-2">
                    {t('onboarding.concepts.universalTitle')}
                  </h3>
                  <p className="text-muted-foreground text-sm">
                    {t('onboarding.concepts.universalDesc')}
                  </p>
                </div>
                <div className="p-6 border rounded-lg bg-card/50 hover:bg-muted/50 transition-colors">
                  <Laptop className="w-10 h-10 text-primary mb-4" />
                  <h3 className="text-xl font-semibold mb-2">
                    {t('onboarding.concepts.localTitle')}
                  </h3>
                  <p className="text-muted-foreground text-sm">
                    {t('onboarding.concepts.localDesc')}
                  </p>
                </div>
              </div>
            </motion.div>
          )}

          {step === 2 && (
            <motion.div
              key="step2"
              variants={variants}
              initial="enter"
              animate="center"
              exit="exit"
              custom={step}
              className="flex flex-col items-center justify-center h-full text-center space-y-6 pb-12"
            >
              <div className="p-4 bg-muted/30 rounded-full">
                <ShieldCheck className="w-16 h-16 text-primary" />
              </div>
              <h2 className="text-3xl font-bold">{t('onboarding.analytics.title')}</h2>
              <p className="text-muted-foreground max-w-lg">{t('onboarding.analytics.subtitle')}</p>

              <div
                className="flex items-start space-x-3 bg-muted/20 p-4 rounded-lg border text-left max-w-md w-full cursor-pointer hover:bg-muted/30 transition-colors"
                onClick={() => setAnalyticsEnabled(!analyticsEnabled)}
              >
                <Checkbox
                  id="analytics"
                  checked={analyticsEnabled}
                  onCheckedChange={c => setAnalyticsEnabled(c === true)}
                  className="mt-1"
                />
                <div className="grid gap-1.5 leading-none select-none">
                  <label
                    htmlFor="analytics"
                    className="text-sm font-medium leading-none cursor-pointer"
                  >
                    {t('onboarding.analytics.checkbox')}
                  </label>
                  <p className="text-sm text-muted-foreground leading-relaxed">
                    {t('onboarding.analytics.privacyHint')}
                  </p>
                </div>
              </div>
            </motion.div>
          )}
        </AnimatePresence>

        <div className="absolute bottom-8 right-8 left-8 flex justify-between items-center">
          <div className="flex space-x-2">
            {[0, 1, 2].map(i => (
              <div
                key={i}
                className={`h-2 w-2 rounded-full transition-colors ${i === step ? 'bg-primary' : 'bg-muted-foreground/30'}`}
              />
            ))}
          </div>

          <Button onClick={handleNext} className="">
            {step === 0 && t('onboarding.welcome.next')}
            {step === 1 && t('onboarding.concepts.next')}
            {step === 2 && t('onboarding.analytics.finish')}
          </Button>
        </div>

        {step > 0 && (
          <button
            onClick={handlePrev}
            className="absolute top-8 left-8 text-muted-foreground hover:text-foreground transition-colors flex items-center"
          >
            <ArrowLeft className="w-4 h-4 mr-2" />
            {t('common.back', 'Back')}
          </button>
        )}
      </motion.div>
    </motion.div>
  );
}
