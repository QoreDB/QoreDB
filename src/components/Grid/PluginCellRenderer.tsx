// SPDX-License-Identifier: Apache-2.0

/**
 * Renders a result cell through a plugin-contributed viewer.
 *
 * Built-in renderers:
 *   - `json-tree`: pretty-print a JSON value.
 *   - `image`: render data-URLs and trusted https image URLs.
 *   - `chart`: tiny recharts visual for `{type, data}` payloads.
 *   - `color`: swatch for hex / rgb(a) color strings.
 *   - `boolean`: colored pill for boolean-ish values.
 *   - `bytes`: humanized size for a numeric byte count.
 *   - `map`: explicit "renderer not available" fallback — QoreDB does not
 *     bundle a map library, so a viewer contributing `renderer: "map"` is
 *     parsed by the manifest but cannot draw a map here.
 */

import { Check, ImageOff, MapPinOff, X } from 'lucide-react';
import { memo, useMemo } from 'react';
import { Area, AreaChart, Bar, BarChart, Line, LineChart, ResponsiveContainer } from 'recharts';
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
    case 'color':
      return <ColorCell value={value} formatted={formatted} />;
    case 'boolean':
      return <BooleanCell value={value} formatted={formatted} />;
    case 'bytes':
      return <BytesCell value={value} formatted={formatted} />;
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
    <img src={value} alt="" className="max-h-16 max-w-full rounded object-contain" loading="lazy" />
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

/** Hex (`#rgb`, `#rrggbb`, `#rrggbbaa`) or `rgb()/rgba()`/`hsl()/hsla()`. */
const COLOR_RE = /^(#(?:[0-9a-f]{3,4}|[0-9a-f]{6}|[0-9a-f]{8})|(?:rgb|hsl)a?\([^)]+\))$/i;

function ColorCell({ value, formatted }: { value: unknown; formatted: string }) {
  const color = typeof value === 'string' ? value.trim() : '';
  if (!COLOR_RE.test(color)) {
    return <span className="block truncate text-muted-foreground">{formatted}</span>;
  }
  return (
    <span className="inline-flex items-center gap-1.5">
      <span
        className="inline-block size-3.5 shrink-0 rounded border border-border"
        style={{ backgroundColor: color }}
      />
      <span className="font-mono text-foreground">{formatted}</span>
    </span>
  );
}

/** Truthy / falsy tokens accepted from string and numeric columns. */
const TRUE_TOKENS = new Set(['true', 't', 'yes', 'y', 'on', '1']);
const FALSE_TOKENS = new Set(['false', 'f', 'no', 'n', 'off', '0']);

/** Coerces a cell value to a boolean, or `null` when it isn't boolean-ish. */
function coerceBoolean(value: unknown): boolean | null {
  if (typeof value === 'boolean') return value;
  if (typeof value === 'number') return value === 1 ? true : value === 0 ? false : null;
  if (typeof value === 'string') {
    const token = value.trim().toLowerCase();
    if (TRUE_TOKENS.has(token)) return true;
    if (FALSE_TOKENS.has(token)) return false;
  }
  return null;
}

function BooleanCell({ value, formatted }: { value: unknown; formatted: string }) {
  const bool = coerceBoolean(value);
  if (bool === null) {
    return <span className="block truncate text-muted-foreground">{formatted}</span>;
  }
  return (
    <span
      className={`inline-flex items-center gap-1 rounded-full px-1.5 py-0.5 text-[10.5px] font-medium ${
        bool ? 'bg-success/15 text-success' : 'bg-muted text-muted-foreground'
      }`}
    >
      {bool ? <Check size={11} /> : <X size={11} />}
      {formatted}
    </span>
  );
}

const BYTE_UNITS = ['B', 'KB', 'MB', 'GB', 'TB', 'PB'];

/** Humanizes a byte count (1024-based). Returns `null` for non-finite input. */
function humanizeBytes(bytes: number): string | null {
  if (!Number.isFinite(bytes)) return null;
  const sign = bytes < 0 ? '-' : '';
  let n = Math.abs(bytes);
  let unit = 0;
  while (n >= 1024 && unit < BYTE_UNITS.length - 1) {
    n /= 1024;
    unit += 1;
  }
  const rounded = unit === 0 ? n : Math.round(n * 100) / 100;
  return `${sign}${rounded} ${BYTE_UNITS[unit]}`;
}

function BytesCell({ value, formatted }: { value: unknown; formatted: string }) {
  const raw =
    typeof value === 'string' ? (value.trim() === '' ? null : Number(value.trim())) : value;
  const size = typeof raw === 'number' ? humanizeBytes(raw) : null;
  if (size === null) {
    return <span className="block truncate text-muted-foreground">{formatted}</span>;
  }
  return <span className="font-mono tabular-nums text-foreground">{size}</span>;
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
