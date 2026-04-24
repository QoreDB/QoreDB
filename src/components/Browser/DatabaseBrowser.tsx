// SPDX-License-Identifier: Apache-2.0

import {
  AlertCircle,
  Calendar,
  ChevronLeft,
  ChevronRight,
  Code2,
  Database,
  Download,
  Eye,
  FileCode,
  FunctionSquare,
  HardDrive,
  Hash,
  List,
  Loader2,
  type LucideIcon,
  PlayCircle,
  Plus,
  Search,
  Shield,
  ShieldAlert,
  Table,
  TerminalSquare,
  X,
  Zap,
} from 'lucide-react';
import { type ReactNode, useCallback, useEffect, useMemo, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { LuaScriptModal } from '@/components/Editor/LuaScriptModal';
import {
  RedisEditorModal,
  type RedisEditorMode,
} from '@/components/Editor/RedisEditorModal';
import { ERDiagram } from '@/components/Schema/ERDiagram';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import {
  type DriverSchemaObjectCapabilities,
  type DriverTerminology,
  getSchemaObjectCapabilities,
  getTerminology,
} from '@/lib/driverCapabilities';
import { emitTableChange, onTableChange } from '@/lib/tableEvents';
import { getNamespaceTableVisits, type TableVisitInsight } from '@/lib/tableInsights';
import { cn } from '@/lib/utils';
import {
  DRIVER_ICONS,
  DRIVER_LABELS,
  Driver,
  type DriverMetadata,
  getDriverMetadata,
} from '../../lib/drivers';
import {
  type Collection,
  type DatabaseEvent,
  type Environment,
  executeQuery,
  listCollections,
  listEvents,
  listRoutines,
  listSequences,
  listTriggers,
  type Namespace,
  type RelationFilter,
  type Routine,
  type Sequence,
  type Trigger,
} from '../../lib/tauri';
import { SchemaExportDialog } from '../Export/SchemaExportDialog';
import { CreateTableModal } from '../Table/CreateTableModal';
import { EventContextMenu } from '../Tree/EventContextMenu';
import { RoutineContextMenu } from '../Tree/RoutineContextMenu';
import { SequenceContextMenu } from '../Tree/SequenceContextMenu';
import { TriggerContextMenu } from '../Tree/TriggerContextMenu';
import { ContentBreadcrumb } from './ContentBreadcrumb';
import { StatCard } from './StatCard';

const OVERVIEW_COLLECTION_LIMIT = 10;
const TABLES_PAGE_SIZE = 20;
const TAB_BUTTON_CLASS_NAME = 'px-3 py-1.5 text-sm font-medium rounded-md transition-colors';
const LIST_SURFACE_CLASS_NAME =
  'border border-border rounded-md divide-y divide-border flex-1 overflow-auto relative min-h-50';
const SECTION_HEADER_CLASS_NAME = 'p-3 bg-muted/30';

export type DatabaseBrowserTab =
  | 'overview'
  | 'tables'
  | 'routines'
  | 'triggers'
  | 'sequences'
  | 'schema';

interface DatabaseBrowserProps {
  sessionId: string;
  namespace: Namespace;
  driver: Driver;
  environment?: Environment;
  readOnly?: boolean;
  connectionName?: string;
  connectionId?: string;
  onTableSelect: (namespace: Namespace, tableName: string, relationFilter?: RelationFilter) => void;
  schemaRefreshTrigger?: number;
  onSchemaChange?: () => void;
  onOpenQueryTab?: (namespace: Namespace) => void;
  onOpenFulltextSearch?: () => void;
  onOpenRoutineSource?: (routine: Routine, namespace: Namespace) => void;
  onCreateRoutine?: (routineType: 'Function' | 'Procedure', namespace: Namespace) => void;
  onOpenTriggerSource?: (trigger: Trigger, namespace: Namespace) => void;
  onCreateTrigger?: (namespace: Namespace) => void;
  onOpenEventSource?: (event: DatabaseEvent, namespace: Namespace) => void;
  onCreateEvent?: (namespace: Namespace) => void;
  onOpenSequenceSource?: (sequence: Sequence, namespace: Namespace) => void;
  onClose: () => void;
  initialTab?: DatabaseBrowserTab;
  onActiveTabChange?: (tab: DatabaseBrowserTab) => void;
}

interface DatabaseStats {
  sizeFormatted?: string;
  tableCount: number;
  indexCount?: number;
}

interface OverviewPreviewItem {
  name: string;
  collectionType?: Collection['collection_type'];
  visitCount?: number;
  lastVisitedAt?: number;
  personalized: boolean;
}

interface BrowserTabDefinition {
  id: DatabaseBrowserTab;
  icon: LucideIcon;
  label: string;
}

interface UseDatabaseBrowserDataArgs {
  activeTab: DatabaseBrowserTab;
  driverMeta: DriverMetadata;
  namespace: Namespace;
  page: number;
  schemaObjectCapabilities: DriverSchemaObjectCapabilities;
  search: string;
  sessionId: string;
}

const EMPTY_DATABASE_STATS: DatabaseStats = {
  tableCount: 0,
};

function formatBytes(bytes: number): string {
  if (!bytes || bytes < 1024) return `${bytes || 0} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  if (bytes < 1024 * 1024 * 1024) return `${(bytes / 1024 / 1024).toFixed(1)} MB`;
  return `${(bytes / 1024 / 1024 / 1024).toFixed(2)} GB`;
}

function parseFloatValue(value: unknown): number | undefined {
  if (typeof value === 'number' && Number.isFinite(value)) {
    return value;
  }

  const parsed = Number.parseFloat(String(value));
  return Number.isFinite(parsed) ? parsed : undefined;
}

function parseIntegerValue(value: unknown): number | undefined {
  const parsed = parseFloatValue(value);
  return parsed === undefined ? undefined : Math.trunc(parsed);
}

function isSameNamespace(a: Namespace, b: Namespace): boolean {
  return a.database === b.database && (a.schema ?? '') === (b.schema ?? '');
}

function isViewCollection(collectionType?: Collection['collection_type']): boolean {
  return collectionType === 'View';
}

function getCollectionFetchOptions(activeTab: DatabaseBrowserTab, page: number, search: string) {
  const isOverview = activeTab === 'overview';

  return {
    isOverview,
    page: isOverview ? 1 : page,
    limit: isOverview ? OVERVIEW_COLLECTION_LIMIT : TABLES_PAGE_SIZE,
    search: isOverview ? undefined : search || undefined,
  };
}

function buildOverviewPreviewItems(
  collections: Collection[],
  tableVisits: TableVisitInsight[]
): OverviewPreviewItem[] {
  if (tableVisits.length === 0) {
    return collections.slice(0, OVERVIEW_COLLECTION_LIMIT).map(collection => ({
      name: collection.name,
      collectionType: collection.collection_type,
      personalized: false,
    }));
  }

  const collectionTypesByName = new Map(
    collections.map(collection => [collection.name, collection.collection_type] as const)
  );

  return tableVisits.map(visit => ({
    name: visit.tableName,
    collectionType: collectionTypesByName.get(visit.tableName),
    visitCount: visit.visitCount,
    lastVisitedAt: visit.lastVisitedAt,
    personalized: true,
  }));
}

async function executeSingleValueQuery(sessionId: string, query: string): Promise<unknown> {
  const result = await executeQuery(sessionId, query);
  return result.success ? result.result?.rows[0]?.values[0] : undefined;
}

async function getQueryValue(
  sessionId: string,
  query: string,
  label: string
): Promise<unknown | undefined> {
  try {
    return await executeSingleValueQuery(sessionId, query);
  } catch (err) {
    console.error(`[DatabaseBrowser] ${label} query error:`, err);
    return undefined;
  }
}

async function loadDatabaseStats({
  driverMeta,
  includeSqlStats,
  namespace,
  sessionId,
  tableCount,
}: {
  driverMeta: DriverMetadata;
  includeSqlStats: boolean;
  namespace: Namespace;
  sessionId: string;
  tableCount: number;
}): Promise<DatabaseStats> {
  const nextStats: DatabaseStats = {
    tableCount,
  };

  if (!includeSqlStats) {
    return nextStats;
  }

  const schemaOrDatabase = namespace.schema || namespace.database;
  const { databaseSizeQuery, indexCountQuery } = driverMeta.queries;

  const [sizeValue, indexCountValue] = await Promise.all([
    databaseSizeQuery
      ? getQueryValue(sessionId, databaseSizeQuery(schemaOrDatabase), 'Size')
      : Promise.resolve(undefined),
    indexCountQuery
      ? getQueryValue(sessionId, indexCountQuery(schemaOrDatabase), 'Index count')
      : Promise.resolve(undefined),
  ]);

  if (typeof sizeValue === 'string') {
    nextStats.sizeFormatted = sizeValue;
  } else {
    const sizeBytes = parseFloatValue(sizeValue);
    if (sizeBytes !== undefined && sizeBytes > 0) {
      nextStats.sizeFormatted = formatBytes(sizeBytes);
    }
  }

  const parsedIndexCount = parseIntegerValue(indexCountValue);
  if (parsedIndexCount !== undefined) {
    nextStats.indexCount = parsedIndexCount;
  }

  return nextStats;
}

function useDatabaseBrowserData({
  activeTab,
  driverMeta,
  namespace,
  page,
  schemaObjectCapabilities,
  search,
  sessionId,
}: UseDatabaseBrowserDataArgs) {
  const [stats, setStats] = useState<DatabaseStats>(EMPTY_DATABASE_STATS);
  const [collections, setCollections] = useState<Collection[]>([]);
  const [routines, setRoutines] = useState<Routine[]>([]);
  const [routinesLoading, setRoutinesLoading] = useState(false);
  const [triggers, setTriggers] = useState<Trigger[]>([]);
  const [triggersLoading, setTriggersLoading] = useState(false);
  const [dbEvents, setDbEvents] = useState<DatabaseEvent[]>([]);
  const [sequences, setSequences] = useState<Sequence[]>([]);
  const [sequencesLoading, setSequencesLoading] = useState(false);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [totalCount, setTotalCount] = useState(0);

  const loadRoutines = useCallback(async () => {
    if (!schemaObjectCapabilities.routines) {
      setRoutines([]);
      setRoutinesLoading(false);
      return;
    }

    setRoutinesLoading(true);
    try {
      const result = await listRoutines(sessionId, namespace);
      if (result.success && result.data) {
        setRoutines(result.data.routines);
      }
    } catch (err) {
      console.error('Failed to load routines:', err);
    } finally {
      setRoutinesLoading(false);
    }
  }, [namespace, schemaObjectCapabilities.routines, sessionId]);

  const loadTriggers = useCallback(async () => {
    if (!schemaObjectCapabilities.triggers && !schemaObjectCapabilities.events) {
      setTriggers([]);
      setDbEvents([]);
      setTriggersLoading(false);
      return;
    }

    setTriggersLoading(true);
    try {
      const [triggerResult, eventsResult] = await Promise.all([
        schemaObjectCapabilities.triggers
          ? listTriggers(sessionId, namespace)
          : Promise.resolve(null),
        schemaObjectCapabilities.events ? listEvents(sessionId, namespace) : Promise.resolve(null),
      ]);

      if (!schemaObjectCapabilities.triggers) {
        setTriggers([]);
      } else if (triggerResult?.success && triggerResult.data) {
        setTriggers(triggerResult.data.triggers);
      }

      if (!schemaObjectCapabilities.events) {
        setDbEvents([]);
      } else if (eventsResult?.success && eventsResult.data) {
        setDbEvents(eventsResult.data.events);
      }
    } catch (err) {
      console.error('Failed to load triggers/events:', err);
    } finally {
      setTriggersLoading(false);
    }
  }, [namespace, schemaObjectCapabilities.events, schemaObjectCapabilities.triggers, sessionId]);

  const loadSequences = useCallback(async () => {
    if (!schemaObjectCapabilities.sequences) {
      setSequences([]);
      setSequencesLoading(false);
      return;
    }

    setSequencesLoading(true);
    try {
      const result = await listSequences(sessionId, namespace);
      if (result.success && result.data) {
        setSequences(result.data.sequences);
      }
    } catch (err) {
      console.error('Failed to load sequences:', err);
    } finally {
      setSequencesLoading(false);
    }
  }, [namespace, schemaObjectCapabilities.sequences, sessionId]);

  const loadCollectionsAndStats = useCallback(async () => {
    setLoading(true);
    setError(null);

    try {
      const fetchOptions = getCollectionFetchOptions(activeTab, page, search);
      const collectionsResult = await listCollections(
        sessionId,
        namespace,
        fetchOptions.search,
        fetchOptions.page,
        fetchOptions.limit
      );

      if (collectionsResult.success && collectionsResult.data) {
        setCollections(collectionsResult.data.collections);
        setTotalCount(collectionsResult.data.total_count);
      }

      const nextStats = await loadDatabaseStats({
        driverMeta,
        includeSqlStats: driverMeta.supportsSQL && fetchOptions.isOverview,
        namespace,
        sessionId,
        tableCount: collectionsResult.data?.total_count ?? 0,
      });

      setStats(nextStats);
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Failed to load database info');
    } finally {
      setLoading(false);
    }
  }, [activeTab, driverMeta, namespace, page, search, sessionId]);

  const refreshData = useCallback(async () => {
    switch (activeTab) {
      case 'schema':
        setLoading(false);
        return;
      case 'routines':
        setLoading(false);
        await loadRoutines();
        return;
      case 'triggers':
        setLoading(false);
        await loadTriggers();
        return;
      case 'sequences':
        setLoading(false);
        await loadSequences();
        return;
      default:
        await loadCollectionsAndStats();
    }
  }, [activeTab, loadCollectionsAndStats, loadRoutines, loadSequences, loadTriggers]);

  return {
    collections,
    dbEvents,
    error,
    loading,
    refreshData,
    routines,
    routinesLoading,
    sequences,
    sequencesLoading,
    stats,
    totalCount,
    triggers,
    triggersLoading,
    loadRoutines,
    loadSequences,
    loadTriggers,
  };
}

export function DatabaseBrowser({
  sessionId,
  namespace,
  driver,
  environment = 'development',
  readOnly = false,
  connectionName,
  connectionId,
  onTableSelect,
  schemaRefreshTrigger,
  onSchemaChange,
  onOpenQueryTab,
  onOpenFulltextSearch,
  onOpenRoutineSource,
  onCreateRoutine,
  onOpenTriggerSource,
  onCreateTrigger,
  onOpenEventSource,
  onCreateEvent,
  onOpenSequenceSource,
  onClose,
  initialTab,
  onActiveTabChange,
}: DatabaseBrowserProps) {
  const { t } = useTranslation();
  const terminology = getTerminology(driver);
  const driverMeta = getDriverMetadata(driver);
  const schemaObjectCapabilities = getSchemaObjectCapabilities(driver);

  const stableNamespace = useMemo<Namespace>(
    () => ({
      database: namespace.database,
      schema: namespace.schema,
    }),
    [namespace.database, namespace.schema]
  );

  const [activeTab, setActiveTab] = useState<DatabaseBrowserTab>(initialTab ?? 'overview');
  const [createTableOpen, setCreateTableOpen] = useState(false);
  const [redisEditorMode, setRedisEditorMode] = useState<RedisEditorMode | null>(null);
  const [luaModalOpen, setLuaModalOpen] = useState(false);
  const [schemaExportOpen, setSchemaExportOpen] = useState(false);
  const [search, setSearch] = useState('');
  const [page, setPage] = useState(1);

  const tableVisits = useMemo(
    () => getNamespaceTableVisits(stableNamespace, connectionId, OVERVIEW_COLLECTION_LIMIT),
    [connectionId, stableNamespace]
  );

  const {
    collections,
    dbEvents,
    error,
    loading,
    refreshData,
    routines,
    routinesLoading,
    sequences,
    sequencesLoading,
    stats,
    totalCount,
    triggers,
    triggersLoading,
    loadRoutines,
    loadSequences,
    loadTriggers,
  } = useDatabaseBrowserData({
    activeTab,
    driverMeta,
    namespace: stableNamespace,
    page,
    schemaObjectCapabilities,
    search,
    sessionId,
  });

  const overviewPreviewItems = buildOverviewPreviewItems(collections, tableVisits);
  const hasRoutinesTab = driverMeta.supportsSQL && schemaObjectCapabilities.routines;
  const hasTriggersTab =
    driverMeta.supportsSQL &&
    (schemaObjectCapabilities.triggers || schemaObjectCapabilities.events);
  const hasSequencesTab = driverMeta.supportsSQL && schemaObjectCapabilities.sequences;
  const hasSchemaTab = driverMeta.supportsSQL;

  const tabs: BrowserTabDefinition[] = [
    {
      id: 'overview',
      icon: Database,
      label: t('databaseBrowser.overview'),
    },
    {
      id: 'tables',
      icon: Table,
      label: `${t(terminology.tablePluralLabel)} (${totalCount})`,
    },
    ...(hasRoutinesTab
      ? [
          {
            id: 'routines' as const,
            icon: FunctionSquare,
            label: `${t('databaseBrowser.routines')} (${routines.length})`,
          },
        ]
      : []),
    ...(hasTriggersTab
      ? [
          {
            id: 'triggers' as const,
            icon: Zap,
            label: `${t('databaseBrowser.triggers')} (${triggers.length + dbEvents.length})`,
          },
        ]
      : []),
    ...(hasSequencesTab
      ? [
          {
            id: 'sequences' as const,
            icon: Hash,
            label: `${t('databaseBrowser.sequences')} (${sequences.length})`,
          },
        ]
      : []),
    ...(hasSchemaTab
      ? [
          {
            id: 'schema' as const,
            icon: List,
            label: t('databaseBrowser.schema'),
          },
        ]
      : []),
  ];

  const handleTabChange = useCallback(
    (tab: DatabaseBrowserTab) => {
      setActiveTab(tab);
      onActiveTabChange?.(tab);
    },
    [onActiveTabChange]
  );

  const formatVisitTime = (timestamp: number) => {
    const diffMs = Math.max(0, Date.now() - timestamp);
    const diffMins = Math.floor(diffMs / 60_000);
    const diffHours = Math.floor(diffMs / 3_600_000);
    const diffDays = Math.floor(diffMs / 86_400_000);

    if (diffMins < 1) return t('history.time.justNow');
    if (diffMins < 60) return t('history.time.minutesAgo', { count: diffMins });
    if (diffHours < 24) return t('history.time.hoursAgo', { count: diffHours });
    return t('history.time.daysAgo', { count: Math.max(1, diffDays) });
  };

  useEffect(() => {
    if (activeTab === 'routines' && !hasRoutinesTab) {
      handleTabChange('overview');
      return;
    }

    if (activeTab === 'triggers' && !hasTriggersTab) {
      handleTabChange('overview');
      return;
    }

    if (activeTab === 'sequences' && !hasSequencesTab) {
      handleTabChange('overview');
      return;
    }

    if (activeTab === 'schema' && !hasSchemaTab) {
      handleTabChange('overview');
    }
  }, [activeTab, handleTabChange, hasRoutinesTab, hasSchemaTab, hasSequencesTab, hasTriggersTab]);

  useEffect(() => {
    void refreshData();
  }, [refreshData]);

  useEffect(() => {
    return onTableChange(event => {
      if (event.type !== 'create' && event.type !== 'drop') {
        return;
      }

      if (isSameNamespace(event.namespace, stableNamespace)) {
        void refreshData();
      }
    });
  }, [refreshData, stableNamespace]);

  useEffect(() => {
    if (schemaRefreshTrigger === undefined) return;
    void refreshData();
  }, [refreshData, schemaRefreshTrigger]);

  let content: ReactNode;

  switch (activeTab) {
    case 'overview':
      content = (
        <OverviewTabContent
          formatVisitTime={formatVisitTime}
          loading={loading}
          error={error}
          namespace={stableNamespace}
          onTableSelect={onTableSelect}
          onViewAll={() => handleTabChange('tables')}
          overviewPreviewItems={overviewPreviewItems}
          stats={stats}
          terminology={terminology}
          totalCount={totalCount}
        />
      );
      break;
    case 'tables':
      content = (
        <TablesTabContent
          collections={collections}
          error={error}
          loading={loading}
          namespace={stableNamespace}
          onNextPage={() => setPage(currentPage => currentPage + 1)}
          onPreviousPage={() => setPage(currentPage => Math.max(1, currentPage - 1))}
          onSearchChange={value => {
            setSearch(value);
            setPage(1);
          }}
          onTableSelect={onTableSelect}
          page={page}
          search={search}
          totalCount={totalCount}
        />
      );
      break;
    case 'routines':
      content = (
        <RoutinesTabContent
          environment={environment}
          loading={routinesLoading}
          namespace={stableNamespace}
          onCreateRoutine={onCreateRoutine}
          onOpenRoutineSource={onOpenRoutineSource}
          onRefresh={loadRoutines}
          readOnly={readOnly}
          routines={routines}
          sessionId={sessionId}
        />
      );
      break;
    case 'triggers':
      content = (
        <TriggersTabContent
          dbEvents={dbEvents}
          driver={driver}
          environment={environment}
          loading={triggersLoading}
          namespace={stableNamespace}
          onCreateEvent={onCreateEvent}
          onCreateTrigger={onCreateTrigger}
          onOpenEventSource={onOpenEventSource}
          onOpenTriggerSource={onOpenTriggerSource}
          onRefresh={loadTriggers}
          readOnly={readOnly}
          sessionId={sessionId}
          supportsEvents={schemaObjectCapabilities.events}
          supportsTriggers={schemaObjectCapabilities.triggers}
          triggers={triggers}
        />
      );
      break;
    case 'sequences':
      content = (
        <SequencesTabContent
          environment={environment}
          loading={sequencesLoading}
          namespace={stableNamespace}
          onOpenSequenceSource={onOpenSequenceSource}
          onRefresh={loadSequences}
          readOnly={readOnly}
          sequences={sequences}
          sessionId={sessionId}
        />
      );
      break;
    default:
      content = (
        <div className="h-full">
          <ERDiagram
            sessionId={sessionId}
            namespace={stableNamespace}
            connectionId={connectionId}
            schemaRefreshTrigger={schemaRefreshTrigger}
            onTableSelect={onTableSelect}
          />
        </div>
      );
  }

  return (
    <div className="flex flex-col h-full bg-background rounded-lg border border-border shadow-sm overflow-hidden isolate contain-[paint]">
      <DatabaseBrowserHeader
        connectionName={connectionName}
        driver={driver}
        environment={environment}
        namespace={stableNamespace}
        onClose={onClose}
        onOpenCreateRedisKey={() => setRedisEditorMode({ kind: 'create-key' })}
        onOpenLuaScript={() => setLuaModalOpen(true)}
        onOpenCreateTable={() => setCreateTableOpen(true)}
        onOpenFulltextSearch={onOpenFulltextSearch}
        onOpenQueryTab={onOpenQueryTab}
        onOpenSchemaExport={() => setSchemaExportOpen(true)}
        readOnly={readOnly}
        supportsSql={driverMeta.supportsSQL}
      />

      <DatabaseBrowserTabBar activeTab={activeTab} onChange={handleTabChange} tabs={tabs} />

      <div className="flex-1 overflow-auto p-4">{content}</div>

      <CreateTableModal
        isOpen={createTableOpen}
        onClose={() => setCreateTableOpen(false)}
        sessionId={sessionId}
        namespace={stableNamespace}
        driver={driver}
        onTableCreated={tableName => {
          onSchemaChange?.();

          if (tableName) {
            emitTableChange({ type: 'create', namespace: stableNamespace, tableName });
            return;
          }

          void refreshData();
        }}
      />

      <SchemaExportDialog
        open={schemaExportOpen}
        onOpenChange={setSchemaExportOpen}
        sessionId={sessionId}
        namespace={stableNamespace}
        supportsRoutines={schemaObjectCapabilities.routines}
        supportsTriggers={schemaObjectCapabilities.triggers}
        supportsEvents={schemaObjectCapabilities.events}
        supportsSequences={schemaObjectCapabilities.sequences}
      />

      {redisEditorMode && (
        <RedisEditorModal
          isOpen={true}
          onClose={() => setRedisEditorMode(null)}
          mode={redisEditorMode}
          sessionId={sessionId}
          onSuccess={() => {
            void refreshData();
          }}
          readOnly={readOnly}
          environment={environment}
          connectionName={connectionName}
          connectionDatabase={stableNamespace.database}
        />
      )}

      {driver === Driver.Redis && (
        <LuaScriptModal
          isOpen={luaModalOpen}
          onClose={() => setLuaModalOpen(false)}
          sessionId={sessionId}
          environment={environment}
          connectionDatabase={stableNamespace.database}
          onSuccess={() => {
            void refreshData();
          }}
        />
      )}
    </div>
  );
}

interface DatabaseBrowserHeaderProps {
  connectionName?: string;
  driver: Driver;
  environment: Environment;
  namespace: Namespace;
  onClose: () => void;
  onOpenCreateRedisKey: () => void;
  onOpenLuaScript: () => void;
  onOpenCreateTable: () => void;
  onOpenFulltextSearch?: () => void;
  onOpenQueryTab?: (namespace: Namespace) => void;
  onOpenSchemaExport: () => void;
  readOnly: boolean;
  supportsSql: boolean;
}

function DatabaseBrowserHeader({
  connectionName,
  driver,
  environment,
  namespace,
  onClose,
  onOpenCreateRedisKey,
  onOpenLuaScript,
  onOpenCreateTable,
  onOpenFulltextSearch,
  onOpenQueryTab,
  onOpenSchemaExport,
  readOnly,
  supportsSql,
}: DatabaseBrowserHeaderProps) {
  const { t } = useTranslation();
  const displayName = namespace.schema
    ? `${namespace.database}.${namespace.schema}`
    : namespace.database;
  const iconSrc = `/databases/${DRIVER_ICONS[driver]}`;

  return (
    <div className="flex items-center justify-between px-4 py-3 border-b border-border bg-muted/20">
      <div className="flex items-center gap-3">
        <div className="p-2 rounded-md bg-accent/10 text-accent">
          <img src={iconSrc} alt={DRIVER_LABELS[driver]} className="w-4 h-4 object-contain" />
        </div>

        <div>
          <h2 className="font-semibold text-foreground">{displayName}</h2>
          <div className="flex items-center gap-2 text-xs text-muted-foreground">
            <ContentBreadcrumb connectionName={connectionName} namespace={namespace} />
            <span>•</span>
            <span>{DRIVER_LABELS[driver]}</span>
            <span>•</span>
            <span
              className={cn(
                'flex items-center gap-1',
                environment === 'production' && 'text-destructive'
              )}
            >
              {environment === 'production' ? <ShieldAlert size={10} /> : <Shield size={10} />}
              {t(`environment.${environment}`)}
            </span>
            {readOnly && (
              <>
                <span>•</span>
                <span className="text-warning">{t('environment.readOnly')}</span>
              </>
            )}
          </div>
        </div>
      </div>

      <div className="flex items-center gap-1">
        {onOpenFulltextSearch && (
          <Button
            variant="ghost"
            size="icon"
            className="h-8 w-8"
            onClick={onOpenFulltextSearch}
            title={t('fulltextSearch.title')}
          >
            <Search size={16} />
          </Button>
        )}

        {supportsSql && (
          <Button
            variant="outline"
            size="sm"
            className="h-8 gap-1.5 text-xs"
            onClick={() => onOpenQueryTab?.(namespace)}
          >
            <TerminalSquare size={14} />
            {t('databaseBrowser.openEditor')}
          </Button>
        )}

        {supportsSql && !readOnly && (
          <Button
            variant="ghost"
            size="icon"
            onClick={onOpenCreateTable}
            className="h-8 w-8"
            title={t('createTable.title')}
          >
            <Plus size={16} />
          </Button>
        )}

        {driver === Driver.Redis && !readOnly && (
          <>
            <Button
              variant="ghost"
              size="icon"
              onClick={onOpenCreateRedisKey}
              className="h-8 w-8"
              title={t('redis.createKeyTitle')}
            >
              <Plus size={16} />
            </Button>
            <Button
              variant="ghost"
              size="icon"
              onClick={onOpenLuaScript}
              className="h-8 w-8"
              title={t('redisLua.title')}
            >
              <FileCode size={16} />
            </Button>
          </>
        )}

        {supportsSql && (
          <Button
            variant="ghost"
            size="icon"
            className="h-8 w-8"
            onClick={onOpenSchemaExport}
            title={t('schemaExport.menuItem')}
          >
            <Download size={16} />
          </Button>
        )}

        <Button variant="ghost" size="icon" onClick={onClose} className="h-8 w-8">
          <X size={16} />
        </Button>
      </div>
    </div>
  );
}

function DatabaseBrowserTabBar({
  activeTab,
  onChange,
  tabs,
}: {
  activeTab: DatabaseBrowserTab;
  onChange: (tab: DatabaseBrowserTab) => void;
  tabs: BrowserTabDefinition[];
}) {
  return (
    <div className="flex items-center gap-1 px-4 py-2 border-b border-border bg-muted/10">
      {tabs.map(tab => {
        const Icon = tab.icon;

        return (
          <button
            key={tab.id}
            type="button"
            className={cn(
              TAB_BUTTON_CLASS_NAME,
              activeTab === tab.id
                ? 'bg-accent text-accent-foreground'
                : 'text-muted-foreground hover:text-foreground hover:bg-muted'
            )}
            onClick={() => onChange(tab.id)}
          >
            <span className="flex items-center gap-2">
              <Icon size={14} />
              {tab.label}
            </span>
          </button>
        );
      })}
    </div>
  );
}

interface OverviewTabContentProps {
  error: string | null;
  formatVisitTime: (timestamp: number) => string;
  loading: boolean;
  namespace: Namespace;
  onTableSelect: DatabaseBrowserProps['onTableSelect'];
  onViewAll: () => void;
  overviewPreviewItems: OverviewPreviewItem[];
  stats: DatabaseStats;
  terminology: DriverTerminology;
  totalCount: number;
}

function OverviewTabContent({
  error,
  formatVisitTime,
  loading,
  namespace,
  onTableSelect,
  onViewAll,
  overviewPreviewItems,
  stats,
  terminology,
  totalCount,
}: OverviewTabContentProps) {
  const { t } = useTranslation();

  if (loading) {
    return <CenteredLoadingState label={t('common.loading')} />;
  }

  if (error) {
    return <ErrorBanner message={error} />;
  }

  return (
    <div className="space-y-6">
      <div className="grid grid-cols-2 md:grid-cols-4 gap-4">
        {stats.sizeFormatted && (
          <StatCard
            icon={<HardDrive size={16} />}
            label={t('databaseBrowser.size')}
            value={stats.sizeFormatted}
          />
        )}

        <StatCard
          icon={<List size={16} />}
          label={t(terminology.tablePluralLabel)}
          value={stats.tableCount.toString()}
        />

        {stats.indexCount !== undefined && (
          <StatCard
            icon={<Hash size={16} />}
            label={t('databaseBrowser.indexCount')}
            value={stats.indexCount.toString()}
          />
        )}
      </div>

      <div className="space-y-2">
        <h3 className="text-sm font-medium text-foreground">
          {t(terminology.tablePluralLabel)}
          <span className="ml-1.5 text-muted-foreground font-normal">
            ({overviewPreviewItems.length})
          </span>
        </h3>

        {overviewPreviewItems.length === 0 ? (
          <div className="text-sm text-muted-foreground italic p-4 text-center border border-dashed border-border rounded-md">
            {t('databaseBrowser.noTables')}
          </div>
        ) : (
          <div className="border border-border rounded-md divide-y divide-border">
            {overviewPreviewItems.map(item => (
              <OverviewPreviewRow
                key={item.name}
                formatVisitTime={formatVisitTime}
                item={item}
                namespace={namespace}
                onTableSelect={onTableSelect}
              />
            ))}

            {totalCount > overviewPreviewItems.length && (
              <button
                type="button"
                className="w-full px-3 py-2 text-sm text-muted-foreground hover:text-foreground hover:bg-muted/50 transition-colors"
                onClick={onViewAll}
              >
                {t('databaseBrowser.viewAll', { count: totalCount })}
              </button>
            )}
          </div>
        )}
      </div>
    </div>
  );
}

function OverviewPreviewRow({
  formatVisitTime,
  item,
  namespace,
  onTableSelect,
}: {
  formatVisitTime: (timestamp: number) => string;
  item: OverviewPreviewItem;
  namespace: Namespace;
  onTableSelect: DatabaseBrowserProps['onTableSelect'];
}) {
  return (
    <button
      type="button"
      className="flex items-center justify-between w-full px-3 py-2.5 hover:bg-muted/50 transition-colors text-left"
      onClick={() => onTableSelect(namespace, item.name)}
    >
      <div className="min-w-0 flex flex-1 items-center gap-2.5">
        <CollectionTypeIcon collectionType={item.collectionType} size={14} />
        <div className="min-w-0 flex-1">
          <div className="flex items-center gap-1.5">
            <span className="font-mono text-sm truncate">{item.name}</span>
            {isViewCollection(item.collectionType) && (
              <span className="text-xs text-muted-foreground shrink-0">(view)</span>
            )}
          </div>
          {item.personalized && item.lastVisitedAt && (
            <p className="text-xs text-muted-foreground mt-0.5 truncate">
              {formatVisitTime(item.lastVisitedAt)}
            </p>
          )}
        </div>
      </div>

      {item.personalized && item.visitCount && (
        <VisitFrequencyDots count={item.visitCount} />
      )}

      <ChevronRight size={14} className="ml-2 shrink-0 text-muted-foreground" />
    </button>
  );
}

const VISIT_FREQUENCY_MAX_DOTS = 5;

function VisitFrequencyDots({ count }: { count: number }) {
  const filled = Math.min(Math.ceil(count / 3), VISIT_FREQUENCY_MAX_DOTS);

  return (
    <div className="flex items-center gap-0.5 ml-2 shrink-0" title={`${count} visits`}>
      {Array.from({ length: VISIT_FREQUENCY_MAX_DOTS }, (_, i) => (
        <span
          key={`dot-${i.toString()}`}
          className={cn(
            'block w-1.5 h-1.5 rounded-full',
            i < filled ? 'bg-accent' : 'bg-border'
          )}
        />
      ))}
    </div>
  );
}

interface TablesTabContentProps {
  collections: Collection[];
  error: string | null;
  loading: boolean;
  namespace: Namespace;
  onNextPage: () => void;
  onPreviousPage: () => void;
  onSearchChange: (value: string) => void;
  onTableSelect: DatabaseBrowserProps['onTableSelect'];
  page: number;
  search: string;
  totalCount: number;
}

function TablesTabContent({
  collections,
  error,
  loading,
  namespace,
  onNextPage,
  onPreviousPage,
  onSearchChange,
  onTableSelect,
  page,
  search,
  totalCount,
}: TablesTabContentProps) {
  const { t } = useTranslation();

  return (
    <div className="flex flex-col h-full gap-4">
      <div className="flex items-center gap-2">
        <div className="relative flex-1">
          <Search className="absolute left-2.5 top-2.5 h-4 w-4 text-muted-foreground" />
          <Input
            placeholder={t('databaseBrowser.searchTables')}
            value={search}
            onChange={event => onSearchChange(event.target.value)}
            className="pl-9"
          />
        </div>
      </div>

      <ListSurface loading={loading}>
        {!loading && error ? (
          <ErrorBanner className="m-4" message={error} />
        ) : collections.length === 0 && !loading ? (
          <ListEmptyState
            message={search ? t('databaseBrowser.noResults') : t('databaseBrowser.noTables')}
          />
        ) : (
          collections.map(collection => (
            <button
              type="button"
              key={collection.name}
              className="flex items-center justify-between w-full px-4 py-3 hover:bg-muted/50 transition-colors text-left"
              onClick={() => onTableSelect(namespace, collection.name)}
            >
              <div className="flex items-center gap-3">
                <CollectionTypeIcon collectionType={collection.collection_type} size={16} />
                <div>
                  <span className="font-mono text-sm">{collection.name}</span>
                  {isViewCollection(collection.collection_type) && (
                    <span className="ml-2 text-xs text-muted-foreground">(view)</span>
                  )}
                </div>
              </div>
              <ChevronRight size={16} className="text-muted-foreground" />
            </button>
          ))
        )}
      </ListSurface>

      <div className="flex items-center justify-between border-t border-border pt-4">
        <div className="text-sm text-muted-foreground">
          {t('common.pagination', {
            start: totalCount === 0 ? 0 : (page - 1) * TABLES_PAGE_SIZE + 1,
            end: Math.min(page * TABLES_PAGE_SIZE, totalCount),
            total: totalCount,
          })}
        </div>

        <div className="flex items-center gap-2">
          <Button
            variant="outline"
            size="sm"
            onClick={onPreviousPage}
            disabled={page === 1 || loading}
          >
            <ChevronLeft size={16} />
          </Button>

          <div className="text-sm font-medium w-8 text-center">{page}</div>

          <Button
            variant="outline"
            size="sm"
            onClick={onNextPage}
            disabled={page * TABLES_PAGE_SIZE >= totalCount || loading}
          >
            <ChevronRight size={16} />
          </Button>
        </div>
      </div>
    </div>
  );
}

interface RoutinesTabContentProps {
  environment: Environment;
  loading: boolean;
  namespace: Namespace;
  onCreateRoutine?: (routineType: 'Function' | 'Procedure', namespace: Namespace) => void;
  onOpenRoutineSource?: (routine: Routine, namespace: Namespace) => void;
  onRefresh: () => Promise<void>;
  readOnly: boolean;
  routines: Routine[];
  sessionId: string;
}

function RoutinesTabContent({
  environment,
  loading,
  namespace,
  onCreateRoutine,
  onOpenRoutineSource,
  onRefresh,
  readOnly,
  routines,
  sessionId,
}: RoutinesTabContentProps) {
  const { t } = useTranslation();
  const functionRoutines = routines.filter(routine => routine.routine_type === 'Function');
  const procedureRoutines = routines.filter(routine => routine.routine_type === 'Procedure');

  return (
    <div className="flex flex-col h-full gap-4">
      {onCreateRoutine && (
        <div className="flex gap-2">
          <Button
            variant="outline"
            size="sm"
            onClick={() => onCreateRoutine('Function', namespace)}
          >
            <Plus size={14} className="mr-1" />
            {t('routineManager.createFunction')}
          </Button>
          <Button
            variant="outline"
            size="sm"
            onClick={() => onCreateRoutine('Procedure', namespace)}
          >
            <Plus size={14} className="mr-1" />
            {t('routineManager.createProcedure')}
          </Button>
        </div>
      )}

      <ListSurface loading={loading}>
        {routines.length === 0 && !loading ? (
          <ListEmptyState message={t('databaseBrowser.noRoutines')} />
        ) : (
          <>
            <RoutineSection
              environment={environment}
              keyPrefix="fn"
              namespace={namespace}
              onOpenRoutineSource={onOpenRoutineSource}
              onRefresh={onRefresh}
              readOnly={readOnly}
              routines={functionRoutines}
              rowIcon={FunctionSquare}
              sessionId={sessionId}
              title={t('dbtree.functions')}
              titleIcon={FunctionSquare}
            />
            <RoutineSection
              environment={environment}
              keyPrefix="proc"
              namespace={namespace}
              onOpenRoutineSource={onOpenRoutineSource}
              onRefresh={onRefresh}
              readOnly={readOnly}
              routines={procedureRoutines}
              rowIcon={PlayCircle}
              sessionId={sessionId}
              title={t('dbtree.procedures')}
              titleIcon={PlayCircle}
            />
          </>
        )}
      </ListSurface>
    </div>
  );
}

interface RoutineSectionProps {
  environment: Environment;
  keyPrefix: string;
  namespace: Namespace;
  onOpenRoutineSource?: (routine: Routine, namespace: Namespace) => void;
  onRefresh: () => Promise<void>;
  readOnly: boolean;
  routines: Routine[];
  rowIcon: LucideIcon;
  sessionId: string;
  title: string;
  titleIcon: LucideIcon;
}

function RoutineSection({
  environment,
  keyPrefix,
  namespace,
  onOpenRoutineSource,
  onRefresh,
  readOnly,
  routines,
  rowIcon,
  sessionId,
  title,
  titleIcon,
}: RoutineSectionProps) {
  const RowIcon = rowIcon;

  if (routines.length === 0) {
    return null;
  }

  return (
    <>
      <ListSectionHeader icon={titleIcon} title={title} count={routines.length} />

      {routines.map(routine => (
        <RoutineContextMenu
          key={`${keyPrefix}-${routine.name}-${routine.arguments}`}
          routine={routine}
          sessionId={sessionId}
          environment={environment}
          readOnly={readOnly}
          onViewSource={targetRoutine => onOpenRoutineSource?.(targetRoutine, namespace)}
          onDrop={onRefresh}
        >
          <button
            type="button"
            className="flex items-center justify-between w-full px-4 py-3 hover:bg-muted/50 transition-colors text-left cursor-pointer"
            onClick={() => onOpenRoutineSource?.(routine, namespace)}
          >
            <div className="flex items-center gap-3">
              <RowIcon size={16} className="text-muted-foreground" />
              <div>
                <span className="font-mono text-sm">{routine.name}</span>
                <span className="text-xs text-muted-foreground ml-1">({routine.arguments})</span>
                {routine.return_type && (
                  <span className="text-xs text-muted-foreground ml-1">
                    &rarr; {routine.return_type}
                  </span>
                )}
              </div>
            </div>

            <div className="flex items-center gap-2">
              {routine.language && (
                <span className="text-xs text-muted-foreground bg-muted px-2 py-0.5 rounded">
                  {routine.language}
                </span>
              )}
              <Code2 size={14} className="text-muted-foreground/50" />
            </div>
          </button>
        </RoutineContextMenu>
      ))}
    </>
  );
}

interface TriggersTabContentProps {
  dbEvents: DatabaseEvent[];
  driver: Driver;
  environment: Environment;
  loading: boolean;
  namespace: Namespace;
  onCreateEvent?: (namespace: Namespace) => void;
  onCreateTrigger?: (namespace: Namespace) => void;
  onOpenEventSource?: (event: DatabaseEvent, namespace: Namespace) => void;
  onOpenTriggerSource?: (trigger: Trigger, namespace: Namespace) => void;
  onRefresh: () => Promise<void>;
  readOnly: boolean;
  sessionId: string;
  supportsEvents: boolean;
  supportsTriggers: boolean;
  triggers: Trigger[];
}

function TriggersTabContent({
  dbEvents,
  driver,
  environment,
  loading,
  namespace,
  onCreateEvent,
  onCreateTrigger,
  onOpenEventSource,
  onOpenTriggerSource,
  onRefresh,
  readOnly,
  sessionId,
  supportsEvents,
  supportsTriggers,
  triggers,
}: TriggersTabContentProps) {
  const { t } = useTranslation();

  return (
    <div className="flex flex-col h-full gap-4">
      {!readOnly && (
        <div className="flex items-center gap-2">
          {supportsTriggers && (
            <Button variant="outline" size="sm" onClick={() => onCreateTrigger?.(namespace)}>
              <Plus size={14} className="mr-1" />
              {t('triggerManager.createTrigger')}
            </Button>
          )}
          {supportsEvents && (
            <Button variant="outline" size="sm" onClick={() => onCreateEvent?.(namespace)}>
              <Plus size={14} className="mr-1" />
              {t('eventManager.createEvent')}
            </Button>
          )}
        </div>
      )}

      <ListSurface loading={loading}>
        {triggers.length === 0 && dbEvents.length === 0 && !loading ? (
          <ListEmptyState message={t('databaseBrowser.noTriggers')} />
        ) : (
          <>
            {supportsTriggers && triggers.length > 0 && (
              <>
                <ListSectionHeader
                  icon={Zap}
                  title={t('databaseBrowser.triggers')}
                  count={triggers.length}
                />
                {triggers.map(trigger => (
                  <TriggerRow
                    key={trigger.name}
                    driver={driver}
                    environment={environment}
                    namespace={namespace}
                    onOpenTriggerSource={onOpenTriggerSource}
                    onRefresh={onRefresh}
                    readOnly={readOnly}
                    sessionId={sessionId}
                    trigger={trigger}
                  />
                ))}
              </>
            )}

            {supportsEvents && dbEvents.length > 0 && (
              <>
                <ListSectionHeader
                  icon={Calendar}
                  title={t('databaseBrowser.events')}
                  count={dbEvents.length}
                />
                {dbEvents.map(event => (
                  <EventRow
                    key={event.name}
                    environment={environment}
                    event={event}
                    namespace={namespace}
                    onOpenEventSource={onOpenEventSource}
                    onRefresh={onRefresh}
                    readOnly={readOnly}
                    sessionId={sessionId}
                  />
                ))}
              </>
            )}
          </>
        )}
      </ListSurface>
    </div>
  );
}

function TriggerRow({
  driver,
  environment,
  namespace,
  onOpenTriggerSource,
  onRefresh,
  readOnly,
  sessionId,
  trigger,
}: {
  driver: Driver;
  environment: Environment;
  namespace: Namespace;
  onOpenTriggerSource?: (trigger: Trigger, namespace: Namespace) => void;
  onRefresh: () => Promise<void>;
  readOnly: boolean;
  sessionId: string;
  trigger: Trigger;
}) {
  return (
    <TriggerContextMenu
      trigger={trigger}
      sessionId={sessionId}
      environment={environment}
      readOnly={readOnly}
      supportsToggle={driver !== 'mysql'}
      onViewSource={targetTrigger => onOpenTriggerSource?.(targetTrigger, namespace)}
      onDrop={onRefresh}
      onToggle={onRefresh}
    >
      <button
        type="button"
        className="flex items-center justify-between w-full px-4 py-3 hover:bg-muted/50 transition-colors text-left"
        onClick={() => onOpenTriggerSource?.(trigger, namespace)}
      >
        <div className="flex items-center gap-3">
          <Zap
            size={16}
            className={cn('text-muted-foreground', !trigger.enabled && 'opacity-40')}
          />
          <div>
            <span className="font-mono text-sm">{trigger.name}</span>
            <span className="text-xs text-muted-foreground ml-2">
              {trigger.timing} {trigger.events.join(' | ')} ON {trigger.table_name}
            </span>
          </div>
        </div>

        <div className="flex items-center gap-2">
          {trigger.function_name && (
            <span className="text-xs text-muted-foreground bg-muted px-2 py-0.5 rounded">
              {trigger.function_name}
            </span>
          )}
          {!trigger.enabled && (
            <span className="text-xs text-orange-500 bg-orange-500/10 px-2 py-0.5 rounded">
              disabled
            </span>
          )}
        </div>
      </button>
    </TriggerContextMenu>
  );
}

function EventRow({
  environment,
  event,
  namespace,
  onOpenEventSource,
  onRefresh,
  readOnly,
  sessionId,
}: {
  environment: Environment;
  event: DatabaseEvent;
  namespace: Namespace;
  onOpenEventSource?: (event: DatabaseEvent, namespace: Namespace) => void;
  onRefresh: () => Promise<void>;
  readOnly: boolean;
  sessionId: string;
}) {
  return (
    <EventContextMenu
      event={event}
      sessionId={sessionId}
      environment={environment}
      readOnly={readOnly}
      onViewSource={targetEvent => onOpenEventSource?.(targetEvent, namespace)}
      onDrop={onRefresh}
    >
      <button
        type="button"
        className="flex items-center justify-between w-full px-4 py-3 hover:bg-muted/50 transition-colors text-left"
        onClick={() => onOpenEventSource?.(event, namespace)}
      >
        <div className="flex items-center gap-3">
          <Calendar size={16} className="text-muted-foreground" />
          <div>
            <span className="font-mono text-sm">{event.name}</span>
            <span className="text-xs text-muted-foreground ml-2">
              {event.event_type}
              {event.interval_value && event.interval_field && (
                <>
                  {' '}
                  every {event.interval_value} {event.interval_field}
                </>
              )}
            </span>
          </div>
        </div>

        <span
          className={cn(
            'text-xs px-2 py-0.5 rounded',
            event.status === 'Enabled'
              ? 'text-emerald-600 bg-emerald-500/10'
              : 'text-orange-500 bg-orange-500/10'
          )}
        >
          {event.status}
        </span>
      </button>
    </EventContextMenu>
  );
}

interface SequencesTabContentProps {
  environment: Environment;
  loading: boolean;
  namespace: Namespace;
  onOpenSequenceSource?: (sequence: Sequence, namespace: Namespace) => void;
  onRefresh: () => Promise<void>;
  readOnly: boolean;
  sequences: Sequence[];
  sessionId: string;
}

function SequencesTabContent({
  environment,
  loading,
  namespace,
  onOpenSequenceSource,
  onRefresh,
  readOnly,
  sequences,
  sessionId,
}: SequencesTabContentProps) {
  const { t } = useTranslation();

  return (
    <div className="flex flex-col h-full gap-4">
      <ListSurface loading={loading}>
        {sequences.length === 0 && !loading ? (
          <ListEmptyState message={t('databaseBrowser.noSequences')} />
        ) : (
          sequences.map(sequence => (
            <SequenceContextMenu
              key={sequence.name}
              sequence={sequence}
              sessionId={sessionId}
              environment={environment}
              readOnly={readOnly}
              onViewSource={targetSequence => onOpenSequenceSource?.(targetSequence, namespace)}
              onDrop={onRefresh}
            >
              <button
                type="button"
                className="flex items-center justify-between w-full px-4 py-3 hover:bg-muted/50 transition-colors text-left"
                onClick={() => onOpenSequenceSource?.(sequence, namespace)}
              >
                <div className="flex items-center gap-3">
                  <Hash size={16} className="text-muted-foreground" />
                  <div>
                    <span className="font-mono text-sm">{sequence.name}</span>
                    <span className="text-xs text-muted-foreground ml-2">
                      {sequence.data_type} — increment: {sequence.increment}
                    </span>
                  </div>
                </div>

                <div className="flex items-center gap-2">
                  <span className="text-xs text-muted-foreground bg-muted px-2 py-0.5 rounded">
                    {sequence.min_value} → {sequence.max_value}
                  </span>
                  {sequence.cycle && (
                    <span className="text-xs text-blue-500 bg-blue-500/10 px-2 py-0.5 rounded">
                      cycle
                    </span>
                  )}
                </div>
              </button>
            </SequenceContextMenu>
          ))
        )}
      </ListSurface>
    </div>
  );
}

function CollectionTypeIcon({
  collectionType,
  className,
  size,
}: {
  collectionType?: Collection['collection_type'];
  className?: string;
  size: number;
}) {
  const Icon = isViewCollection(collectionType) ? Eye : Table;
  return <Icon size={size} className={cn('text-muted-foreground shrink-0', className)} />;
}

function CenteredLoadingState({ label }: { label: string }) {
  return (
    <div className="flex items-center justify-center h-full gap-2 text-muted-foreground">
      <Loader2 size={20} className="animate-spin" />
      <span>{label}</span>
    </div>
  );
}

function ErrorBanner({ className, message }: { className?: string; message: string }) {
  return (
    <div
      className={cn(
        'flex items-center gap-3 p-4 rounded-md bg-error/10 border border-error/20 text-error',
        className
      )}
    >
      <AlertCircle size={18} />
      <pre className="text-sm font-mono whitespace-pre-wrap">{message}</pre>
    </div>
  );
}

function ListSurface({ children, loading }: { children: ReactNode; loading?: boolean }) {
  return (
    <div className={LIST_SURFACE_CLASS_NAME}>
      {loading && <LoadingOverlay />}
      {children}
    </div>
  );
}

function LoadingOverlay() {
  return (
    <div className="absolute inset-0 z-10 bg-background/50 flex items-center justify-center backdrop-blur-[1px]">
      <Loader2 size={24} className="animate-spin text-primary" />
    </div>
  );
}

function ListEmptyState({ message }: { message: string }) {
  return <div className="text-sm text-muted-foreground italic p-8 text-center">{message}</div>;
}

function ListSectionHeader({
  count,
  icon,
  title,
}: {
  count: number;
  icon: LucideIcon;
  title: string;
}) {
  const Icon = icon;

  return (
    <div className={SECTION_HEADER_CLASS_NAME}>
      <h4 className="text-xs font-semibold text-muted-foreground uppercase tracking-wider flex items-center gap-2">
        <Icon size={12} />
        {title} ({count})
      </h4>
    </div>
  );
}
