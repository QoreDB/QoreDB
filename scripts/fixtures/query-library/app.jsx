// SPDX-License-Identifier: Apache-2.0

import { useEffect } from 'react';
import { createRoot } from 'react-dom/client';
import '../../../src/i18n';
import * as library from '../../../src/lib/query/queryLibrary';
import { useWorkspace, WorkspaceProvider } from '../../../src/providers/WorkspaceProvider';

function Probe() {
  const workspace = useWorkspace();
  useEffect(() => {
    window.__workspace = workspace;
    window.__library = library;
  }, [workspace]);
  return <p>{workspace.projectId}</p>;
}

createRoot(document.getElementById('root')).render(
  <WorkspaceProvider>
    <Probe />
  </WorkspaceProvider>
);
