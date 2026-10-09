// SPDX-License-Identifier: Apache-2.0

import { useState } from 'react';
import { createRoot } from 'react-dom/client';
import '../../../src/i18n';
import { ConnectionModal } from '../../../src/components/Connection/ConnectionModal';
import { ProjectTransferCard } from '../../../src/components/Settings/ProjectTransferCard';
import { TooltipProvider } from '../../../src/components/ui/tooltip';
import { useOpenNotebook } from '../../../src/hooks/useOpenNotebook';
import { consumePendingNotebook } from '../../../src/lib/notebook/notebookIO';
import { resolveConfirm, useConfirmState } from '../../../src/lib/stores/confirmStore';
import {
  setActiveWorkspace,
  setWorkspaceLoading,
  useWorkspaceStore,
} from '../../../src/lib/stores/workspaceStore';

setActiveWorkspace(null, 'a');
const openTab = tab => window.__opened.push(tab);
function Probe() {
  const [session, setSession] = useState('session-a');
  const [connection, setConnection] = useState(null);
  window.__connection = setConnection;
  const projectId = useWorkspaceStore(state => state.projectId);
  window.__openNotebook = useOpenNotebook(session, openTab);
  window.__session = setSession;
  window.__workspace = id => setActiveWorkspace(null, id);
  window.__loading = setWorkspaceLoading;
  window.__confirm = useConfirmState();
  window.__resolveConfirm = resolveConfirm;
  window.__consume = consumePendingNotebook;
  return (
    <TooltipProvider>
      <ProjectTransferCard projectId={projectId} />
      <ConnectionModal
        isOpen={connection !== null}
        onClose={() => setConnection(null)}
        editConnection={
          connection?.edit
            ? {
                id: 'c',
                name: 'Synthetic',
                driver: 'postgres',
                host: 'localhost',
                port: 5432,
                username: 'user',
                environment: 'development',
                read_only: true,
                ssl: false,
                project_id: projectId,
              }
            : undefined
        }
        onSaved={value => {
          window.__saved = value;
        }}
        onConnected={(sessionId, value) => {
          window.__connected = { sessionId, value };
        }}
      />
    </TooltipProvider>
  );
}
function Fixture() {
  const [mounted, setMounted] = useState(true);
  window.__unmount = () => setMounted(false);
  return mounted ? <Probe /> : <p>Closed</p>;
}
createRoot(document.getElementById('root')).render(<Fixture />);
