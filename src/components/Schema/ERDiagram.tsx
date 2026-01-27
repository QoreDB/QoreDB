import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import type { PointerEvent, WheelEvent } from 'react';
import { useTranslation } from 'react-i18next';
import {
  Search,
  RefreshCw,
  ZoomIn,
  ZoomOut,
  Crosshair,
  Loader2,
  Table as TableIcon,
  Filter,
} from 'lucide-react';
import { Namespace, TableSchema, TableColumn, ForeignKey } from '@/lib/tauri';
import { useSchemaCache } from '@/hooks/useSchemaCache';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { cn } from '@/lib/utils';
import { onTableChange } from '@/lib/tableEvents';

interface ERDiagramProps {
  sessionId: string;
  namespace: Namespace;
  schemaRefreshTrigger?: number;
  onTableSelect: (namespace: Namespace, tableName: string) => void;
}

interface DiagramColumn {
  name: string;
  data_type: string;
  isPrimary: boolean;
  isForeign: boolean;
}

interface TableInfo {
  id: string;
  name: string;
  schema: TableSchema;
  columns: TableColumn[];
  primaryKey: string[];
  foreignKeyColumns: string[];
  incomingColumns: string[];
}

interface DiagramNode extends TableInfo {
  x: number;
  y: number;
  width: number;
  height: number;
  displayColumns: DiagramColumn[];
  overflowCount: number;
  columnIndex: Map<string, number>;
}

interface DiagramEdge {
  id: string;
  fromId: string;
  toId: string;
  fromTable: string;
  toTable: string;
  fromColumn: string;
  toColumn: string;
  constraint?: string;
}

interface EdgePath extends DiagramEdge {
  path: string;
  midX: number;
  midY: number;
}

const MAX_COLUMNS = 8;
const NODE_WIDTH = 240;
const COLUMN_GAP = 90;
const ROW_GAP = 56;
const HEADER_HEIGHT = 32;
const ROW_HEIGHT = 20;
const FOOTER_HEIGHT = 14;
const MIN_ZOOM = 0.4;
const MAX_ZOOM = 1.6;
const SCHEMA_CONCURRENCY = 6;

function makeTableId(namespace: Namespace, tableName: string): string {
  return `${namespace.database}:${namespace.schema ?? ''}:${tableName}`;
}

function buildDisplayColumns(
  columns: TableColumn[],
  primaryKey: Set<string>,
  foreignKeys: Set<string>,
  incomingRefs: Set<string>
): { displayColumns: DiagramColumn[]; overflow: number } {
  const mustInclude = new Set<string>();
  primaryKey.forEach(col => mustInclude.add(col));
  foreignKeys.forEach(col => mustInclude.add(col));
  incomingRefs.forEach(col => mustInclude.add(col));

  const displayColumns: DiagramColumn[] = [];
  const seen = new Set<string>();

  for (const column of columns) {
    if (!mustInclude.has(column.name)) continue;
    displayColumns.push({
      name: column.name,
      data_type: column.data_type,
      isPrimary: primaryKey.has(column.name),
      isForeign: foreignKeys.has(column.name),
    });
    seen.add(column.name);
  }

  for (const column of columns) {
    if (displayColumns.length >= MAX_COLUMNS && !mustInclude.has(column.name)) break;
    if (seen.has(column.name)) continue;
    displayColumns.push({
      name: column.name,
      data_type: column.data_type,
      isPrimary: primaryKey.has(column.name),
      isForeign: foreignKeys.has(column.name),
    });
    seen.add(column.name);
  }

  const overflow = Math.max(columns.length - displayColumns.length, 0);
  return { displayColumns, overflow };
}

function estimateNodeHeight(displayColumns: DiagramColumn[], overflowCount: number): number {
  const rows = Math.max(displayColumns.length, 1);
  const footer = overflowCount > 0 ? FOOTER_HEIGHT + 8 : FOOTER_HEIGHT;
  return HEADER_HEIGHT + rows * ROW_HEIGHT + footer;
}

