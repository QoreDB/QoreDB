// SPDX-License-Identifier: Apache-2.0

import { AlertCircle } from 'lucide-react';

interface CellErrorViewerProps {
  error: string;
}

export function CellErrorViewer({ error }: CellErrorViewerProps) {
  return (
    <div className="flex items-start gap-2 p-3 bg-destructive/10 border border-destructive/20 rounded-md text-sm mt-2">
      <AlertCircle className="text-destructive shrink-0 mt-0.5" size={16} />
      <pre className="whitespace-pre-wrap text-destructive font-mono text-xs">{error}</pre>
    </div>
  );
}
