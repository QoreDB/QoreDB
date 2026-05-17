// SPDX-License-Identifier: BUSL-1.1

import { save } from '@tauri-apps/plugin-dialog';
import { writeTextFile } from '@tauri-apps/plugin-fs';
import { Check, Copy, Download } from 'lucide-react';
import { useEffect, useState } from 'react';
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
import { getOpenApiDocument } from '@/lib/instantApi';

interface Props {
  open: boolean;
  onClose: () => void;
}

export function OpenApiPreview({ open, onClose }: Props) {
  const { t } = useTranslation();
  const [doc, setDoc] = useState<string>('');
  const [loading, setLoading] = useState(false);
  const [copied, setCopied] = useState(false);

  useEffect(() => {
    if (!open) return;
    let cancelled = false;
    setLoading(true);
    setDoc('');
    setCopied(false);
    getOpenApiDocument()
      .then(json => {
        if (!cancelled) setDoc(json);
      })
      .catch(e => {
        if (cancelled) return;
        toast.error(t('instantApi.openapi.loadFailed'), {
          description: e instanceof Error ? e.message : String(e),
        });
      })
      .finally(() => {
        if (!cancelled) setLoading(false);
      });
    return () => {
      cancelled = true;
    };
  }, [open, t]);

  async function handleCopy() {
    try {
      await navigator.clipboard.writeText(doc);
      setCopied(true);
      window.setTimeout(() => setCopied(false), 1500);
    } catch (e) {
      toast.error(t('instantApi.token.copyFailed'), {
        description: e instanceof Error ? e.message : String(e),
      });
    }
  }

  async function handleDownload() {
    try {
      const path = await save({
        defaultPath: 'qoredb-openapi.json',
        filters: [{ name: 'JSON', extensions: ['json'] }],
      });
      if (!path) return;
      await writeTextFile(path, doc);
    } catch (e) {
      toast.error(t('instantApi.openapi.loadFailed'), {
        description: e instanceof Error ? e.message : String(e),
      });
    }
  }

  return (
    <Dialog open={open} onOpenChange={v => !v && onClose()}>
      <DialogContent className="max-w-3xl max-h-[85vh] flex flex-col">
        <DialogHeader>
          <DialogTitle>{t('instantApi.openapi.title')}</DialogTitle>
          <DialogDescription>{t('instantApi.openapi.description')}</DialogDescription>
        </DialogHeader>

        <div className="flex-1 min-h-0 overflow-auto rounded-md border border-border bg-muted/30">
          <pre className="text-[11px] font-mono leading-relaxed p-3 whitespace-pre">
            {loading ? '…' : doc}
          </pre>
        </div>

        <DialogFooter className="flex flex-row items-center justify-between sm:justify-between">
          <div className="flex gap-2">
            <Button variant="outline" size="sm" onClick={handleCopy} disabled={!doc || loading}>
              {copied ? <Check size={13} /> : <Copy size={13} />}
              {copied ? t('instantApi.openapi.copied') : t('instantApi.openapi.copy')}
            </Button>
            <Button variant="outline" size="sm" onClick={handleDownload} disabled={!doc || loading}>
              <Download size={13} />
              {t('instantApi.openapi.download')}
            </Button>
          </div>
          <Button onClick={onClose}>{t('instantApi.openapi.close')}</Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