function buildLayout(
  tables: TableInfo[],
  expandedTables: Set<string>
): { nodes: DiagramNode[]; width: number; height: number } {
  if (tables.length === 0) {
    return { nodes: [], width: 0, height: 0 };
  }

  const sorted = [...tables].sort((a, b) => a.name.localeCompare(b.name));
  const columnCount = Math.min(4, Math.max(1, Math.ceil(Math.sqrt(sorted.length))));
  const columnHeights = new Array(columnCount).fill(0);
  const nodes: DiagramNode[] = [];

  for (const table of sorted) {
    const isExpanded = expandedTables.has(table.id);
    const primaryKey = new Set(table.primaryKey);
    const foreignKeys = new Set(table.foreignKeyColumns);
    const incomingRefs = new Set(table.incomingColumns);

    const { displayColumns, overflow } = isExpanded
      ? {
          displayColumns: table.columns.map(column => ({
            name: column.name,
            data_type: column.data_type,
            isPrimary: primaryKey.has(column.name),
            isForeign: foreignKeys.has(column.name),
          })),
          overflow: 0,
        }
      : buildDisplayColumns(table.columns, primaryKey, foreignKeys, incomingRefs);

    const height = estimateNodeHeight(displayColumns, overflow);
    let targetColumn = 0;
    for (let i = 1; i < columnCount; i += 1) {
      if (columnHeights[i] < columnHeights[targetColumn]) {
        targetColumn = i;
      }
    }

    const x = targetColumn * (NODE_WIDTH + COLUMN_GAP);
    const y = columnHeights[targetColumn];
    columnHeights[targetColumn] += height + ROW_GAP;

    nodes.push({
      ...table,
      x,
      y,
      width: NODE_WIDTH,
      height,
      displayColumns,
      overflowCount: overflow,
      columnIndex: new Map(displayColumns.map((col, idx) => [col.name, idx])),
    });
  }

  const width = columnCount * NODE_WIDTH + (columnCount - 1) * COLUMN_GAP;
  const height = Math.max(...columnHeights) - ROW_GAP;

  return {
    nodes,
    width: Math.max(width, NODE_WIDTH),
    height: Math.max(height, 200),
  };
}

function buildEdgePaths(edges: DiagramEdge[], nodesById: Map<string, DiagramNode>): EdgePath[] {
  const paths: EdgePath[] = [];

  for (const edge of edges) {
    const source = nodesById.get(edge.fromId);
    const target = nodesById.get(edge.toId);
    if (!source || !target) continue;
    if (edge.fromId === edge.toId) continue;

    const sourceIndex =
      source.columnIndex.get(edge.fromColumn) ?? Math.floor(source.displayColumns.length / 2);
    const targetIndex =
      target.columnIndex.get(edge.toColumn) ?? Math.floor(target.displayColumns.length / 2);

    const startX = source.x + (target.x >= source.x ? source.width : 0);
    const endX = target.x + (target.x >= source.x ? 0 : target.width);
    const startY = source.y + HEADER_HEIGHT + sourceIndex * ROW_HEIGHT + ROW_HEIGHT / 2;
    const endY = target.y + HEADER_HEIGHT + targetIndex * ROW_HEIGHT + ROW_HEIGHT / 2;

    const deltaX = Math.max(Math.abs(endX - startX) * 0.45, 40);
    const controlX1 = startX + (endX >= startX ? deltaX : -deltaX);
    const controlX2 = endX + (endX >= startX ? -deltaX : deltaX);

    const path = `M ${startX} ${startY} C ${controlX1} ${startY}, ${controlX2} ${endY}, ${endX} ${endY}`;

    paths.push({
      ...edge,
      path,
      midX: (startX + endX) / 2,
      midY: (startY + endY) / 2,
    });
  }

  return paths;
}

