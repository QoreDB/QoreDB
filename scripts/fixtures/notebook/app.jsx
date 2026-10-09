// SPDX-License-Identifier: BUSL-1.1

import { useState } from 'react';
import { createRoot } from 'react-dom/client';
import '/src/i18n';
import { useNotebook } from '/src/hooks/useNotebook';
import { resolveConfirm, useConfirmState } from '/src/lib/stores/confirmStore';
import { setActiveWorkspace } from '/src/lib/stores/workspaceStore';

const initialNotebook = {
  version: 1,
  metadata: { id: 'fixture', title: 'Synthetic notebook', createdAt: '', updatedAt: '' },
  variables: { value: { name: 'value', type: 'number', defaultValue: '1' } },
  cells: [
    { id: 'a', type: 'sql', source: 'SELECT $value', config: { label: 'a' } },
    { id: 'b', type: 'sql', source: 'SELECT $a.id', config: { label: 'b' } },
    { id: 'c', type: 'sql', source: 'SELECT $b.id', config: { label: 'c' } },
  ],
};
function Notebook() {
  window.__confirm = useConfirmState();
  window.__resolveConfirm = resolveConfirm;
  const [context, setContext] = useState({
    sessionId: 'session-a',
    namespace: { database: 'fixture' },
  });
  window.__context = setContext;
  window.__workspace = projectId => setActiveWorkspace(null, projectId);
  const notebook = useNotebook({
    tabId: 'fixture',
    initialNotebook: new URLSearchParams(location.search).has('draft')
      ? undefined
      : initialNotebook,
    ...context,
  });
  window.__notebook = notebook;
  return <output>{JSON.stringify(notebook.notebook)}</output>;
}
function Fixture() {
  const [mounted, setMounted] = useState(true);
  window.__unmount = () => setMounted(false);
  return mounted ? <Notebook /> : <p>Closed</p>;
}
createRoot(document.getElementById('root')).render(<Fixture />);
