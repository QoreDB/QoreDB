// SPDX-License-Identifier: BUSL-1.1

import { Check, Copy, Terminal } from 'lucide-react';
import { useState } from 'react';
import { useTranslation } from 'react-i18next';
import { toast } from 'sonner';

import { Button } from '@/components/ui/button';
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog';

interface Props {
  open: boolean;
  onClose: () => void;
  /** Raw token shown once after endpoint creation. Cleared on close. */
  token: string;
  /** Endpoint URL (e.g. `http://127.0.0.1:4787/api/orders_top`). */
  url: string;
}

export function EndpointTokenDialog({ open, onClose, token, url }: Props) {
  const { t } = useTranslation();
  const [copiedKey, setCopiedKey] = useState<'token' | 'curl' | null>(null);

  const curlExample = `curl -H "Authorization: Bearer ${token}" "${url}"`;

  async function copy(value: string, key: 'token' | 'curl') {
    try {
      await navigator.clipboard.writeText(value);
      setCopiedKey(key);
      window.setTimeout(() => setCopiedKey(null), 1500);
    } catch (e) {
      toast.error(t('instantApi.token.copyFailed'), {
        description: e instanceof Error ? e.message : String(e),
      });
    }
  }

  return (
    <Dialog open={open} onOpenChange={v => !v && onClose()}>
      <DialogContent className="max-w-xl">
        <DialogHeader>
          <DialogTitle>{t('instantApi.token.title')}</DialogTitle>
          <DialogDescription>{t('instantApi.token.description')}</DialogDescription>
        </DialogHeader>

        <div className="space-y-4 py-2">
          <div>
            <div className="text-xs font-medium text-muted-foreground mb-1.5">
              {t('instantApi.token.tokenLabel')}
            </div>
            <div className="flex items-stretch gap-2">
              <code className="flex-1 min-w-0 truncate font-mono text-xs px-3 py-2 rounded border border-border bg-muted/30">
                {token}
              </code>
              <Button
                variant="outline"
                size="sm"
                onClick={() => copy(token, 'token')}
                className="shrink-0"
              >
                {copiedKey === 'token' ? <Check size={14} /> : <Copy size={14} />}
              </Button>
            </div>
          </div>

          <div>
            <div className="flex items-center gap-1.5 text-xs font-medium text-muted-foreground mb-1.5">
              <Terminal size={12} />
              {t('instantApi.token.curlLabel')}
            </div>
            <div className="flex items-stretch gap-2">
              <code className="flex-1 min-w-0 truncate font-mono text-xs px-3 py-2 rounded border border-border bg-muted/30">
                {curlExample}
              </code>
              <Button
                variant="outline"
                size="sm"
                onClick={() => copy(curlExample, 'curl')}
                className="shrink-0"
              >
                {copiedKey === 'curl' ? <Check size={14} /> : <Copy size={14} />}
              </Button>
            </div>
          </div>

          <div className="text-xs text-amber-700 dark:text-amber-400 px-3 py-2 rounded border border-amber-500/30 bg-amber-500/10">
            {t('instantApi.token.warning')}
          </div>
        </div>

        <DialogFooter>
          <Button onClick={onClose}>{t('instantApi.token.done')}</Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
