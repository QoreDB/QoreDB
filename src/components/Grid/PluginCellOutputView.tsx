// SPDX-License-Identifier: Apache-2.0

import { Badge } from '@/components/ui/badge';
import type { CellBadgeTone, CellOutput } from '@/lib/plugins';
import { cn } from '@/lib/utils';

const TONE_CLASSES: Record<CellBadgeTone, string> = {
  neutral: 'bg-muted text-foreground',
  info: 'bg-accent/15 text-accent border border-accent/30',
  success: 'bg-success/15 text-success border border-success/30',
  warning: 'bg-warning/15 text-warning border border-warning/30',
  error: 'bg-error/15 text-error border border-error/30',
};

function formatTableValue(value: unknown): string {
  if (value === null || value === undefined) return '';
  if (typeof value === 'string') return value;
  return JSON.stringify(value);
}

export function PluginCellOutputView({ output }: { output: CellOutput }) {
  switch (output.kind) {
    case 'text':
      return (
        <pre className="max-h-[60vh] overflow-auto whitespace-pre-wrap break-words rounded border border-border bg-muted/30 p-3 text-xs font-mono text-foreground">
          {output.text}
        </pre>
      );
    case 'badge':
      return (
        <Badge variant="secondary" className={cn('text-sm', TONE_CLASSES[output.tone])}>
          {output.label}
        </Badge>
      );
    case 'json':
      return (
        <pre className="max-h-[60vh] overflow-auto rounded border border-border bg-muted/30 p-3 text-xs font-mono text-foreground">
          {JSON.stringify(output.value, null, 2)}
        </pre>
      );
    case 'table':
      return (
        <div className="max-h-[60vh] overflow-auto rounded border border-border">
          <table className="w-full text-xs">
            <thead className="sticky top-0 bg-muted">
              <tr>
                {output.columns.map((column, index) => (
                  <th
                    // biome-ignore lint/suspicious/noArrayIndexKey: plugin column names may repeat
                    key={index}
                    scope="col"
                    className="px-2 py-1 text-left font-semibold text-muted-foreground"
                  >
                    {column}
                  </th>
                ))}
              </tr>
            </thead>
            <tbody>
              {output.rows.map((row, rowIndex) => (
                // biome-ignore lint/suspicious/noArrayIndexKey: rows are static plugin output
                <tr key={rowIndex} className="border-t border-border">
                  {row.map((value, cellIndex) => (
                    <td
                      // biome-ignore lint/suspicious/noArrayIndexKey: rows are static plugin output
                      key={cellIndex}
                      className="px-2 py-1 font-mono text-foreground"
                    >
                      {formatTableValue(value)}
                    </td>
                  ))}
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      );
  }
}
