// SPDX-License-Identifier: Apache-2.0

/**
 * Renders a result cell through a plugin-contributed viewer.
 *
 * Built-in renderers:
 *   - `json-tree`: pretty-print a JSON value.
 *   - `image`: render data-URLs and trusted https image URLs.
 *   - `chart`: tiny recharts visual for `{type, data}` payloads.
 *   - `map`: explicit "renderer not available" fallback — QoreDB does not
 *     bundle a map library, so a viewer contributing `renderer: "map"` is
 *     parsed by the manifest but cannot draw a map here.
 */

import { ImageOff, MapPinOff } from 'lucide-react';
import { memo, useMemo } from 'react';
import {
  Area,
  AreaChart,
  Bar,
  BarChart,
  Line,
  LineChart,
  ResponsiveContainer,
} from 'recharts';
import type { ResultViewerContribution } from '@/lib/plugins';

interface PluginCellRendererProps {
  viewer: ResultViewerContribution;
  value: unknown;
  formatted: string;
}

export const PluginCellRenderer = memo(function PluginCellRenderer({
  viewer,
  value,
  formatted,
}: PluginCellRendererProps) {
  switch (viewer.renderer) {
    case 'json-tree':
      return <JsonTreeCell value={value} formatted={formatted} />;
    case 'image':
      return <ImageCell value={value} formatted={formatted} />;
    case 'chart':
      return <ChartCell value={value} formatted={formatted} />;
    case 'map':
      return <MapFallbackCell formatted={formatted} />;
  }
});

function JsonTreeCell({ value, formatted }: { value: unknown; formatted: string }) {
  // The cell may receive an already-stringified JSON (most drivers serialise
  // JSON columns as strings) or a structured value. Re-parse when it's a
  // string so we can pretty-print, otherwise keep what we have.
  let pretty = formatted;
  if (typeof value === 'string') {
    try {
      pretty = JSON.stringify(JSON.parse(value), null, 2);
    } catch {
      pretty = value;
    }
  } else if (value !== null && typeof value === 'object') {
    try {
      pretty = JSON.stringify(value, null, 2);
    } catch {
      // Keep the default formatted string.
    }
  }
  return (
    <pre className="block max-h-32 overflow-auto rounded border border-border bg-muted/40 px-1.5 py-1 text-[10.5px] font-mono leading-tight text-foreground">
      {pretty}
    </pre>
  );
}

/** Accepts `data:image/...` URLs and `https?://*.{png,jpg,jpeg,gif,webp,svg}`.
 *  Anything else falls back to the default text so a misclassified cell
 *  doesn't break the layout. */
const IMAGE_URL_RE = /^(data:image\/[a-z+]+;base64,|https?:\/\/\S+\.(?:png|jpe?g|gif|webp|svg))/i;

function ImageCell({ value, formatted }: { value: unknown; formatted: string }) {
  if (typeof value !== 'string' || !IMAGE_URL_RE.test(value)) {
    return (
      <span className="inline-flex items-center gap-1 text-muted-foreground">
        <ImageOff size={12} />
        {formatted}
      </span>
    );
  }
  return (
    <img
      src={value}
      alt=""
      className="max-h-16 max-w-full rounded object-contain"
      loading="lazy"
    />
  );
}

type ChartKind = 'bar' | 'line' | 'area';

interface ChartPayload {
  type?: ChartKind;
  data?: Array<Record<string, unknown>>;
}

/** Parses a chart payload from the cell value: accepts an object directly or
 *  a JSON-encoded string. Returns `null` if the shape isn't usable so the
 *  cell can fall back to its text representation. */
interface ParsedChart {
  type?: ChartKind;
  data: Array<Record<string, unknown>>;
}

function parseChartPayload(value: unknown): ParsedChart | null {
  let parsed: unknown = value;
  if (typeof value === 'string') {
    try {
      parsed = JSON.parse(value);
    } catch {
      return null;
    }
  }
  if (!parsed || typeof parsed !== 'object') return null;
  const payload = parsed as ChartPayload;
  if (!Array.isArray(payload.data) || payload.data.length === 0) return null;
  return { type: payload.type, data: payload.data };
}

function ChartCell({ value, formatted }: { value: unknown; formatted: string }) {
  const payload = useMemo(() => parseChartPayload(value), [value]);
  if (!payload) {
    return <span className="block truncate text-muted-foreground">{formatted}</span>;
  }
  const kind: ChartKind = payload.type ?? 'bar';
  // Infer the numeric key from the first row so plugins don't have to commit
  // to a specific field name. `name` is conventional for the X axis.
  const sample = payload.data[0];
  const valueKey =
    Object.keys(sample).find(k => k !== 'name' && typeof sample[k] === 'number') ?? 'value';
  return (
    <div className="h-12 w-full">
      <ResponsiveContainer width="100%" height="100%">
        {kind === 'line' ? (
          <LineChart data={payload.data}>
            <Line
              type="monotone"
              dataKey={valueKey}
              stroke="currentColor"
              strokeWidth={1.5}
              dot={false}
            />
          </LineChart>
        ) : kind === 'area' ? (
          <AreaChart data={payload.data}>
            <Area
              type="monotone"
              dataKey={valueKey}
              stroke="currentColor"
              fill="currentColor"
              fillOpacity={0.2}
            />
          </AreaChart>
        ) : (
          <BarChart data={payload.data}>
            <Bar dataKey={valueKey} fill="currentColor" />
          </BarChart>
        )}
      </ResponsiveContainer>
    </div>
  );
}

function MapFallbackCell({ formatted }: { formatted: string }) {
  return (
    <span
      className="inline-flex items-center gap-1 text-muted-foreground"
      title="Map renderer is not bundled in this build."
    >
      <MapPinOff size={12} />
      {formatted}
    </span>
  );
}
