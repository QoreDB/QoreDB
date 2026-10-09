// SPDX-License-Identifier: BUSL-1.1

import { useState } from 'react';
import { setActiveWorkspace } from '/src/lib/stores/workspaceStore';
import { createRoot } from 'react-dom/client';
import '/src/i18n';
import { useReplay } from '/src/hooks/useReplay';

function Fixture() {
  const [session, setSession] = useState('synthetic-session');
  window.__switchContext = (nextSession, project) => {
    setSession(nextSession);
    if (project) setActiveWorkspace(null, project);
  };
  const replay = useReplay(session);
  window.__replay = replay;
  return <pre id="state">{JSON.stringify({ slug: replay.activeSlug, report: replay.report, runs: replay.runs })}</pre>;
}
createRoot(document.getElementById('root')).render(<Fixture />);
