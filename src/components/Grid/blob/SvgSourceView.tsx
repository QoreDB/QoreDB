// SPDX-License-Identifier: Apache-2.0

import { useMemo } from 'react';
import { ScrollArea } from '@/components/ui/scroll-area';
import { decodeBase64AsText } from '@/lib/binaryUtils';

interface SvgSourceViewProps {
  base64: string;
}

export function SvgSourceView({ base64 }: SvgSourceViewProps) {
  const source = useMemo(() => decodeBase64AsText(base64), [base64]);

  return (
    <ScrollArea className="h-[400px] rounded-md border border-border bg-muted/20">
      <pre className="font-mono text-xs leading-5 p-3 select-text whitespace-pre-wrap break-all">
        {source}
      </pre>
    </ScrollArea>
  );
}
