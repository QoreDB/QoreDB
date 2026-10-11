// SPDX-License-Identifier: Apache-2.0

import { useCallback, useEffect, useState } from 'react';
import { captureWorkspaceScope, useWorkspaceStore } from '../lib/stores/workspaceStore';
import { listSavedConnections, type SavedConnection } from '../lib/tauri';

const EMPTY: SavedConnection[] = [];
type Status = 'loading' | 'ready' | 'error';

/** Never expose a previous activation's list while the next workspace is loading. */
export function useSavedConnections(refreshKey = 0, enabled = true) {
  const workspace = useWorkspaceStore(state => state);
  const [revision, setRevision] = useState(0);
  const [snapshot, setSnapshot] = useState<{
    isCurrent: () => boolean;
    connections: SavedConnection[];
    status: Status;
  } | null>(null);
  const refresh = useCallback(() => setRevision(value => value + 1), []);

  // biome-ignore lint/correctness/useExhaustiveDependencies: Both counters explicitly request a fresh read.
  useEffect(() => {
    if (!enabled || workspace.isLoading) return;
    const inWorkspace = captureWorkspaceScope(workspace.projectId);
    let active = true;
    const isCurrent = () => active && inWorkspace();
    setSnapshot(previous => ({
      isCurrent: inWorkspace,
      connections: previous?.isCurrent() ? previous.connections : EMPTY,
      status: 'loading',
    }));
    listSavedConnections(workspace.projectId).then(
      connections => {
        if (isCurrent()) setSnapshot({ isCurrent: inWorkspace, connections, status: 'ready' });
      },
      () => {
        if (isCurrent())
          setSnapshot({ isCurrent: inWorkspace, connections: EMPTY, status: 'error' });
      }
    );
    return () => {
      active = false;
    };
  }, [workspace, enabled, revision, refreshKey]);

  const current = enabled && snapshot?.isCurrent() ? snapshot : null;
  return {
    connections: current?.connections ?? EMPTY,
    status: current?.status ?? 'loading',
    refresh,
  };
}
