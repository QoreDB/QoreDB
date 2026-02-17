// SPDX-License-Identifier: Apache-2.0

import { useCallback, useEffect, useRef } from 'react';
import { listen, UnlistenFn } from '@tauri-apps/api/event';
import { useTranslation } from 'react-i18next';
import { toast } from 'sonner';

import {
  cancelExport,
  exportProgressEvent,
  ExportConfig,
  ExportProgress,
  ExportState,
  startExport,
} from '@/lib/export';
import { ExportProgressToast } from '@/components/Export/ExportProgressToast';

type ActiveExport = {
  unlisten: UnlistenFn;
};

const ACTIVE_STATES: ExportState[] = ['pending', 'running'];

function getErrorMessage(error: unknown): string {
  if (typeof error === 'string') return error;
  if (error instanceof Error) return error.message;
  if (error && typeof error === 'object' && 'message' in error) {
    return String((error as { message: unknown }).message);
  }
  return 'Unknown error';
}

export function useStreamingExport(sessionId?: string) {
  const { t } = useTranslation();
  const activeExportsRef = useRef<Map<string, ActiveExport>>(new Map());

  const cleanupExport = useCallback((exportId: string) => {
    const active = activeExportsRef.current.get(exportId);
    if (active) {
      active.unlisten();
      activeExportsRef.current.delete(exportId);
    }
  }, []);

  const showToast = useCallback(
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
                      toast.error(t('export.cancelFailed'), {
                        description: getErrorMessage(error),
                      });
                    });
                  }
                : undefined
            }
          />
        ),
        {
          id: `export:${progress.export_id}`,
          duration: isActive ? Infinity : 4000,
        }
      );

      if (!isActive) {
        cleanupExport(progress.export_id);
      }
    },
    [cleanupExport, t]
  );

  const startStreamingExport = useCallback(
    async (config: ExportConfig) => {
      if (!sessionId) {
        toast.error(t('export.noSession'));
        return null;
      }

      const exportId =
        crypto.randomUUID?.() ?? `${Date.now()}-${Math.random().toString(16).slice(2)}`;

      try {
        let sawProgress = false;
        const unlisten = await listen<ExportProgress>(exportProgressEvent(exportId), event => {
          sawProgress = true;
          showToast(event.payload);
        });

        activeExportsRef.current.set(exportId, { unlisten });
        const response = await startExport(sessionId, config, exportId);

        if (!sawProgress) {
          showToast({
            export_id: response.export_id,
            state: 'pending',
            rows_exported: 0,
            bytes_written: 0,
            elapsed_ms: 0,
          });
        }

        return response.export_id;
      } catch (error) {
        cleanupExport(exportId);
        toast.error(t('export.startFailed'), {
          description: getErrorMessage(error),
        });
        return null;
      }
    },
    [sessionId, showToast, t]
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
    startStreamingExport,
  };
}
