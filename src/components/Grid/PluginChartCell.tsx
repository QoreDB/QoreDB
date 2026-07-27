// SPDX-License-Identifier: Apache-2.0

import { Area, AreaChart, Bar, BarChart, Line, LineChart, ResponsiveContainer } from 'recharts';

export type ChartKind = 'bar' | 'line' | 'area';

interface PluginChartCellProps {
  kind: ChartKind;
  data: Array<Record<string, unknown>>;
}

export default function PluginChartCell({ kind, data }: PluginChartCellProps) {
  const sample = data[0];
  const valueKey =
    Object.keys(sample).find(key => key !== 'name' && typeof sample[key] === 'number') ?? 'value';

  return (
    <div className="h-12 w-full">
      <ResponsiveContainer width="100%" height="100%">
        {kind === 'line' ? (
          <LineChart data={data}>
            <Line
              type="monotone"
              dataKey={valueKey}
              stroke="currentColor"
              strokeWidth={1.5}
              dot={false}
            />
          </LineChart>
        ) : kind === 'area' ? (
          <AreaChart data={data}>
            <Area
              type="monotone"
              dataKey={valueKey}
              stroke="currentColor"
              fill="currentColor"
              fillOpacity={0.2}
            />
          </AreaChart>
        ) : (
          <BarChart data={data}>
            <Bar dataKey={valueKey} fill="currentColor" />
          </BarChart>
        )}
      </ResponsiveContainer>
    </div>
  );
}
