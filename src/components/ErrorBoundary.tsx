// SPDX-License-Identifier: Apache-2.0

import type { ReactNode } from 'react';
import { type FallbackProps, ErrorBoundary as ReactErrorBoundary } from 'react-error-boundary';
import { useTranslation } from 'react-i18next';
import { logger } from '@/lib/logger';

function ErrorFallback({ error, resetErrorBoundary }: FallbackProps) {
  const { t } = useTranslation();

  const errorMessage = error instanceof Error ? error.message : String(error);

  return (
    <div className="flex flex-col items-center justify-center min-h-screen bg-background text-foreground p-4">
      <div className="max-w-md w-full bg-card border rounded-lg shadow-lg p-6">
        <h2 className="text-xl font-semibold text-destructive mb-2">{t('errorBoundary.title')}</h2>
        <p className="text-muted-foreground mb-4">{t('errorBoundary.description')}</p>

        <div className="bg-muted p-2 rounded text-xs overflow-auto max-h-32 mb-4 font-mono">
          {errorMessage}
        </div>

        <button
          onClick={() => {
            resetErrorBoundary();
            window.location.reload();
          }}
          className="px-4 py-2 bg-primary text-primary-foreground rounded hover:bg-primary/90 transition-colors"
        >
          {t('errorBoundary.reload')}
        </button>
      </div>
    </div>
  );
}

interface Props {
  children: ReactNode;
}

export function ErrorBoundary({ children }: Props) {
  const handleError = (error: unknown, info: { componentStack?: string | null }) => {
    const err = error instanceof Error ? error : new Error(String(error));
    logger.error(
      `Uncaught error: ${err.message}\nComponent Stack: ${info.componentStack ?? 'unknown'}`,
      err
    );
  };

  return (
    <ReactErrorBoundary FallbackComponent={ErrorFallback} onError={handleError}>
      {children}
    </ReactErrorBoundary>
  );
}
