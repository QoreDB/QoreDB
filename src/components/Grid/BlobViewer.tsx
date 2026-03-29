// SPDX-License-Identifier: Apache-2.0

import { save } from '@tauri-apps/plugin-dialog';
import { writeFile } from '@tauri-apps/plugin-fs';
import { Binary, Copy, Download, Eye, FileCode } from 'lucide-react';
import { useCallback, useMemo, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { toast } from 'sonner';

import {
  type ImageDetection,
  MAX_DECODE_SIZE,
  MAX_HEX_DUMP_BYTES,
  MAX_PREVIEW_SIZE,
  base64ToUint8Array,
  detectImageType,
  estimateByteSizeFromBase64,
  formatFileSize,
  formatHexDump,
  getDataUri,
} from '@/lib/binaryUtils';
import { Button } from '@/components/ui/button';
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog';
import { ScrollArea } from '@/components/ui/scroll-area';

type Tab = 'hex' | 'base64' | 'preview';

interface BlobViewerProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  /** Base64-encoded binary value */
  value: string;
  /** Column name for display */
  columnName: string;
  /** Database column type (e.g., "bytea", "blob", "varbinary(255)") */
  dataType: string;
}

const TABS: Array<{ id: Tab; labelKey: string; Icon: typeof FileCode }> = [
  { id: 'hex', labelKey: 'blobViewer.hex', Icon: FileCode },
  { id: 'base64', labelKey: 'blobViewer.base64', Icon: Binary },
  { id: 'preview', labelKey: 'blobViewer.preview', Icon: Eye },
];

