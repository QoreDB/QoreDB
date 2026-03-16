// SPDX-License-Identifier: Apache-2.0

import { useCallback, useEffect, useState } from 'react';
import { createPortal } from 'react-dom';
import { useTranslation } from 'react-i18next';
import type { TourStepDef } from './tourDefinitions';
import { TourTooltip } from './TourTooltip';

interface FeatureTourProps {
  steps: TourStepDef[];
  onComplete: () => void;
  onDismiss: () => void;
}

interface Rect {
  top: number;
  left: number;
  width: number;
  height: number;
}

const PADDING = 8;

export function FeatureTour({ steps, onComplete, onDismiss }: FeatureTourProps) {
  const { t } = useTranslation();
  const [stepIndex, setStepIndex] = useState(0);
  const [targetRect, setTargetRect] = useState<Rect | null>(null);

  const currentStep = steps[stepIndex];

  // Measure target element position
  useEffect(() => {
    if (!currentStep) return;

    const measure = () => {
      const el = document.querySelector(currentStep.targetSelector);
      if (el) {
        const rect = el.getBoundingClientRect();
        setTargetRect({
          top: rect.top,
          left: rect.left,
          width: rect.width,
          height: rect.height,
        });
      } else {
        setTargetRect(null);
      }
    };

    // Initial measure with delay for DOM to settle
    const timer = setTimeout(measure, 100);

    // Re-measure on resize/scroll
    window.addEventListener('resize', measure);
    window.addEventListener('scroll', measure, true);

    return () => {
      clearTimeout(timer);
      window.removeEventListener('resize', measure);
      window.removeEventListener('scroll', measure, true);
    };
  }, [currentStep]);

  const handleNext = useCallback(() => {
    if (stepIndex >= steps.length - 1) {
      onComplete();
    } else {
      setStepIndex(i => i + 1);
    }
  }, [stepIndex, steps.length, onComplete]);

  // Escape key to dismiss
  useEffect(() => {
    const handler = (e: KeyboardEvent) => {
      if (e.key === 'Escape') onDismiss();
    };
    window.addEventListener('keydown', handler);
    return () => window.removeEventListener('keydown', handler);
  }, [onDismiss]);

  if (!currentStep || !targetRect) return null;

  // Calculate tooltip position
  const tooltipStyle = getTooltipPosition(targetRect, currentStep.position);

  // Create a spotlight overlay using box-shadow
  const spotlightStyle: React.CSSProperties = {
    position: 'fixed',
    top: targetRect.top - PADDING,
    left: targetRect.left - PADDING,
    width: targetRect.width + PADDING * 2,
    height: targetRect.height + PADDING * 2,
    borderRadius: '8px',
    boxShadow: '0 0 0 9999px rgba(0, 0, 0, 0.5)',
    zIndex: 9998,
    pointerEvents: 'none',
  };

  return createPortal(
    <>
      {/* Click overlay to prevent interaction outside spotlight */}
      {/* biome-ignore lint/a11y/noStaticElementInteractions: overlay dismiss pattern */}
      {/* biome-ignore lint/a11y/useKeyWithClickEvents: keyboard handled via useEffect */}
      <div className="fixed inset-0 z-[9997]" onClick={onDismiss} />

      {/* Spotlight cutout */}
      <div style={spotlightStyle} />

      {/* Tooltip */}
      <div className="fixed z-[9999]" style={tooltipStyle}>
        <TourTooltip
          title={t(currentStep.titleKey)}
          description={t(currentStep.descriptionKey)}
          stepIndex={stepIndex}
          totalSteps={steps.length}
          onNext={handleNext}
          onDismiss={onDismiss}
        />
      </div>
    </>,
    document.body
  );
}

function getTooltipPosition(rect: Rect, position: TourStepDef['position']): React.CSSProperties {
  const GAP = 12;
  const TOOLTIP_WIDTH = 288; // w-72 = 18rem = 288px

  switch (position) {
    case 'bottom':
      return {
        top: rect.top + rect.height + PADDING + GAP,
        left: Math.max(8, rect.left + rect.width / 2 - TOOLTIP_WIDTH / 2),
      };
    case 'top':
      return {
        bottom: window.innerHeight - rect.top + PADDING + GAP,
        left: Math.max(8, rect.left + rect.width / 2 - TOOLTIP_WIDTH / 2),
      };
    case 'right':
      return {
        top: rect.top + rect.height / 2 - 60,
        left: rect.left + rect.width + PADDING + GAP,
      };
    case 'left':
      return {
        top: rect.top + rect.height / 2 - 60,
        right: window.innerWidth - rect.left + PADDING + GAP,
      };
  }
}
