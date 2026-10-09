// SPDX-License-Identifier: Apache-2.0

import { createRoot } from 'react-dom/client';
import '/src/i18n';
import '/src/index.css';
import { AppOverlays } from '/src/components/AppOverlays';
import { TooltipProvider } from '/src/components/ui/tooltip';
import { setLibraryModalOpen } from '/src/lib/stores/modalStore';

const noop = () => {};
window.__libraryOpen = setLibraryModalOpen;
createRoot(document.getElementById('root')).render(
  <TooltipProvider>
    <AppOverlays
      onConnected={noop}
      onConnectionSaved={noop}
      onSearchSelect={noop}
      onSelectLibraryQuery={noop}
      onNavigateToTable={noop}
      paletteCommands={[]}
      paletteFeatures={[]}
      sessionId={null}
    />
  </TooltipProvider>
);