export function BlobViewer({ open, onOpenChange, value, columnName, dataType }: BlobViewerProps) {
  const { t } = useTranslation();
  const [activeTab, setActiveTab] = useState<Tab>('hex');

  const byteSize = useMemo(() => estimateByteSizeFromBase64(value), [value]);
  const imageInfo = useMemo(() => detectImageType(value), [value]);
  const canPreview = imageInfo !== null && byteSize <= MAX_PREVIEW_SIZE;
  const isTooLarge = byteSize > MAX_DECODE_SIZE;
  const isTruncated = byteSize > MAX_HEX_DUMP_BYTES;

  // Lazy decode: only decode the first MAX_HEX_DUMP_BYTES for hex view
  const hexDump = useMemo(() => {
    if (activeTab !== 'hex') return '';
    const bytes = base64ToUint8Array(value, MAX_HEX_DUMP_BYTES);
    return formatHexDump(bytes, MAX_HEX_DUMP_BYTES);
  }, [value, activeTab]);

  const handleCopyBase64 = useCallback(async () => {
    try {
      await navigator.clipboard.writeText(value);
      toast.success(t('blobViewer.copyBase64'));
    } catch {
      toast.error(t('blobViewer.clipboardError'));
    }
  }, [value, t]);

  const handleCopyHex = useCallback(async () => {
    try {
      const bytes = base64ToUint8Array(value, MAX_HEX_DUMP_BYTES);
      const dump = formatHexDump(bytes, MAX_HEX_DUMP_BYTES);
      await navigator.clipboard.writeText(dump);
      toast.success(t('blobViewer.copyHex'));
    } catch {
      toast.error(t('blobViewer.clipboardError'));
    }
  }, [value, t]);

  const handleDownload = useCallback(async () => {
    if (isTooLarge) {
      toast.error(t('blobViewer.tooLarge'));
      return;
    }
    try {
      const ext = imageInfo?.type ?? 'bin';
      const filePath = await save({
        defaultPath: `${columnName}.${ext}`,
        filters: [{ name: ext.toUpperCase(), extensions: [ext] }],
      });

      if (!filePath) return;

      const bytes = base64ToUint8Array(value);
      await writeFile(filePath, bytes);
      toast.success(t('blobViewer.downloadSuccess'));
    } catch (err) {
      console.error('Download failed:', err);
      toast.error(t('blobViewer.downloadError'), {
        description: err instanceof Error ? err.message : String(err),
      });
    }
  }, [value, columnName, imageInfo, isTooLarge, t]);

  const handleTabKeyDown = useCallback((e: React.KeyboardEvent, tabIdx: number) => {
    let nextIdx = -1;
    if (e.key === 'ArrowRight') nextIdx = (tabIdx + 1) % TABS.length;
    else if (e.key === 'ArrowLeft') nextIdx = (tabIdx - 1 + TABS.length) % TABS.length;

    if (nextIdx >= 0) {
      e.preventDefault();
      setActiveTab(TABS[nextIdx].id);
      // Focus the next tab button
      const nextButton = document.getElementById(`blob-tab-${TABS[nextIdx].id}`);
      nextButton?.focus();
    }
  }, []);

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="max-w-3xl max-h-[80vh] flex flex-col">
        <DialogHeader>
          <DialogTitle className="flex items-center gap-2 font-mono text-sm">
            <Binary className="h-4 w-4 text-muted-foreground" />
            {columnName}
          </DialogTitle>
          <DialogDescription className="flex items-center gap-3 text-xs">
            <span>
              {t('blobViewer.type')}: <code className="text-foreground">{dataType}</code>
            </span>
            <span>
              {t('blobViewer.size')}:{' '}
              <code className="text-foreground">{formatFileSize(byteSize)}</code>
            </span>
            {imageInfo && (
              <span className="text-accent">
                {imageInfo.type.toUpperCase()} {t('blobViewer.imagePreview').toLowerCase()}
              </span>
            )}
            {isTooLarge && (
              <span className="text-destructive font-medium">{t('blobViewer.tooLarge')}</span>
            )}
          </DialogDescription>
        </DialogHeader>

        {/* Accessible tab bar */}
        <div
          role="tablist"
          aria-label={t('blobViewer.title')}
          className="flex gap-1 border-b border-border pb-0"
        >
          {TABS.map((tab, idx) => (
            <button
              type="button"
              key={tab.id}
              id={`blob-tab-${tab.id}`}
              role="tab"
              aria-selected={activeTab === tab.id}
              aria-controls={`blob-tabpanel-${tab.id}`}
              tabIndex={activeTab === tab.id ? 0 : -1}
              onClick={() => setActiveTab(tab.id)}
              onKeyDown={e => handleTabKeyDown(e, idx)}
              className={`
                flex items-center gap-1.5 px-3 py-1.5 text-xs font-medium rounded-t-md
                transition-colors border-b-2 -mb-px
                ${
                  activeTab === tab.id
                    ? 'border-accent text-accent bg-accent/5'
                    : 'border-transparent text-muted-foreground hover:text-foreground hover:bg-muted/30'
                }
              `}
            >
              <tab.Icon className="h-3.5 w-3.5" />
              {t(tab.labelKey)}
            </button>
          ))}
        </div>

        {/* Tab panels */}
        <div className="flex-1 min-h-0">
          {activeTab === 'hex' && (
            <div role="tabpanel" id="blob-tabpanel-hex" aria-labelledby="blob-tab-hex">
              <ScrollArea className="h-[400px] rounded-md border border-border bg-muted/20">
                <pre className="font-mono text-xs leading-5 p-3 select-text whitespace-pre">
                  {hexDump}
                </pre>
                {isTruncated && (
                  <div className="px-3 pb-3 text-xs text-muted-foreground italic">
                    {t('blobViewer.truncated', {
                      size: formatFileSize(MAX_HEX_DUMP_BYTES),
                      total: formatFileSize(byteSize),
                    })}
                  </div>
                )}
              </ScrollArea>
            </div>
          )}

          {activeTab === 'base64' && (
            <div role="tabpanel" id="blob-tabpanel-base64" aria-labelledby="blob-tab-base64">
              <ScrollArea className="h-[400px] rounded-md border border-border bg-muted/20">
                {isTooLarge ? (
                  <div className="p-4 text-sm text-muted-foreground italic text-center">
                    {t('blobViewer.tooLarge')} ({formatFileSize(byteSize)})
                  </div>
                ) : (
                  <pre className="font-mono text-xs leading-5 p-3 select-text whitespace-pre-wrap break-all">
                    {value}
                  </pre>
                )}
              </ScrollArea>
            </div>
          )}

          {activeTab === 'preview' && (
            <div
              role="tabpanel"
              id="blob-tabpanel-preview"
              aria-labelledby="blob-tab-preview"
              className="h-[400px] rounded-md border border-border bg-muted/20 flex items-center justify-center overflow-auto"
            >
              {canPreview && imageInfo ? (
                <ImagePreview base64={value} imageInfo={imageInfo} />
              ) : (
                <div className="text-sm text-muted-foreground italic p-4 text-center">
                  {byteSize > MAX_PREVIEW_SIZE
                    ? t('blobViewer.truncated', {
                        size: formatFileSize(MAX_PREVIEW_SIZE),
                        total: formatFileSize(byteSize),
                      })
                    : t('blobViewer.noPreview')}
                </div>
              )}
            </div>
          )}
        </div>

        <DialogFooter className="flex-row gap-2 sm:justify-between">
          <div className="flex gap-2">
            <Button variant="outline" size="sm" onClick={handleCopyBase64} disabled={isTooLarge}>
              <Copy className="h-3.5 w-3.5 mr-1.5" />
              Base64
            </Button>
            <Button variant="outline" size="sm" onClick={handleCopyHex}>
              <Copy className="h-3.5 w-3.5 mr-1.5" />
              Hex
            </Button>
          </div>
          <div className="flex gap-2">
            <Button variant="outline" size="sm" onClick={handleDownload} disabled={isTooLarge}>
              <Download className="h-3.5 w-3.5 mr-1.5" />
              {t('blobViewer.download')}
            </Button>
            <Button variant="ghost" size="sm" onClick={() => onOpenChange(false)}>
              {t('common.close')}
            </Button>
          </div>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}

function ImagePreview({ base64, imageInfo }: { base64: string; imageInfo: ImageDetection }) {
  const dataUri = getDataUri(base64, imageInfo.mime);

  return (
    <div className="p-4 flex flex-col items-center gap-2">
      <img
        src={dataUri}
        alt="Binary data preview"
        className="max-h-[350px] max-w-full object-contain rounded shadow-sm"
      />
      <span className="text-xs text-muted-foreground">{imageInfo.type.toUpperCase()}</span>
    </div>
  );
}
