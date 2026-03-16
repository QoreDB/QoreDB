// SPDX-License-Identifier: Apache-2.0

import { AlertCircle, Copy } from 'lucide-react';
import { useTranslation } from 'react-i18next';
import { toast } from 'sonner';
import { Button } from '@/components/ui/button';

interface CellErrorViewerProps {
  error: string;
}

export function CellErrorViewer({ error }: CellErrorViewerProps) {
  const { t } = useTranslation();

  const handleCopy = () => {
    navigator.clipboard.writeText(error).then(() => {
      toast.success(t('notebook.copyError'));
    });
  };

  return (
    <div className="flex items-start gap-2 p-3 bg-destructive/10 border border-destructive/20 rounded-md text-sm mt-2">
      <AlertCircle className="text-destructive shrink-0 mt-0.5" size={16} />
      <pre className="whitespace-pre-wrap text-destructive font-mono text-xs flex-1">{error}</pre>
      <Button
        variant="ghost"
        size="icon"
        className="h-5 w-5 shrink-0 text-muted-foreground hover:text-foreground"
        title={t('notebook.copyError')}
        onClick={handleCopy}
      >
        <Copy size={12} />
      </Button>
    </div>
  );
}