export function ERDiagram({
  sessionId,
  namespace,
  schemaRefreshTrigger,
  onTableSelect,
}: ERDiagramProps) {
  const { t } = useTranslation();
  const { getCollections, getTableSchema, forceRefresh } = useSchemaCache(sessionId);
  const [loading, setLoading] = useState(true);
  const [progress, setProgress] = useState({ loaded: 0, total: 0 });
  const [error, setError] = useState<string | null>(null);
  const [tables, setTables] = useState<TableInfo[]>([]);
  const [edges, setEdges] = useState<DiagramEdge[]>([]);
  const [search, setSearch] = useState('');
  const [hoveredTable, setHoveredTable] = useState<string | null>(null);
  const [hoveredEdge, setHoveredEdge] = useState<string | null>(null);
  const [selectedTableId, setSelectedTableId] = useState<string | null>(null);
  const [isolatedTableId, setIsolatedTableId] = useState<string | null>(null);
  const [expandedTables, setExpandedTables] = useState<string[]>([]);
  const [zoom, setZoom] = useState(1);
  const [pan, setPan] = useState({ x: 0, y: 0 });
  const [hasFit, setHasFit] = useState(false);
  const viewportRef = useRef<HTMLDivElement | null>(null);
  const pointerRef = useRef<{ x: number; y: number; panX: number; panY: number } | null>(null);
  const isPanningRef = useRef(false);
  const progressRef = useRef({ loaded: 0, total: 0 });
  const progressRafRef = useRef<number | null>(null);

  const loadIdRef = useRef(0);

  const scheduleProgressUpdate = useCallback(() => {
    if (progressRafRef.current !== null) return;
    progressRafRef.current = window.requestAnimationFrame(() => {
      progressRafRef.current = null;
      setProgress({ ...progressRef.current });
    });
  }, []);

  useEffect(() => {
    return () => {
      if (progressRafRef.current !== null) {
        window.cancelAnimationFrame(progressRafRef.current);
      }
    };
  }, []);

  const loadSchema = useCallback(
    async (forceReload = false) => {
      const loadId = loadIdRef.current + 1;
      loadIdRef.current = loadId;

      setLoading(true);
      setError(null);
      progressRef.current = { loaded: 0, total: 0 };
      setProgress({ loaded: 0, total: 0 });
      setHasFit(false);

      try {
        if (forceReload) {
          forceRefresh();
        }
        const collections = await getCollections(namespace);
        if (loadId !== loadIdRef.current) return;

        const tableCollections = collections.filter(col => col.collection_type === 'Table');
        const totalTables = tableCollections.length;
        progressRef.current = { loaded: 0, total: totalTables };
        setProgress({ loaded: 0, total: totalTables });

        const schemaMap = new Map<string, TableSchema>();
        const incomingRefs = new Map<string, Set<string>>();
        const tableIdByName = new Map<string, string>();

        tableCollections.forEach(col => {
          tableIdByName.set(col.name, makeTableId(namespace, col.name));
        });

        const queue = [...tableCollections];
        const workers = Array.from(
          { length: Math.min(SCHEMA_CONCURRENCY, totalTables || 1) },
          async () => {
            while (queue.length > 0) {
              const table = queue.shift();
              if (!table || loadId !== loadIdRef.current) break;
              const schema = await getTableSchema(namespace, table.name);
              schemaMap.set(
                table.name,
                schema ?? { columns: [], foreign_keys: [], primary_key: [], indexes: [] }
              );
              if (loadId !== loadIdRef.current) break;
              progressRef.current = {
                loaded: Math.min(progressRef.current.loaded + 1, totalTables),
                total: totalTables,
              };
              scheduleProgressUpdate();
            }
          }
        );

        await Promise.all(workers);
        if (loadId !== loadIdRef.current) return;

        schemaMap.forEach(schema => {
          schema.foreign_keys?.forEach(fk => {
            const refDb = fk.referenced_database ?? namespace.database;
            const refSchema = fk.referenced_schema ?? namespace.schema;
            if (refDb !== namespace.database) return;
            if ((refSchema ?? '') !== (namespace.schema ?? '')) return;
            const refs = incomingRefs.get(fk.referenced_table) ?? new Set<string>();
            refs.add(fk.referenced_column);
            incomingRefs.set(fk.referenced_table, refs);
          });
        });

        const tableInfos: TableInfo[] = tableCollections.map(col => {
          const schema = schemaMap.get(col.name) ?? {
            columns: [],
            foreign_keys: [],
            primary_key: [],
            indexes: [],
          };
          const incoming = incomingRefs.get(col.name) ?? new Set<string>();
          return {
            id: makeTableId(namespace, col.name),
            name: col.name,
            schema,
            columns: schema.columns ?? [],
            primaryKey: schema.primary_key ?? [],
            foreignKeyColumns: schema.foreign_keys?.map(fk => fk.column) ?? [],
            incomingColumns: Array.from(incoming),
          };
        });

        const diagramEdges: DiagramEdge[] = [];

        schemaMap.forEach((schema, tableName) => {
          const fromId = tableIdByName.get(tableName);
          if (!fromId) return;

          schema.foreign_keys?.forEach((fk: ForeignKey) => {
            const refDb = fk.referenced_database ?? namespace.database;
            const refSchema = fk.referenced_schema ?? namespace.schema;
            if (refDb !== namespace.database) return;
            if ((refSchema ?? '') !== (namespace.schema ?? '')) return;

            const toId = tableIdByName.get(fk.referenced_table);
            if (!toId) return;

            diagramEdges.push({
              id: `${fromId}:${fk.column}->${toId}:${fk.referenced_column}`,
              fromId,
              toId,
              fromTable: tableName,
              toTable: fk.referenced_table,
              fromColumn: fk.column,
              toColumn: fk.referenced_column,
              constraint: fk.constraint_name,
            });
          });
        });

        setTables(tableInfos);
        setEdges(diagramEdges);
      } catch (err) {
        if (loadId === loadIdRef.current) {
          setError(err instanceof Error ? err.message : 'Failed to load schema');
        }
      } finally {
        if (loadId === loadIdRef.current) setLoading(false);
      }
    },
    [namespace, getCollections, getTableSchema, forceRefresh, scheduleProgressUpdate]
  );

  useEffect(() => {
    void loadSchema();
  }, [loadSchema]);

  useEffect(() => {
    if (schemaRefreshTrigger === undefined) return;
    void loadSchema(true);
  }, [schemaRefreshTrigger, loadSchema]);

  useEffect(() => {
    return onTableChange(event => {
      if (event.type !== 'create' && event.type !== 'drop') return;
      if (
        event.namespace.database === namespace.database &&
        (event.namespace.schema ?? '') === (namespace.schema ?? '')
      ) {
        void loadSchema(true);
      }
    });
  }, [loadSchema, namespace.database, namespace.schema]);

  useEffect(() => {
    const ids = new Set(tables.map(table => table.id));
    setExpandedTables(prev => {
      const next = prev.filter(id => ids.has(id));
      return next.length === prev.length ? prev : next;
    });
    if (selectedTableId && !ids.has(selectedTableId)) {
      setSelectedTableId(null);
    }
    if (isolatedTableId && !ids.has(isolatedTableId)) {
      setIsolatedTableId(null);
    }
  }, [tables, selectedTableId, isolatedTableId]);

  const expandedSet = useMemo(() => new Set(expandedTables), [expandedTables]);

  const isolationSet = useMemo(() => {
    if (!isolatedTableId) return null;
    const set = new Set<string>([isolatedTableId]);
    edges.forEach(edge => {
      if (edge.fromId === isolatedTableId || edge.toId === isolatedTableId) {
        set.add(edge.fromId);
        set.add(edge.toId);
      }
    });
    return set;
  }, [isolatedTableId, edges]);

  const visibleTables = useMemo(
    () => (isolationSet ? tables.filter(table => isolationSet.has(table.id)) : tables),
    [tables, isolationSet]
  );

  const visibleEdges = useMemo(
    () =>
      isolationSet
        ? edges.filter(edge => isolationSet.has(edge.fromId) && isolationSet.has(edge.toId))
        : edges,
    [edges, isolationSet]
  );

  const layout = useMemo(
    () => buildLayout(visibleTables, expandedSet),
    [visibleTables, expandedSet]
  );
  const nodesById = useMemo(
    () => new Map(layout.nodes.map(node => [node.id, node])),
    [layout.nodes]
  );
  const edgePaths = useMemo(
    () => buildEdgePaths(visibleEdges, nodesById),
    [visibleEdges, nodesById]
  );
  const hoveredEdgeData = useMemo(
    () => edgePaths.find(item => item.id === hoveredEdge) ?? null,
    [edgePaths, hoveredEdge]
  );

  const searchValue = search.trim().toLowerCase();
  const matchedNodes = useMemo(
    () => layout.nodes.filter(node => node.name.toLowerCase().includes(searchValue)),
    [layout.nodes, searchValue]
  );
  const primaryMatch = matchedNodes[0];

  const fitToView = useCallback(() => {
    if (!viewportRef.current || layout.nodes.length === 0) return;
    const rect = viewportRef.current.getBoundingClientRect();
    const padding = 80;
    const scaleX = (rect.width - padding * 2) / layout.width;
    const scaleY = (rect.height - padding * 2) / layout.height;
    const nextZoom = Math.min(Math.max(Math.min(scaleX, scaleY), MIN_ZOOM), MAX_ZOOM);
    const nextPan = {
      x: (rect.width - layout.width * nextZoom) / 2,
      y: (rect.height - layout.height * nextZoom) / 2,
    };
    setZoom(nextZoom);
    setPan(nextPan);
  }, [layout]);

  const resetView = useCallback(() => {
    setZoom(1);
    setPan({ x: 0, y: 0 });
  }, []);

  const focusOnTable = useCallback(
    (tableId: string) => {
      if (!viewportRef.current) return;
      const node = nodesById.get(tableId);
      if (!node) return;
      const rect = viewportRef.current.getBoundingClientRect();
      const centerX = node.x + node.width / 2;
      const centerY = node.y + node.height / 2;
      setPan({
        x: rect.width / 2 - centerX * zoom,
        y: rect.height / 2 - centerY * zoom,
      });
    },
    [nodesById, zoom]
  );

  useEffect(() => {
    if (!hasFit && layout.nodes.length > 0) {
      fitToView();
      setHasFit(true);
    }
  }, [fitToView, hasFit, layout.nodes.length]);

  const handlePointerDown = useCallback(
    (event: PointerEvent<HTMLDivElement>) => {
      if ((event.target as HTMLElement).closest('[data-node]')) return;
      isPanningRef.current = true;
      pointerRef.current = {
        x: event.clientX,
        y: event.clientY,
        panX: pan.x,
        panY: pan.y,
      };
      event.currentTarget.setPointerCapture(event.pointerId);
    },
    [pan]
  );

  const handlePointerMove = useCallback((event: PointerEvent<HTMLDivElement>) => {
    if (!isPanningRef.current || !pointerRef.current) return;
    const deltaX = event.clientX - pointerRef.current.x;
    const deltaY = event.clientY - pointerRef.current.y;
    setPan({
      x: pointerRef.current.panX + deltaX,
      y: pointerRef.current.panY + deltaY,
    });
  }, []);

  const handlePointerUp = useCallback((event: PointerEvent<HTMLDivElement>) => {
    if (!isPanningRef.current) return;
    isPanningRef.current = false;
    pointerRef.current = null;
    event.currentTarget.releasePointerCapture(event.pointerId);
  }, []);

  const handleWheel = useCallback(
    (event: WheelEvent<HTMLDivElement>) => {
      if (!viewportRef.current) return;
      event.preventDefault();

      if (event.ctrlKey || event.metaKey) {
        const nextZoom = Math.min(Math.max(zoom - event.deltaY * 0.001, MIN_ZOOM), MAX_ZOOM);
        const rect = viewportRef.current.getBoundingClientRect();
        const offsetX = event.clientX - rect.left;
        const offsetY = event.clientY - rect.top;
        const scale = nextZoom / zoom;

        setPan(prev => ({
          x: offsetX - (offsetX - prev.x) * scale,
          y: offsetY - (offsetY - prev.y) * scale,
        }));
        setZoom(nextZoom);
      } else {
        setPan(prev => ({ x: prev.x - event.deltaX, y: prev.y - event.deltaY }));
      }
    },
    [zoom]
  );

  const zoomIn = useCallback(() => {
    setZoom(current => Math.min(current + 0.1, MAX_ZOOM));
  }, []);

  const zoomOut = useCallback(() => {
    setZoom(current => Math.max(current - 0.1, MIN_ZOOM));
  }, []);

  const toggleTableExpanded = useCallback((tableId: string) => {
    setExpandedTables(prev => {
      if (prev.includes(tableId)) {
        return prev.filter(id => id !== tableId);
      }
      return [...prev, tableId];
    });
  }, []);

  const isolateTable = useCallback((tableId: string) => {
    setSelectedTableId(tableId);
    setIsolatedTableId(tableId);
    setHasFit(false);
  }, []);

  const clearIsolation = useCallback(() => {
    setIsolatedTableId(null);
    setHasFit(false);
  }, []);

  const isolatedTableName = useMemo(() => {
    if (!isolatedTableId) return null;
    return tables.find(table => table.id === isolatedTableId)?.name ?? null;
  }, [isolatedTableId, tables]);

  const isolateCandidateId = selectedTableId ?? primaryMatch?.id ?? null;

  const diagramEmpty = !loading && tables.length === 0 && !error;

  return (
    <div className="flex h-full flex-col rounded-md border border-border bg-muted/10">
      <div className="flex flex-wrap items-center gap-2 border-b border-border bg-muted/20 px-3 py-2">
        <div className="relative w-64">
          <Search className="absolute left-2.5 top-2.5 h-4 w-4 text-muted-foreground" />
          <Input
            value={search}
            onChange={event => setSearch(event.target.value)}
            onKeyDown={event => {
              if (event.key === 'Enter' && primaryMatch) {
                focusOnTable(primaryMatch.id);
              }
            }}
            placeholder={t('databaseBrowser.searchTables')}
            className="pl-9 h-9"
          />
        </div>
        <div className="flex items-center gap-1">
          <Button
            variant="ghost"
            size="icon"
            className="h-8 w-8"
            onClick={zoomOut}
            title={t('databaseBrowser.diagramZoomOut')}
          >
            <ZoomOut size={16} />
          </Button>
          <Button
            variant="ghost"
            size="icon"
            className="h-8 w-8"
            onClick={zoomIn}
            title={t('databaseBrowser.diagramZoomIn')}
          >
            <ZoomIn size={16} />
          </Button>
          <div className="text-xs text-muted-foreground w-12 text-center">
            {Math.round(zoom * 100)}%
          </div>
          <Button variant="outline" size="sm" className="h-8 gap-2" onClick={fitToView}>
            {t('databaseBrowser.diagramFit')}
          </Button>
          <Button variant="outline" size="sm" className="h-8 gap-2" onClick={resetView}>
            {t('databaseBrowser.diagramReset')}
          </Button>
        </div>
        <div className="flex items-center gap-2 ml-auto">
          {isolatedTableId ? (
            <div className="flex items-center gap-2">
              <span className="text-xs text-muted-foreground">
                {t('databaseBrowser.diagramIsolated', {
                  name: isolatedTableName ?? t('databaseBrowser.diagramUnknownTable'),
                })}
              </span>
              <Button variant="outline" size="sm" className="h-8" onClick={clearIsolation}>
                {t('databaseBrowser.diagramShowAll')}
              </Button>
            </div>
          ) : (
            <Button
              variant="outline"
              size="sm"
              className="h-8 gap-2"
              onClick={() => isolateCandidateId && isolateTable(isolateCandidateId)}
              disabled={!isolateCandidateId}
            >
              <Filter size={14} />
              {t('databaseBrowser.diagramIsolate')}
            </Button>
          )}
          {primaryMatch && (
            <Button
              variant="outline"
              size="sm"
              className="h-8 gap-2"
              onClick={() => focusOnTable(primaryMatch.id)}
            >
              <Crosshair size={14} />
              {t('databaseBrowser.diagramFocus')}
            </Button>
          )}
          <Button
            variant="outline"
            size="sm"
            className="h-8 gap-2"
            onClick={() => loadSchema(true)}
          >
            <RefreshCw size={14} />
            {t('databaseBrowser.diagramRefresh')}
          </Button>
        </div>
      </div>

      {loading ? (
        <div className="flex flex-1 flex-col items-center justify-center gap-2 text-muted-foreground">
          <Loader2 size={20} className="animate-spin" />
          <div className="text-sm">{t('common.loading')}</div>
          {progress.total > 0 && (
            <div className="text-xs text-muted-foreground">
              {progress.loaded}/{progress.total}
            </div>
          )}
        </div>
      ) : error ? (
        <div className="flex flex-1 items-center justify-center">
          <div className="flex items-center gap-3 rounded-md border border-error/20 bg-error/10 px-4 py-3 text-error">
            {error}
          </div>
        </div>
      ) : diagramEmpty ? (
        <div className="flex flex-1 items-center justify-center text-sm text-muted-foreground">
          {t('databaseBrowser.diagramEmpty')}
        </div>
      ) : (
        <div className="relative flex-1 overflow-hidden">
          <div
            ref={viewportRef}
            className="absolute inset-0 select-none"
            style={{
              touchAction: 'none',
              backgroundImage: 'radial-gradient(var(--q-border) 1px, transparent 1px)',
              backgroundSize: '24px 24px',
              backgroundPosition: '0 0',
            }}
            onPointerDown={handlePointerDown}
            onPointerMove={handlePointerMove}
            onPointerUp={handlePointerUp}
            onPointerLeave={handlePointerUp}
            onWheel={handleWheel}
          >
            <div
              className="absolute left-0 top-0"
              style={{
                width: layout.width,
                height: layout.height,
                transform: `translate(${pan.x}px, ${pan.y}px) scale(${zoom})`,
                transformOrigin: '0 0',
              }}
            >
              <svg className="absolute left-0 top-0" width={layout.width} height={layout.height}>
                {edgePaths.map(edge => {
                  const isActive =
                    hoveredEdge === edge.id ||
                    (hoveredTable && (edge.fromId === hoveredTable || edge.toId === hoveredTable));
                  return (
                    <path
                      key={edge.id}
                      d={edge.path}
                      stroke="currentColor"
                      fill="none"
                      className={cn(
                        'transition-colors',
                        isActive ? 'text-accent' : 'text-muted-foreground/40'
                      )}
                      strokeWidth={isActive ? 2 : 1.2}
                      onMouseEnter={() => setHoveredEdge(edge.id)}
                      onMouseLeave={() => setHoveredEdge(null)}
                    />
                  );
                })}
              </svg>

              {layout.nodes.map(node => {
                const isMatch = !searchValue || node.name.toLowerCase().includes(searchValue);
                const isExpanded = expandedSet.has(node.id);
                const isSelected = selectedTableId === node.id;
                const isActive =
                  hoveredTable === node.id ||
                  (hoveredEdgeData &&
                    (hoveredEdgeData.fromId === node.id || hoveredEdgeData.toId === node.id));
                const isHighlighted = isActive || isSelected;

                return (
                  <div
                    key={node.id}
                    data-node
                    tabIndex={0}
                    onMouseEnter={() => setHoveredTable(node.id)}
                    onMouseLeave={() => setHoveredTable(null)}
                    onFocus={() => {
                      setHoveredTable(node.id);
                      setSelectedTableId(node.id);
                    }}
                    onBlur={() => setHoveredTable(null)}
                    onClick={event => {
                      setSelectedTableId(node.id);
                      if (event.altKey) {
                        isolateTable(node.id);
                        return;
                      }
                      onTableSelect(namespace, node.name);
                    }}
                    onKeyDown={event => {
                      if (event.key === 'Enter') {
                        onTableSelect(namespace, node.name);
                      }
                    }}
                    className={cn(
                      'absolute rounded-md border bg-background shadow-sm transition-colors outline-none',
                      'focus-visible:ring-2 focus-visible:ring-accent focus-visible:ring-offset-2 focus-visible:ring-offset-background',
                      isHighlighted ? 'border-accent' : 'border-border',
                      isMatch ? 'opacity-100' : 'opacity-30'
                    )}
                    style={{
                      left: node.x,
                      top: node.y,
                      width: node.width,
                      height: node.height,
                    }}
                  >
                    <div className="flex items-center justify-between border-b border-border px-3 py-2">
                      <div className="flex items-center gap-2">
                        <TableIcon size={14} className="text-muted-foreground" />
                        <span className="font-mono text-sm text-foreground truncate max-w-35">
                          {node.name}
                        </span>
                      </div>
                      <div className="flex items-center gap-1">
                        <button
                          type="button"
                          onClick={event => {
                            event.stopPropagation();
                            isolateTable(node.id);
                          }}
                          title={t('databaseBrowser.diagramIsolate')}
                          className="h-6 w-6 inline-flex items-center justify-center rounded-md text-muted-foreground hover:text-foreground hover:bg-muted"
                        >
                          <Filter size={12} />
                        </button>
                        <span className="text-xs text-muted-foreground">
                          {node.schema.columns?.length ?? 0}
                        </span>
                      </div>
                    </div>
                    <div className="px-3 py-2 space-y-1">
                      {node.displayColumns.length === 0 ? (
                        <div className="text-xs text-muted-foreground">-</div>
                      ) : (
                        node.displayColumns.map(col => (
                          <div key={col.name} className="flex items-center justify-between gap-2">
                            <div className="flex items-center gap-2 min-w-0">
                              <span
                                className={cn(
                                  'h-1.5 w-1.5 rounded-full',
                                  col.isPrimary
                                    ? 'bg-accent'
                                    : col.isForeign
                                      ? 'bg-info'
                                      : 'bg-muted-foreground/30'
                                )}
                              />
                              <span
                                className={cn(
                                  'text-xs font-mono truncate',
                                  col.isPrimary
                                    ? 'text-accent'
                                    : col.isForeign
                                      ? 'text-info'
                                      : 'text-foreground'
                                )}
                              >
                                {col.name}
                              </span>
                            </div>
                            <span className="text-[10px] text-muted-foreground truncate max-w-22">
                              {col.data_type}
                            </span>
                          </div>
                        ))
                      )}
                      {(node.overflowCount > 0 ||
                        (isExpanded && node.columns.length > MAX_COLUMNS)) && (
                        <button
                          type="button"
                          className="text-[11px] text-muted-foreground hover:text-foreground"
                          onClick={event => {
                            event.stopPropagation();
                            toggleTableExpanded(node.id);
                          }}
                        >
                          {isExpanded
                            ? t('databaseBrowser.diagramHideColumns')
                            : t('databaseBrowser.diagramShowColumns', {
                                count: node.overflowCount,
                              })}
                        </button>
                      )}
                    </div>
                  </div>
                );
              })}

              {hoveredEdgeData && (
                <div
                  className="absolute rounded-md border border-border bg-background px-2 py-1 text-[11px] text-muted-foreground shadow-sm"
                  style={{
                    left: hoveredEdgeData.midX,
                    top: hoveredEdgeData.midY,
                    transform: 'translate(-50%, -120%)',
                  }}
                >
                  <span className="font-mono text-foreground">
                    {hoveredEdgeData.fromTable}.{hoveredEdgeData.fromColumn}
                  </span>
                  {' -> '}
                  <span className="font-mono text-foreground">
                    {hoveredEdgeData.toTable}.{hoveredEdgeData.toColumn}
                  </span>
                  {hoveredEdgeData.constraint && (
                    <span className="ml-2 text-[10px] text-muted-foreground">
                      {hoveredEdgeData.constraint}
                    </span>
                  )}
                </div>
              )}
            </div>
          </div>

          {visibleEdges.length === 0 && (
            <div className="absolute left-4 top-4 rounded-md border border-border bg-background/80 px-3 py-2 text-xs text-muted-foreground shadow-sm">
              {t('databaseBrowser.diagramNoRelations')}
            </div>
          )}
        </div>
      )}
    </div>
  );
}
