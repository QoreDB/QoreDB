// SPDX-License-Identifier: BUSL-1.1

import { createRoot } from 'react-dom/client';
import { ReplayIndicator } from '/src/components/Replay/ReplayIndicator';
import { setActiveWorkspace } from '/src/lib/stores/workspaceStore';
import '/src/i18n';

window.__switchWorkspace = project => {
  window.__backendProject = project;
  setActiveWorkspace(null, project);
};
createRoot(document.getElementById('root')).render(<ReplayIndicator />);
