// SPDX-License-Identifier: BUSL-1.1

//! Structural schema diff between two connections (e.g. Prod↔Staging). Opens a
//! dedicated session per connection, captures both schemas, compares them by
//! schema-qualified table name (database-agnostic), and always releases the
//! sessions it opened on unmount.

import { AlertTriangle, ArrowLeftRight, Loader2 } from 'lucide-react';
import { useEffect, useState } from 'react';
import { useTranslation } from 'react-i18next';
import type { Driver } from '@/lib/connection/drivers';
import type { SchemaDelta } from '@/lib/migrations/schemaCompare';
import { compareSnapshots, stripDatabaseKey } from '@/lib/migrations/schemaCompare';
import { captureSnapshot } from '@/lib/migrations/schemaDiff';
import {
  connectSavedConnection,
  disconnect,
  listSavedConnections,
  type Namespace,
  type SavedConnection,
} from '@/lib/tauri';
import { useWorkspace } from '@/providers/WorkspaceProvider';
import { SchemaDeltaView } from './SchemaDeltaView';

interface SchemaDiffViewerProps {
  leftConnectionId: string;
  rightConnectionId: string;
  namespace?: Namespace;
}

type ViewerState =
  | { status: 'loading' }
  | { status: 'error'; code: 'missing' | 'mismatch' | 'connect'; detail?: string }
  | {
      status: 'ready';
      delta: SchemaDelta;
      leftLabel: string;
      rightLabel: string;
      incomplete: boolean;
    };

export function SchemaDiffViewer({
  leftConnectionId,
  rightConnectionId,
  namespace,
}: SchemaDiffViewerProps) {
  const { t } = useTranslation();
  const { projectId } = useWorkspace();
  const [state, setState] = useState<ViewerState>({ status: 'loading' });

  const anchorDatabase = namespace?.database;

  useEffect(() => {
    let cancelled = false;
    const openedSessions: string[] = [];

    const openSession = async (conn: SavedConnection): Promise<string> => {
      const res = await connectSavedConnection(projectId, conn.id);
      if (!res.success || !res.session_id) {
        throw new Error(res.error || conn.name);
      }
      openedSessions.push(res.session_id);
      return res.session_id;
    };

    (async () => {
      setState({ status: 'loading' });
      try {
        const connections = await listSavedConnections(projectId);
        const left = connections.find(c => c.id === leftConnectionId);
        const right = connections.find(c => c.id === rightConnectionId);
        if (!left || !right) {
          if (!cancelled) setState({ status: 'error', code: 'missing' });
          return;
        }
        if (left.driver !== right.driver) {
          if (!cancelled) setState({ status: 'error', code: 'mismatch' });
          return;
        }

        let leftSession: string;
        let rightSession: string;
        try {
          leftSession = await openSession(left);
          rightSession = await openSession(right);
        } catch (err) {
          if (!cancelled) {
            setState({
              status: 'error',
              code: 'connect',
              detail: err instanceof Error ? err.message : String(err),
            });
          }
          return;
        }
        if (cancelled) return;

        const [leftCap, rightCap] = await Promise.all([
          captureSnapshot(leftSession, left.driver as Driver, anchorDatabase ?? left.database),
          captureSnapshot(rightSession, right.driver as Driver, right.database),
        ]);
        if (cancelled) return;

        // In the database-agnostic keyspace, exclude tables that failed to describe
        // on either side so they aren't reported as one-sided additions/removals.
        const ignoreKeys = new Set(
          [...leftCap.failedTables, ...rightCap.failedTables].map(stripDatabaseKey)
        );
        const delta = compareSnapshots(leftCap.snapshot, rightCap.snapshot, {
          ignoreDatabase: true,
          ignoreKeys,
        });
        setState({
          status: 'ready',
          delta,
          leftLabel: left.name,
          rightLabel: right.name,
          incomplete: leftCap.failedTables.length > 0 || rightCap.failedTables.length > 0,
        });
      } catch (err) {
        if (!cancelled) {
          setState({
            status: 'error',
            code: 'connect',
            detail: err instanceof Error ? err.message : String(err),
          });
        }
      }
    })();

    return () => {
      cancelled = true;
      for (const sid of openedSessions) {
        disconnect(sid).catch(err => console.warn('Failed to release schema-diff session:', err));
      }
    };
  }, [leftConnectionId, rightConnectionId, projectId, anchorDatabase]);

  if (state.status === 'loading') {
    return (
      <div className="flex-1 min-h-0 flex flex-col items-center justify-center gap-2 text-sm text-muted-foreground">
        <Loader2 className="w-6 h-6 animate-spin" />
        {t('schemaDiff.loading')}
      </div>
    );
  }

  if (state.status === 'error') {
    const message =
      state.code === 'missing'
        ? t('schemaDiff.connectionMissing')
        : state.code === 'mismatch'
          ? t('schemaDiff.driverMismatch')
          : t('schemaDiff.connectFailed', { detail: state.detail ?? '' });
    return (
      <div className="flex-1 min-h-0 flex flex-col items-center justify-center gap-2 p-8 text-center">
        <AlertTriangle className="w-8 h-8 text-amber-500" />
        <p className="max-w-md text-sm text-muted-foreground">{message}</p>
      </div>
    );
  }

  return (
    <div className="flex-1 min-h-0 flex flex-col">
      <div className="flex items-center gap-2 px-4 py-3 border-b border-border">
        <span className="font-mono text-sm truncate">{state.leftLabel}</span>
        <ArrowLeftRight className="w-4 h-4 shrink-0 text-muted-foreground" />
        <span className="font-mono text-sm truncate">{state.rightLabel}</span>
      </div>
      {state.incomplete && (
        <div className="flex items-center gap-2 px-4 py-2 text-xs bg-amber-500/10 text-amber-700 dark:text-amber-400 border-b border-border">
          <AlertTriangle className="w-3.5 h-3.5 shrink-0" />
          {t('schemaDiff.incomplete')}
        </div>
      )}
      <div className="flex-1 min-h-0 overflow-y-auto p-4">
        <SchemaDeltaView delta={state.delta} emptyMessage={t('schemaDiff.identical')} />
      </div>
    </div>
  );
}
