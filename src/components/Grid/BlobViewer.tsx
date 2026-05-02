// SPDX-License-Identifier: Apache-2.0

import { Binary, Copy, Download, ExternalLink, Eye, FileCode, Link2 } from 'lucide-react';
import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { useTranslation } from 'react-i18next';

import { AnalyticsService } from '@/components/Onboarding/AnalyticsService';
import { Button } from '@/components/ui/button';
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog';
import { ScrollArea } from '@/components/ui/scroll-area';
import {
  base64ToUint8Array,
  detectBlobKind,
  estimateByteSizeFromBase64,
  formatFileSize,
  formatHexDump,
  MAX_DECODE_SIZE,
  MAX_HEX_DUMP_BYTES,
  MAX_PREVIEW_SIZE,
  sizeBucket,
} from '@/lib/binaryUtils';
import { ImagePreview } from './blob/ImagePreview';
import { SvgSourceView } from './blob/SvgSourceView';
import { useBlobActions } from './blob/useBlobActions';

type Tab = 'preview' | 'svgSource' | 'hex' | 'base64';

interface TabSpec {
  id: Tab;
  labelKey: string;
  Icon: typeof FileCode;
}

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

export function BlobViewer({ open, onOpenChange, value, columnName, dataType }: BlobViewerProps) {
  const { t } = useTranslation();
  const openTrackedRef = useRef(false);

  const byteSize = useMemo(() => estimateByteSizeFromBase64(value), [value]);
  const blobKind = useMemo(() => detectBlobKind(value), [value]);
  const canPreview = blobKind !== null && byteSize <= MAX_PREVIEW_SIZE;
  const isTooLarge = byteSize > MAX_DECODE_SIZE;
  const isTruncated = byteSize > MAX_HEX_DUMP_BYTES;
  const isSvg = blobKind?.kind === 'svg';

  const tabs = useMemo(() => {
    const list: TabSpec[] = [];
    if (blobKind) list.push({ id: 'preview', labelKey: 'blobViewer.preview', Icon: Eye });
    if (isSvg) list.push({ id: 'svgSource', labelKey: 'blobViewer.svgSource', Icon: FileCode });
    list.push({ id: 'hex', labelKey: 'blobViewer.hex', Icon: FileCode });
    list.push({ id: 'base64', labelKey: 'blobViewer.base64', Icon: Binary });
    return list;
  }, [blobKind, isSvg]);

  const [activeTab, setActiveTab] = useState<Tab>(() => tabs[0].id);

  useEffect(() => {
    if (open) setActiveTab(tabs[0].id);
  }, [open, tabs]);

  const actions = useBlobActions({
    value,
    byteSize,
    columnName,
    dataType,
    blobKind,
    isTooLarge,
  });

  useEffect(() => {
    if (!open) {
      openTrackedRef.current = false;
      return;
    }
    if (openTrackedRef.current) return;
    openTrackedRef.current = true;
    AnalyticsService.capture('blob_viewer_opened', {
      column_type: dataType,
      size_bucket: sizeBucket(byteSize),
      detected_kind: blobKind ? (blobKind.kind === 'svg' ? 'svg' : blobKind.type) : 'unknown',
    });
  }, [open, blobKind, byteSize, dataType]);

  const hexDump = useMemo(() => {
    if (activeTab !== 'hex') return '';
    const bytes = base64ToUint8Array(value, MAX_HEX_DUMP_BYTES);
    return formatHexDump(bytes, MAX_HEX_DUMP_BYTES);
  }, [value, activeTab]);

  const handleTabKeyDown = useCallback(
    (e: React.KeyboardEvent, tabIdx: number) => {
      let nextIdx = -1;
      if (e.key === 'ArrowRight') nextIdx = (tabIdx + 1) % tabs.length;
      else if (e.key === 'ArrowLeft') nextIdx = (tabIdx - 1 + tabs.length) % tabs.length;

      if (nextIdx >= 0) {
        e.preventDefault();
        const next = tabs[nextIdx];
        setActiveTab(next.id);
        document.getElementById(`blob-tab-${next.id}`)?.focus();
      }
    },
    [tabs]
  );

  const kindLabel = blobKind
    ? blobKind.kind === 'svg'
      ? 'SVG'
      : blobKind.type.toUpperCase()
    : null;

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="max-w-3xl max-h-[80vh] flex flex-col">
        <DialogHeader>
          <DialogTitle className="flex items-center gap-2 font-mono text-sm">
            <Binary className="h-4 w-4 text-muted-foreground" />
            {columnName}
          </DialogTitle>
          <DialogDescription className="text-xs flex items-center gap-2">
            <code className="text-foreground">{dataType}</code>
            <span className="text-muted-foreground">·</span>
            <span>{formatFileSize(byteSize)}</span>
            {kindLabel && (
              <>
                <span className="text-muted-foreground">·</span>
                <span className="text-accent font-medium">{kindLabel}</span>
              </>
            )}
            {isTooLarge && (
              <span className="ml-auto text-destructive font-medium">
                {t('blobViewer.tooLarge')}
              </span>
            )}
          </DialogDescription>
        </DialogHeader>

        <div className="flex items-end justify-between gap-2 border-b border-border">
          <div role="tablist" aria-label={t('blobViewer.title')} className="flex gap-1">
            {tabs.map((tab, idx) => (
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
          <div className="flex items-center gap-1 pb-1">
            {blobKind && (
              <Button
                variant="ghost"
                size="sm"
                onClick={actions.copyDataUri}
                disabled={isTooLarge}
                title={t('blobViewer.copyDataUri')}
                aria-label={t('blobViewer.copyDataUri')}
              >
                <Link2 className="h-3.5 w-3.5" />
              </Button>
            )}
            <Button
              variant="ghost"
              size="sm"
              onClick={actions.openExternal}
              disabled={isTooLarge}
              title={t('blobViewer.openExternal')}
              aria-label={t('blobViewer.openExternal')}
            >
              <ExternalLink className="h-3.5 w-3.5" />
            </Button>
            <Button
              variant="ghost"
              size="sm"
              onClick={actions.download}
              disabled={isTooLarge}
              title={t('blobViewer.download')}
              aria-label={t('blobViewer.download')}
            >
              <Download className="h-3.5 w-3.5" />
            </Button>
          </div>
        </div>

        <div className="flex-1 min-h-0">
          {activeTab === 'preview' && (
            <div
              role="tabpanel"
              id="blob-tabpanel-preview"
              aria-labelledby="blob-tab-preview"
              className="h-[400px] rounded-md border border-border bg-muted/20 flex items-center justify-center overflow-auto"
            >
              {canPreview && blobKind ? (
                <ImagePreview base64={value} blobKind={blobKind} />
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

          {activeTab === 'svgSource' && isSvg && (
            <div role="tabpanel" id="blob-tabpanel-svgSource" aria-labelledby="blob-tab-svgSource">
              <SvgSourceView base64={value} />
            </div>
          )}

          {activeTab === 'hex' && (
            <div role="tabpanel" id="blob-tabpanel-hex" aria-labelledby="blob-tab-hex">
              <div className="flex justify-end pb-1">
                <Button variant="ghost" size="sm" onClick={actions.copyHex} className="h-6 px-2">
                  <Copy className="h-3 w-3 mr-1" />
                  <span className="text-xs">Hex</span>
                </Button>
              </div>
              <ScrollArea className="h-[370px] rounded-md border border-border bg-muted/20">
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
              <div className="flex justify-end pb-1">
                <Button
                  variant="ghost"
                  size="sm"
                  onClick={actions.copyBase64}
                  disabled={isTooLarge}
                  className="h-6 px-2"
                >
                  <Copy className="h-3 w-3 mr-1" />
                  <span className="text-xs">Base64</span>
                </Button>
              </div>
              <ScrollArea className="h-[370px] rounded-md border border-border bg-muted/20">
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
        </div>
      </DialogContent>
    </Dialog>
  );
}
