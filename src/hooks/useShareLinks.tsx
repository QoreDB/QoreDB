// SPDX-License-Identifier: Apache-2.0

import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import { openUrl } from '@tauri-apps/plugin-opener';
import { useCallback, useEffect, useRef } from 'react';
import { useTranslation } from 'react-i18next';
import { toast } from 'sonner';
import { ExportProgressToast } from '@/components/Export/ExportProgressToast';
import {
  cancelExport,
  type ExportConfig,
  exportProgressEvent,
  type ExportProgress,
  type ExportState,
  startExport,
} from '@/lib/export';
import { notify } from '@/lib/notify';
import {
  shareCleanupExport,
  sharePrepareExport,
  shareSnapshot as shareSnapshotCommand,
  shareUploadPreparedExport,
  type ShareSnapshotRequest,
} from '@/lib/share';
import {
  getShareProviderSettings,
  isShareProviderConfigured,
  toShareProviderConfig,
} from '@/lib/shareSettings';
import type { Namespace } from '@/lib/tauri';

type ActiveExport = {
  unlisten: UnlistenFn;
};

const ACTIVE_STATES: ExportState[] = ['pending', 'running'];

export interface ShareLiveExportRequest {
  query: string;
  namespace?: Namespace;
  file_name: string;
  format: ExportConfig['format'];
  include_headers: boolean;
  table_name?: string;
  batch_size?: number;
  limit?: number;
}

function extensionForFormat(format: ShareLiveExportRequest['format']): string {
  switch (format) {
    case 'json':
      return 'json';
    case 'sql_insert':
      return 'sql';
    case 'html':
      return 'html';
    case 'xlsx':
      return 'xlsx';
    case 'parquet':
      return 'parquet';
    default:
      return 'csv';
  }
}

export function useShareLinks(sessionId?: string) {
  const { t } = useTranslation();
  const activeExportsRef = useRef<Map<string, ActiveExport>>(new Map());

  const cleanupExport = useCallback((exportId: string) => {
    const active = activeExportsRef.current.get(exportId);
    if (active) {
      active.unlisten();
      activeExportsRef.current.delete(exportId);
    }
  }, []);

  const showExportToast = useCallback(
    (progress: ExportProgress) => {
      const isActive = ACTIVE_STATES.includes(progress.state);

      toast.custom(
        () => (
          <ExportProgressToast
            progress={progress}
            onCancel={
              isActive
                ? () => {
                    cancelExport(progress.export_id).catch(error => {
                      notify.error(t('export.cancelFailed'), error);
                    });
                  }
                : undefined
            }
          />
        ),
        {
          id: `share-export:${progress.export_id}`,
          duration: isActive ? Infinity : 4000,
        }
      );

      if (!isActive) {
        cleanupExport(progress.export_id);
      }
    },
    [cleanupExport, t]
  );

  const resolveProvider = useCallback(() => {
    const settings = getShareProviderSettings();
    if (!isShareProviderConfigured(settings)) {
      notify.error(t('share.providerRequired'));
      return null;
    }
    return toShareProviderConfig(settings);
  }, [t]);

  const finalizeShare = useCallback(
    async (shareUrl: string) => {
      let copied = false;
      try {
        await navigator.clipboard.writeText(shareUrl);
        copied = true;
      } catch {
        copied = false;
      }

      notify.success(copied ? t('share.successCopied') : t('share.success'), {
        description: shareUrl,
        duration: 10000,
        action: {
          label: t('share.open'),
          onClick: () => {
            void openUrl(shareUrl);
          },
        },
      });

      return shareUrl;
    },
    [t]
  );

  const startShareExport = useCallback(
    async (request: ShareLiveExportRequest) => {
      if (!sessionId) {
        notify.error(t('share.noSession'));
        return null;
      }

      const provider = resolveProvider();
      if (!provider) return null;

      const prepared = await sharePrepareExport(
        request.file_name,
        extensionForFormat(request.format)
      ).catch(error => {
        notify.error(t('share.prepareFailed'), error);
        return null;
      });
      if (!prepared) return null;

      let sawProgress = false;
      let resolveCompletion: (() => void) | null = null;
      let rejectCompletion: ((error: Error) => void) | null = null;

      const completion = new Promise<void>((resolve, reject) => {
        resolveCompletion = resolve;
        rejectCompletion = reject;
      });

      try {
        const unlisten = await listen<ExportProgress>(
          exportProgressEvent(prepared.share_id),
          event => {
            sawProgress = true;
            showExportToast(event.payload);

            if (event.payload.state === 'completed') {
              resolveCompletion?.();
            } else if (event.payload.state === 'failed') {
              rejectCompletion?.(new Error(event.payload.error ?? t('share.exportFailed')));
            } else if (event.payload.state === 'cancelled') {
              rejectCompletion?.(new Error(t('share.exportCancelled')));
            }
          }
        );

        activeExportsRef.current.set(prepared.share_id, { unlisten });

        const exportConfig: ExportConfig = {
          query: request.query,
          namespace: request.namespace,
          output_path: prepared.output_path,
          format: request.format,
          table_name: request.table_name,
          include_headers: request.include_headers,
          batch_size: request.batch_size,
          limit: request.limit,
        };

        const response = await startExport(sessionId, exportConfig, prepared.share_id);

        if (!sawProgress) {
          showExportToast({
            export_id: response.export_id,
            state: 'pending',
            rows_exported: 0,
            bytes_written: 0,
            elapsed_ms: 0,
          });
        }

        await completion;
        notify.info(t('share.uploading'));

        const upload = await shareUploadPreparedExport(response.export_id, provider);
        return await finalizeShare(upload.share_url);
      } catch (error) {
        notify.error(t('share.failed'), error);
        await shareCleanupExport(prepared.share_id).catch(() => {});
        return null;
      } finally {
        cleanupExport(prepared.share_id);
      }
    },
    [cleanupExport, finalizeShare, resolveProvider, sessionId, showExportToast, t]
  );

  const shareSnapshot = useCallback(
    async (request: Omit<ShareSnapshotRequest, 'provider'>) => {
      const provider = resolveProvider();
      if (!provider) return null;

      try {
        const response = await shareSnapshotCommand({
          ...request,
          provider,
        });
        return await finalizeShare(response.share_url);
      } catch (error) {
        notify.error(t('share.failed'), error);
        return null;
      }
    },
    [finalizeShare, resolveProvider, t]
  );

  useEffect(
    () => () => {
      for (const exportEntry of activeExportsRef.current.values()) {
        exportEntry.unlisten();
      }
      activeExportsRef.current.clear();
    },
    []
  );

  return {
    startShareExport,
    shareSnapshot,
  };
}
