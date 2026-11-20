// SPDX-License-Identifier: Apache-2.0

import { ChevronDown, ChevronRight } from 'lucide-react';
import { useMemo, useState } from 'react';
import { useTranslation } from 'react-i18next';
import {
  Bar,
  BarChart,
  CartesianGrid,
  Line,
  LineChart,
  Pie,
  PieChart,
  Cell as RechartsCell,
  ResponsiveContainer,
  Scatter,
  ScatterChart,
  Tooltip,
  XAxis,
  YAxis,
} from 'recharts';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select';
import { buildChartPoints, numericColumnNames } from '@/lib/notebook/chartData';
import type { ChartConfig, NotebookCell as NotebookCellType } from '@/lib/notebook/notebookTypes';

interface ChartCellProps {
  cell: NotebookCellType;
  allCells: NotebookCellType[];
  onChartConfigChange?: (chartConfig: ChartConfig) => void;
}

const CHART_TYPES: ChartConfig['type'][] = ['bar', 'line', 'pie', 'scatter'];

const COLORS = [
  'var(--color-accent)',
  '#22C55E',
  '#F59E0B',
  '#3B82F6',
  '#EF4444',
  '#8B5CF6',
  '#EC4899',
];

export function ChartCell({ cell, allCells, onChartConfigChange }: ChartCellProps) {
  const { t } = useTranslation();
  const config = cell.config?.chartConfig;

  const labeledCells = useMemo(() => {
    const counts = new Map<string, number>();
    for (const candidate of allCells) {
      const label = candidate.config?.label;
      if ((candidate.type === 'sql' || candidate.type === 'mongo') && label) {
        counts.set(label, (counts.get(label) ?? 0) + 1);
      }
    }
    return [...counts.entries()].filter(([, count]) => count === 1).map(([label]) => label);
  }, [allCells]);

  const sourceCell = useMemo(() => {
    if (!config?.sourceLabel) return undefined;
    const matches = allCells.filter(c => c.config?.label === config.sourceLabel);
    return matches.length === 1 ? matches[0] : undefined;
  }, [allCells, config?.sourceLabel]);

  const sourceTable =
    sourceCell?.lastResult?.type === 'table' && sourceCell.lastResult.columns
      ? { columns: sourceCell.lastResult.columns, rows: sourceCell.lastResult.rows ?? [] }
      : null;
  const sourceReady = sourceCell?.executionState === 'success' && sourceTable !== null;
  const availableColumns = sourceTable
    ? sourceTable.columns.filter(c => !c.masked).map(c => c.name)
    : [];
  const numericColumns = sourceTable
    ? numericColumnNames(sourceTable.columns, sourceTable.rows)
    : [];

  const data =
    config?.xColumn && config.yColumns.length > 0 && sourceReady && sourceTable
      ? buildChartPoints(sourceTable.columns, sourceTable.rows, config.xColumn, config.yColumns)
      : [];

  const activeConfig: ChartConfig = config ?? {
    sourceLabel: '',
    type: 'bar',
    xColumn: '',
    yColumns: [],
  };

  const updateConfig = (patch: Partial<ChartConfig>) => {
    if (!onChartConfigChange) return;
    onChartConfigChange({ ...activeConfig, ...patch });
  };

  const handleSourceChange = (nextSource: string) => {
    const nextCell = allCells.find(c => c.config?.label === nextSource);
    const result = nextCell?.lastResult?.type === 'table' ? nextCell.lastResult : null;
    const cols = result?.columns ? result.columns.filter(c => !c.masked).map(c => c.name) : [];
    const numeric = result?.columns ? numericColumnNames(result.columns, result.rows ?? []) : [];
    const nextX = cols.includes(activeConfig.xColumn) ? activeConfig.xColumn : (cols[0] ?? '');
    const keptY = activeConfig.yColumns.filter(col => numeric.includes(col));
    const firstY = numeric.find(col => col !== nextX);
    const nextY = keptY.length > 0 ? keptY : firstY ? [firstY] : [];
    updateConfig({ sourceLabel: nextSource, xColumn: nextX, yColumns: nextY });
  };

  const toggleYColumn = (col: string) => {
    if (activeConfig.type === 'pie' || activeConfig.type === 'scatter') {
      updateConfig({ yColumns: [col] });
      return;
    }
    const exists = activeConfig.yColumns.includes(col);
    const nextY = exists
      ? activeConfig.yColumns.filter(item => item !== col)
      : [...activeConfig.yColumns, col];
    updateConfig({ yColumns: nextY });
  };

  const isConfigured =
    Boolean(config?.sourceLabel) &&
    Boolean(config?.xColumn) &&
    Boolean(config?.yColumns && config.yColumns.length > 0);
  const [configOpen, setConfigOpen] = useState(!isConfigured);

  return (
    <div className="space-y-2">
      {onChartConfigChange && (
        <Button
          type="button"
          variant="ghost"
          size="sm"
          className="h-6 gap-1 px-1.5 text-[11px] text-muted-foreground"
          aria-expanded={configOpen}
          onClick={() => setConfigOpen(open => !open)}
        >
          {configOpen ? <ChevronDown size={12} /> : <ChevronRight size={12} />}
          {configOpen
            ? t('notebook.chartHideConfig')
            : isConfigured && config
              ? t('notebook.chartConfigSummary', {
                  source: config.sourceLabel,
                  x: config.xColumn,
                  y: config.yColumns.join(', '),
                })
              : t('notebook.chartShowConfig')}
        </Button>
      )}
      {onChartConfigChange && configOpen && (
        <div className="flex flex-wrap items-center gap-2 rounded-md border border-border/60 bg-muted/20 p-2 text-xs">
          <Select value={activeConfig.sourceLabel || undefined} onValueChange={handleSourceChange}>
            <SelectTrigger className="h-7 w-36 text-xs">
              <SelectValue placeholder={t('notebook.chartSourcePlaceholder')} />
            </SelectTrigger>
            <SelectContent>
              {labeledCells.map(label => (
                <SelectItem key={label} value={label} className="text-xs font-mono">
                  #{label}
                </SelectItem>
              ))}
            </SelectContent>
          </Select>

          <Select
            value={activeConfig.type}
            onValueChange={nextType => {
              const chartType = nextType as ChartConfig['type'];
              const nextY =
                (chartType === 'pie' || chartType === 'scatter') && activeConfig.yColumns.length > 1
                  ? [activeConfig.yColumns[0]]
                  : activeConfig.yColumns;
              updateConfig({ type: chartType, yColumns: nextY });
            }}
          >
            <SelectTrigger className="h-7 w-28 text-xs">
              <SelectValue />
            </SelectTrigger>
            <SelectContent>
              {CHART_TYPES.map(chartType => (
                <SelectItem key={chartType} value={chartType} className="text-xs capitalize">
                  {t(`notebook.chartTypes.${chartType}`)}
                </SelectItem>
              ))}
            </SelectContent>
          </Select>

          <Select
            value={activeConfig.xColumn || undefined}
            onValueChange={nextX => updateConfig({ xColumn: nextX })}
            disabled={availableColumns.length === 0}
          >
            <SelectTrigger className="h-7 w-36 text-xs">
              <SelectValue placeholder={t('notebook.chartXColumn')} />
            </SelectTrigger>
            <SelectContent>
              {availableColumns.map(col => (
                <SelectItem key={col} value={col} className="text-xs font-mono">
                  {col}
                </SelectItem>
              ))}
            </SelectContent>
          </Select>

          <Input
            value={activeConfig.title ?? ''}
            onChange={e => updateConfig({ title: e.target.value || undefined })}
            placeholder={t('notebook.chartTitlePlaceholder')}
            className="h-7 w-40 text-xs"
          />

          {availableColumns.length > 0 && (
            <div className="flex flex-wrap items-center gap-1">
              <span className="text-[11px] text-muted-foreground mr-1">
                {t('notebook.chartYColumns')}:
              </span>
              {numericColumns.length === 0 && (
                <span className="text-[11px] text-muted-foreground italic">
                  {t('notebook.chartNoNumericColumns')}
                </span>
              )}
              {numericColumns.map(col => {
                const selected = activeConfig.yColumns.includes(col);
                return (
                  <Button
                    key={col}
                    type="button"
                    size="sm"
                    variant={selected ? 'secondary' : 'outline'}
                    className="h-6 px-2 text-[11px] font-mono"
                    onClick={() => toggleYColumn(col)}
                  >
                    {col}
                  </Button>
                );
              })}
            </div>
          )}
        </div>
      )}

      {!isConfigured ? (
        <div className="p-4 text-sm text-muted-foreground italic border border-dashed border-border rounded-md">
          {labeledCells.length === 0
            ? t('notebook.chartNeedLabelledCell')
            : t('notebook.chartNoConfig')}
        </div>
      ) : data.length === 0 ? (
        <div className="p-4 text-sm text-muted-foreground italic border border-dashed border-border rounded-md">
          {sourceReady
            ? t('notebook.chartNoData')
            : t('notebook.chartSourceNotRun', { label: config?.sourceLabel })}
        </div>
      ) : (
        <div className="border border-border rounded-md p-2">
          {config?.title && (
            <div className="text-sm font-medium text-center mb-2">{config.title}</div>
          )}
          <ResponsiveContainer width="100%" height={250}>
            {renderChart(config as ChartConfig, data)}
          </ResponsiveContainer>
        </div>
      )}
    </div>
  );
}

