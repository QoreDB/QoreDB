// SPDX-License-Identifier: Apache-2.0

import { useCallback, useEffect, useMemo, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { Database, Loader2, Plus, Terminal } from 'lucide-react';
import { Button } from '@/components/ui/button';
import { CreateDatabaseModal } from '@/components/Tree/CreateDatabaseModal';
import { useSchemaCache } from '@/hooks/useSchemaCache';
import { getDriverMetadata, Driver } from '@/lib/drivers';
import type { Namespace, SavedConnection } from '@/lib/tauri';
import { ENVIRONMENT_CONFIG } from '@/lib/environment';

interface ConnectionDashboardProps {
  sessionId: string;
  driver: Driver;
  connection: SavedConnection;
  schemaRefreshTrigger?: number;
  onOpenQuery: () => void;
  onOpenDatabase: (namespace: Namespace) => void;
  onSchemaChange?: () => void;
}

export function ConnectionDashboard({
  sessionId,
  driver,
  connection,
  schemaRefreshTrigger,
  onOpenQuery,
  onOpenDatabase,
  onSchemaChange,
}: ConnectionDashboardProps) {
  const { t } = useTranslation();
  const driverMeta = getDriverMetadata(driver);
  const { getNamespaces, invalidateNamespaces, loading } = useSchemaCache(sessionId);
  const [namespaces, setNamespaces] = useState<Namespace[]>([]);
  const [createOpen, setCreateOpen] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const descriptionKey = driverMeta.supportsSchemas
    ? 'connectionDashboard.descriptionSchema'
    : 'connectionDashboard.descriptionDatabase';
  const emptyKey = driverMeta.supportsSchemas
    ? 'connectionDashboard.emptySchemas'
    : 'connectionDashboard.emptyDatabases';

  const loadNamespaces = useCallback(
    async (force = false) => {
      if (force) {
        invalidateNamespaces();
      }
      setError(null);
      try {
        const data = await getNamespaces();
        setNamespaces(data);
      } catch (err) {
        const message = err instanceof Error ? err.message : t('common.unknownError');
        setError(message);
      }
    },
    [getNamespaces, invalidateNamespaces, t]
  );

  useEffect(() => {
    void loadNamespaces();
  }, [loadNamespaces]);

  useEffect(() => {
    if (schemaRefreshTrigger === undefined) return;
    void loadNamespaces(true);
  }, [schemaRefreshTrigger, loadNamespaces]);

  const sortedNamespaces = useMemo(() => {
    return [...namespaces].sort((a, b) =>
      `${a.database}.${a.schema || ''}`.localeCompare(`${b.database}.${b.schema || ''}`)
    );
  }, [namespaces]);

  const titleLabel = connection?.name || t('connectionDashboard.unknownConnection');
  const createLabelKey =
    driverMeta.createAction === 'schema' ? 'database.newSchema' : 'database.newDatabase';

  return (
    <div className="h-full flex flex-col gap-6">
      <div className="flex flex-col gap-4 sm:flex-row sm:items-start sm:justify-between">
        <div className="space-y-1">
          <div className="flex items-center gap-3">
            <h2 className="text-xl font-semibold text-foreground">
              {t('connectionDashboard.title', { name: titleLabel })}
            </h2>
            {connection?.environment &&
              connection.environment !== 'development' &&
              (() => {
                const config = ENVIRONMENT_CONFIG[connection.environment];
                return (
                  <span
                    className="px-2 py-0.5 text-xs font-bold rounded"
                    style={{ backgroundColor: config.bgSoft, color: config.color }}
                  >
                    {config.labelShort}
                  </span>
                );
              })()}
          </div>
          <p className="text-sm text-muted-foreground">{t(descriptionKey)}</p>
        </div>
        <div className="flex flex-wrap items-center gap-2">
          <Button variant="outline" onClick={onOpenQuery}>
            <Terminal size={14} className="mr-2" />
            {t('connectionDashboard.openQuery')}
          </Button>
          {driverMeta.createAction !== 'none' && (
            <Button
              onClick={() => setCreateOpen(true)}
              disabled={connection.read_only}
              title={connection.read_only ? t('environment.blocked') : undefined}
            >
              <Plus size={14} className="mr-2" />
              {t(createLabelKey)}
            </Button>
          )}
        </div>
      </div>

      <div className="border border-border rounded-lg bg-muted/20 overflow-hidden">
        <div className="px-4 py-2 border-b border-border flex items-center justify-between">
          <span className="text-xs font-semibold text-muted-foreground uppercase tracking-wider">
            {t(driverMeta.treeRootLabel)}
          </span>
          {loading && (
            <span className="flex items-center gap-2 text-xs text-muted-foreground">
              <Loader2 size={12} className="animate-spin" />
              {t('common.loading')}
            </span>
          )}
        </div>

        {error && (
          <div className="px-4 py-3 text-sm text-error border-b border-border bg-error/5">
            {error}
          </div>
        )}

        {sortedNamespaces.length === 0 && !loading ? (
          <div className="px-4 py-8 flex flex-col items-center justify-center text-center gap-4">
            <p className="text-sm text-muted-foreground">{t(emptyKey)}</p>
            <div className="flex gap-2">
              <Button variant="outline" size="sm" onClick={onOpenQuery}>
                <Terminal size={14} className="mr-2" />
                {t('connectionDashboard.openQuery')}
              </Button>
              {driverMeta.createAction !== 'none' && !connection.read_only && (
                <Button size="sm" onClick={() => setCreateOpen(true)}>
                  <Plus size={14} className="mr-2" />
                  {t(createLabelKey)}
                </Button>
              )}
            </div>
          </div>
        ) : (
          <div className="divide-y divide-border">
            {sortedNamespaces.map(ns => {
              const label = ns.schema ? `${ns.database}.${ns.schema}` : ns.database;
              return (
                <button
                  key={`${ns.database}:${ns.schema || ''}`}
                  type="button"
                  className="w-full flex items-center justify-between px-4 py-2 text-left hover:bg-muted/50 transition-colors"
                  onClick={() => onOpenDatabase(ns)}
                >
                  <span className="flex items-center gap-2 min-w-0">
                    <Database size={14} className="shrink-0 text-muted-foreground" />
                    <span className="truncate font-mono text-sm">{label}</span>
                  </span>
                  <span className="text-xs text-muted-foreground">{t('dbtree.open')}</span>
                </button>
              );
            })}
          </div>
        )}
      </div>

      {driverMeta.createAction !== 'none' && (
        <CreateDatabaseModal
          isOpen={createOpen}
          onClose={() => setCreateOpen(false)}
          sessionId={sessionId}
          driver={driver}
          environment={connection.environment || 'development'}
          readOnly={connection.read_only || false}
          connectionName={connection.name}
          connectionDatabase={connection.database}
          onCreated={() => {
            onSchemaChange?.();
            void loadNamespaces(true);
          }}
        />
      )}
    </div>
  );
}
