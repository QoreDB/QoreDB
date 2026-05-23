// SPDX-License-Identifier: Apache-2.0

/**
 * Renders a result cell through a plugin-contributed viewer. Only `json-tree`
 * and `image` are wired in Phase 4 — `map` and `chart` parse in the manifest
 * but fall back to default rendering until their dedicated UIs land.
 */

import { ImageOff } from 'lucide-react';
import { memo } from 'react';
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
    case 'map':
    case 'chart':
      return <span className="block truncate">{formatted}</span>;
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