function renderChart(config: ChartConfig, data: Record<string, unknown>[]) {
  switch (config.type) {
    case 'bar':
      return (
        <BarChart data={data}>
          <CartesianGrid strokeDasharray="3 3" />
          <XAxis dataKey={config.xColumn} tick={{ fontSize: 11 }} />
          <YAxis tick={{ fontSize: 11 }} />
          <Tooltip />
          {config.yColumns.map((col, i) => (
            <Bar key={col} dataKey={col} fill={COLORS[i % COLORS.length]} />
          ))}
        </BarChart>
      );
    case 'line':
      return (
        <LineChart data={data}>
          <CartesianGrid strokeDasharray="3 3" />
          <XAxis dataKey={config.xColumn} tick={{ fontSize: 11 }} />
          <YAxis tick={{ fontSize: 11 }} />
          <Tooltip />
          {config.yColumns.map((col, i) => (
            <Line
              key={col}
              type="monotone"
              dataKey={col}
              stroke={COLORS[i % COLORS.length]}
              dot={false}
            />
          ))}
        </LineChart>
      );
    case 'pie':
      return (
        <PieChart>
          <Tooltip />
          <Pie
            data={data}
            dataKey={config.yColumns[0]}
            nameKey={config.xColumn}
            cx="50%"
            cy="50%"
            outerRadius={90}
            label
          >
            {data.map((_, i) => (
              <RechartsCell key={i} fill={COLORS[i % COLORS.length]} />
            ))}
          </Pie>
        </PieChart>
      );
    case 'scatter':
      return (
        <ScatterChart>
          <CartesianGrid strokeDasharray="3 3" />
          <XAxis dataKey={config.xColumn} tick={{ fontSize: 11 }} />
          <YAxis dataKey={config.yColumns[0]} tick={{ fontSize: 11 }} />
          <Tooltip />
          <Scatter data={data} fill={COLORS[0]} />
        </ScatterChart>
      );
  }
}
