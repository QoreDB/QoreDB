// SPDX-License-Identifier: BUSL-1.1
import React from 'react';
import { createRoot } from 'react-dom/client';
import '/src/index.css';
import '/src/i18n';
import { TimeTravelSettingsCard } from '/src/components/Settings/sections/DataSection';

createRoot(document.getElementById('root')).render(
  <main className="mx-auto max-w-xl p-6">
    <TimeTravelSettingsCard />
  </main>
);
