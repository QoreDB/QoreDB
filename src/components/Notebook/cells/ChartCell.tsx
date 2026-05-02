// SPDX-License-Identifier: BUSL-1.1

import { useMemo } from 'react';
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
import type { ChartConfig, NotebookCell as NotebookCellType } from '@/lib/notebook/notebookTypes';

interface ChartCellProps {
  cell: NotebookCellType;
  allCells: NotebookCellType[];
}

const COLORS = ['#6B5CFF', '#22C55E', '#F59E0B', '#3B82F6', '#EF4444', '#8B5CF6', '#EC4899'];

export function ChartCell({ cell, allCells }: ChartCellProps) {
  const { t } = useTranslation();
  const config = cell.config?.chartConfig;

  const data = useMemo(() => {
    if (!config) return [];
    const sourceCell = allCells.find(c => c.config?.label === config.sourceLabel);
    if (!sourceCell?.lastResult || sourceCell.lastResult.type !== 'table') return [];
    const { columns, rows } = sourceCell.lastResult;
    if (!columns || !rows) return [];

    const xIdx = columns.findIndex(c => c.name === config.xColumn);
    const yIdxs = config.yColumns.map(y => columns.findIndex(c => c.name === y));

    return rows.map(row => {
      const point: Record<string, unknown> = {
        [config.xColumn]: row.values[xIdx],
      };
      for (let i = 0; i < config.yColumns.length; i++) {
        const idx = yIdxs[i];
        if (idx >= 0) point[config.yColumns[i]] = Number(row.values[idx]) || 0;
      }
      return point;
    });
  }, [config, allCells]);

  if (!config) {
    return (
      <div className="p-4 text-sm text-muted-foreground italic border border-dashed border-border rounded-md">
        {t('notebook.chartNoConfig')}
      </div>
    );
  }

  if (data.length === 0) {
    return (
      <div className="p-4 text-sm text-muted-foreground italic border border-dashed border-border rounded-md">
        {t('notebook.chartNoData')}
      </div>
    );
  }

  return (
    <div className="border border-border rounded-md p-2">
      {config.title && <div className="text-sm font-medium text-center mb-2">{config.title}</div>}
      <ResponsiveContainer width="100%" height={250}>
        {renderChart(config, data)}
      </ResponsiveContainer>
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
