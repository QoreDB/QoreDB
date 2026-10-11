// SPDX-License-Identifier: BUSL-1.1

import { useState } from 'react';
import { createRoot } from 'react-dom/client';
import '../../../src/i18n';
import { ConnectionContextMenu } from '../../../src/components/Connection/ConnectionContextMenu';
import { ConnectionMenu } from '../../../src/components/Connection/ConnectionMenu';
import { useConnectionActions } from '../../../src/components/Connection/useConnectionActions';
import { useColumnMasking } from '../../../src/components/Grid/hooks/useColumnMasking';
import { AgentsSection } from '../../../src/components/Settings/sections/AgentsSection';
import { TooltipProvider } from '../../../src/components/ui/tooltip';
import { useSavedConnections } from '../../../src/hooks/useSavedConnections';
import {
  setActiveWorkspace,
  setWorkspaceLoading,
  useWorkspaceStore,
} from '../../../src/lib/stores/workspaceStore';

function connection(project) {
  return {
    id: 'same-id',
    name: `Connection ${project}`,
    driver: 'postgres',
    host: 'localhost',
    port: 5432,
    username: 'fixture',
    environment: 'production',
    read_only: true,
    ssl: false,
    project_id: 'default',
    expose_to_agents: false,
    masking: {
      rules: [{ table: 'users', column: 'secret', mode: 'hidden' }],
      mask_detected_columns: false,
    },
  };
}
window.__workspace = project => {
  window.__project = project;
  window.__connections = [connection(project)];
  setActiveWorkspace(null, project);
};
window.__loading = setWorkspaceLoading;
window.__workspace('a');
const edited = (_connection, password) => window.__events.push({ type: 'edit', password });
const changed = () => window.__events.push({ type: 'changed' });
const after = () => window.__events.push({ type: 'after' });
function Probe() {
  useWorkspaceStore(state => state);
  const item = window.__connections[0];
  window.__list = useSavedConnections();
  const [table, setTable] = useState('users');
  window.__table = setTable;
  window.__actions = useConnectionActions({
    connection: item,
    onEdit: edited,
    onDeleted: changed,
    onAfterAction: after,
  });
  const { columnMask, removalDialog } = useColumnMasking({
    connectionId: item.id,
    tableName: table,
    environment: 'production',
    confirmationLabel: 'SYNTHETIC',
    maskedColumns: new Set(['secret']),
    onChanged: changed,
  });
  window.__mask = columnMask;
  return (
    <TooltipProvider>
      <div data-testid="menu">
        <ConnectionMenu connection={item} onEdit={edited} onDeleted={changed} />
      </div>
      <ConnectionContextMenu connection={item} onEdit={edited} onDeleted={changed}>
        <div data-testid="context">Context target</div>
      </ConnectionContextMenu>
      {removalDialog}
      <AgentsSection />
    </TooltipProvider>
  );
}
function Fixture() {
  const [mounted, setMounted] = useState(true);
  window.__unmount = () => setMounted(false);
  return mounted ? <Probe /> : <p>Closed</p>;
}
createRoot(document.getElementById('root')).render(<Fixture />);
