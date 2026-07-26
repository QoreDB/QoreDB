// SPDX-License-Identifier: Apache-2.0

import React from 'react';
import ReactDOM from 'react-dom/client';
import './i18n';
import App from './App';
import './index.css';
import { ErrorBoundary } from '@/components/ErrorBoundary';
import { TooltipProvider } from '@/components/ui/tooltip';
import { logger } from './lib/diagnostics/logger';
import { purgeLegacyTelemetryState } from './lib/legacyTelemetryPurge';

purgeLegacyTelemetryState();

window.addEventListener('error', event => {
  const error = event.error instanceof Error ? event.error : new Error(event.message);
  logger.error(`Unhandled renderer error: ${error.message}`, error);
});

window.addEventListener('unhandledrejection', event => {
  const error = event.reason instanceof Error ? event.reason : new Error(String(event.reason));
  logger.error(`Unhandled promise rejection: ${error.message}`, error);
});

ReactDOM.createRoot(document.getElementById('root') as HTMLElement).render(
  <React.StrictMode>
    <TooltipProvider>
      <ErrorBoundary>
        <App />
      </ErrorBoundary>
    </TooltipProvider>
  </React.StrictMode>
);
