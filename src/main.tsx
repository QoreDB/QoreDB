// SPDX-License-Identifier: Apache-2.0

import React from 'react';
import ReactDOM from 'react-dom/client';
import './i18n'; // Import i18n configuration
import App from './App';
import './index.css';
import { ErrorBoundary } from '@/components/ErrorBoundary';
import { TooltipProvider } from '@/components/ui/tooltip';
import { AnalyticsService } from './components/Onboarding/AnalyticsService';

AnalyticsService.init();

ReactDOM.createRoot(document.getElementById('root') as HTMLElement).render(
  <React.StrictMode>
    <TooltipProvider>
      <ErrorBoundary>
        <App />
      </ErrorBoundary>
    </TooltipProvider>
  </React.StrictMode>
);
